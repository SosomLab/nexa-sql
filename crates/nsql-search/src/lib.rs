//! `nsql-search` — 파일 검색 엔진(docs/36 §2 · T-81b). GUI 파일 검색 패널(T-81a)과 CLI `nsql grep`이 같은 코어를 쓴다(DR-6).
//!
//! - **열거**: 병렬 폴더 걷기([`walk`] · std 스레드 풀 · 폴더 단위 큐 · 링크 루프 차단) · 무시 규칙([`ignore`] · 기본 + `.gitignore` 부분집합) ·
//!   이진 제외(첫 8KB NUL) · 크기 상한 `max_file_kb`.
//! - **제외 로그**: 건너뛴 파일은 전부 [`Batch::Skipped`]로 **이유와 함께**([`SkipReason`] — 크기 초과 · 이진 · 읽기 실패 · UTF-8 아닌 이름)
//!   같은 채널에 실려 온다(워커가 여럿이어도 채널이 하나라 로그는 도착 순으로 **병합**된다 · 사용자 09-23 "제외된 파일과 이유를 검색 로그에").
//! - **매칭**: [`Matcher`] — 리터럴(SWAR 후보 스캔) · 대소문자 무시(ASCII 빠른 길 + 비ASCII `char` 경로) · 단어 단위 · 정규식(`regex` · 리터럴 접두 사전 필터).
//! - **줄 번호**: 일치 파일에서만 계산([`lines`]) · 앞뒤 문맥 1줄.
//! - **인코딩**: UTF-8 그대로 · UTF-16 BOM 변환 · EUC-KR은 T-79 뒤.
//! - **스트리밍**: `mpsc` 채널로 파일 단위 [`Batch`](≤ [`BATCH_MAX`] 일치) · 취소 [`CancelHandle`] · 진행 통계 [`Progress`].
//! - **열린 탭**: [`search_text`] — 편집기 버퍼(저장 안 된 본문)를 같은 매처로.
//!
//! 외부 crate = `regex`만(D-76 예외) · 네트워크 0 · 자원 = 스레드 `threads`(0 = 코어 수 · 상한 16) · 파일당 읽기 1회.
//!
//! ```no_run
//! use nsql_search::{Batch, Search, SearchOpts};
//! let opts = SearchOpts { query: "SELECT".into(), roots: vec!["/proj".into()], ..SearchOpts::default() };
//! let (rx, cancel) = Search::spawn(opts).expect("regex ok");
//! for b in rx {
//!     match b {
//!         Batch::Matches(ms) => println!("{}: {}", ms[0].path.display(), ms.len()),
//!         Batch::Error { path, message } => eprintln!("{}: {message}", path.display()),
//!         Batch::Skipped { path, reason } => eprintln!("skipped {}: {}", path.display(), reason.code()),
//!         Batch::Done(p) => println!("{} files · {} matches · {:?}", p.files_searched, p.matches, p.elapsed),
//!     }
//! }
//! let _ = cancel;
//! ```

mod bytes;
mod decode;
mod ignore;
mod lines;
mod matcher;
mod walk;

pub use ignore::DEFAULT_EXCLUDES;
pub use matcher::{Error, Matcher};
pub use walk::MAX_THREADS;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 한 배치의 최대 일치 수(파일 단위 · 큰 파일은 여러 배치).
pub const BATCH_MAX: usize = 64;

/// 검색 옵션. 설정 키 후보(nsql-settings): `search.max_file_kb`(1024) · `search.threads`(0) · `search.gitignore`(true) · `search.excludes`.
#[derive(Debug, Clone)]
pub struct SearchOpts {
    /// 검색어(리터럴 또는 `regex`가 참이면 정규식).
    pub query: String,
    /// 대소문자 구분.
    pub case: bool,
    /// 단어 단위(ASCII 영숫자·`_`·비ASCII = 단어 문자).
    pub word: bool,
    /// `query`를 정규식으로(`regex` crate 문법 · 여러 줄 모드).
    pub regex: bool,
    /// 검색 루트(폴더 또는 파일 · 여러 개 · 겹치면 한 번만).
    pub roots: Vec<PathBuf>,
    /// 추가 제외 패턴(gitignore 문법 · 루트 기준) — 기본 [`DEFAULT_EXCLUDES`]에 더한다.
    pub excludes: Vec<String>,
    /// 파일 크기 상한(KB · 0 = 무제한).
    pub max_file_kb: usize,
    /// 워커 스레드 수(0 = 코어 수 · 상한 [`MAX_THREADS`]).
    pub threads: usize,
    /// `.gitignore` 적용(D-55 기본 예).
    pub gitignore: bool,
}

impl Default for SearchOpts {
    fn default() -> Self {
        SearchOpts {
            query: String::new(),
            case: false,
            word: false,
            regex: false,
            roots: Vec::new(),
            excludes: Vec::new(),
            max_file_kb: 1024,
            threads: 0,
            gitignore: true,
        }
    }
}

/// 일치 하나.
#[derive(Debug, Clone)]
pub struct Match {
    /// 파일 경로(루트에서 이어 붙인 형태) · [`search_text`]에서는 호출자가 준 라벨.
    pub path: Arc<Path>,
    /// 1부터.
    pub line_no: usize,
    /// 줄 안의 바이트 열(1부터 · ripgrep `--column` 규약).
    pub col: usize,
    /// `line` 안의 일치 바이트 범위(강조용 · 여러 줄에 걸친 정규식 일치는 줄 끝에서 자른다).
    pub span: (usize, usize),
    /// 일치 줄(개행·`\r` 제외 · 잘못된 UTF-8은 U+FFFD).
    pub line: Arc<str>,
    /// 앞 줄 문맥(첫 줄이면 `None`).
    pub before: Option<Arc<str>>,
    /// 뒤 줄 문맥(마지막 줄이면 `None`).
    pub after: Option<Arc<str>>,
}

/// 진행 통계(검색 중 [`CancelHandle::progress`] · 끝에 [`Batch::Done`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// 열거되어 읽기 대상이 된 파일 수.
    pub files_seen: u64,
    /// 실제로 매칭한 파일 수(이진·읽기 실패 제외).
    pub files_searched: u64,
    /// 일치가 있는 파일 수.
    pub matches_files: u64,
    /// 일치 수.
    pub matches: u64,
    /// 폴더 읽기 실패 수([`Batch::Error`]).
    pub errors: u64,
    /// 건너뛴 파일 수([`Batch::Skipped`] · 크기·이진·읽기 실패·이름).
    pub skipped: u64,
    pub elapsed: Duration,
    pub cancelled: bool,
    pub done: bool,
}

impl Progress {
    /// 일치가 있는 파일 수(옛 이름 호환).
    #[must_use]
    pub fn files_matched(&self) -> u64 {
        self.matches_files
    }
}

/// 파일을 건너뛴 이유(검색 로그 · 사용자 09-23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// 크기 상한 초과(`bytes` > `limit` · `max_file_kb`).
    TooLarge { bytes: u64, limit: u64 },
    /// 이진 파일(첫 8KB에 NUL · 텍스트 검색 불가).
    Binary,
    /// 열기/읽기 실패(권한 · 잠김 · 사라짐).
    Unreadable(String),
    /// UTF-8이 아닌 이름(무시 규칙을 맞출 수 없어 건너뜀).
    NonUtf8Name,
}

impl SkipReason {
    /// 짧은 코드(로그 · CLI).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            SkipReason::TooLarge { .. } => "too_large",
            SkipReason::Binary => "binary",
            SkipReason::Unreadable(_) => "unreadable",
            SkipReason::NonUtf8Name => "non_utf8_name",
        }
    }
}

/// 채널로 오는 조각.
#[derive(Debug, Clone)]
pub enum Batch {
    /// 한 파일의 일치(같은 `path` · 오름차순 · ≤ [`BATCH_MAX`]).
    Matches(Vec<Match>),
    /// 폴더 읽기 실패 — 검색은 계속된다.
    Error { path: PathBuf, message: String },
    /// 파일 하나를 건너뛰었다(이유 포함) — 검색 로그. 워커가 여럿이어도 채널 하나라 도착 순으로 병합된다.
    Skipped { path: PathBuf, reason: SkipReason },
    /// 끝(취소 포함) · 마지막 메시지.
    Done(Progress),
}

#[derive(Debug)]
struct Shared {
    cancel: AtomicBool,
    done: AtomicBool,
    files_seen: AtomicU64,
    files_searched: AtomicU64,
    files_matched: AtomicU64,
    matches: AtomicU64,
    errors: AtomicU64,
    skipped: AtomicU64,
    started: Instant,
}

impl Shared {
    fn snapshot(&self) -> Progress {
        Progress {
            files_seen: self.files_seen.load(Ordering::Relaxed),
            files_searched: self.files_searched.load(Ordering::Relaxed),
            matches_files: self.files_matched.load(Ordering::Relaxed),
            matches: self.matches.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
            elapsed: self.started.elapsed(),
            cancelled: self.cancel.load(Ordering::Relaxed),
            done: self.done.load(Ordering::Relaxed),
        }
    }
}

/// 취소 토큰 + 진행 조회(복제 가능 · UI 스레드가 든다).
#[derive(Debug, Clone)]
pub struct CancelHandle {
    shared: Arc<Shared>,
}

impl CancelHandle {
    /// 취소 — 워커는 다음 폴더/파일/256 일치 단위에서 멈추고 [`Batch::Done`]을 보낸다.
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.shared.cancel.load(Ordering::Relaxed)
    }

    /// 지금까지의 통계(잠금 없음 · 16ms 틱마다 읽어도 비용 0).
    #[must_use]
    pub fn progress(&self) -> Progress {
        self.shared.snapshot()
    }
}

/// 검색 시작점.
#[derive(Debug)]
pub struct Search;

impl Matcher {
    /// 옵션에서 매처를(정규식 오류는 `Err`).
    pub fn from_opts(o: &SearchOpts) -> Result<Matcher, Error> {
        Matcher::new(&o.query, o.case, o.word, o.regex)
    }
}

impl Search {
    /// 백그라운드에서 검색을 시작한다. 정규식/빈 검색어 오류는 즉시 `Err`. 수신자를 버리면 검색은 스스로 취소된다.
    pub fn spawn(opts: SearchOpts) -> Result<(Receiver<Batch>, CancelHandle), Error> {
        let matcher = Matcher::from_opts(&opts)?;
        let (tx, rx) = mpsc::channel::<Batch>();
        let shared = Arc::new(Shared {
            cancel: AtomicBool::new(false),
            done: AtomicBool::new(false),
            files_seen: AtomicU64::new(0),
            files_searched: AtomicU64::new(0),
            files_matched: AtomicU64::new(0),
            matches: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            started: Instant::now(),
        });
        let handle = CancelHandle {
            shared: Arc::clone(&shared),
        };
        let builder = std::thread::Builder::new().name("nsql-search".into());
        builder
            .spawn(move || run(opts, matcher, tx, shared))
            .map_err(|e| Error::Regex(format!("spawn: {e}")))?;
        Ok((rx, handle))
    }
}

/// 코디네이터 스레드 본체 — 워커를 만들고 끝나면 `Done`.
fn run(opts: SearchOpts, matcher: Matcher, tx: Sender<Batch>, shared: Arc<Shared>) {
    let wopts = walk::WalkOpts {
        threads: opts.threads,
        gitignore: opts.gitignore,
        excludes: opts.excludes.clone(),
    };
    let max_bytes = (opts.max_file_kb as u64).saturating_mul(1024);
    let send = |b: Batch| {
        if tx.send(b).is_err() {
            // 수신자가 사라졌다(창 닫힘) → 자기 취소.
            shared.cancel.store(true, Ordering::Relaxed);
        }
    };
    let skip = |path: PathBuf, reason: SkipReason| {
        shared.skipped.fetch_add(1, Ordering::Relaxed);
        send(Batch::Skipped { path, reason });
    };
    let visit = |path: PathBuf| search_file(path, max_bytes, &matcher, &shared, &send, &skip);
    let error = |path: PathBuf, message: String| {
        shared.errors.fetch_add(1, Ordering::Relaxed);
        send(Batch::Error { path, message });
    };
    walk::walk(
        &opts.roots,
        &wopts,
        &shared.cancel,
        &walk::Hooks {
            visit: &visit,
            error: &error,
            skip: &skip,
        },
    );
    shared.done.store(true, Ordering::Relaxed);
    let _ = tx.send(Batch::Done(shared.snapshot()));
}

thread_local! {
    /// 워커별 읽기 버퍼(파일마다 할당하지 않는다).
    static BUF: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// 파일 하나 — 열기 → `fstat` 크기 상한 → 첫 8KB 이진 판정(이진이면 나머지를 읽지 않는다) → 읽기 → BOM 처리 → 매칭 →
/// 일치 파일만 줄 계산 → 배치 전송. `max_bytes` 0 = 무제한. 건너뛰면 이유와 함께 `skip`(검색 로그).
fn search_file(
    path: PathBuf,
    max_bytes: u64,
    matcher: &Matcher,
    shared: &Shared,
    send: &dyn Fn(Batch),
    skip: &dyn Fn(PathBuf, SkipReason),
) {
    use std::io::Read;
    BUF.with(|cell| {
        let mut raw = cell.borrow_mut();
        raw.clear();
        let read = std::fs::File::open(&path).and_then(|mut f| {
            let len = f.metadata()?.len();
            if max_bytes != 0 && len > max_bytes {
                return Ok(Some(SkipReason::TooLarge {
                    bytes: len,
                    limit: max_bytes,
                }));
            }
            shared.files_seen.fetch_add(1, Ordering::Relaxed);
            raw.reserve(len as usize);
            (&mut f)
                .take(decode::BINARY_PROBE as u64)
                .read_to_end(&mut raw)?;
            if decode::probe_is_binary(&raw) {
                return Ok(Some(SkipReason::Binary));
            }
            f.read_to_end(&mut raw)?;
            Ok(None)
        });
        match read {
            Ok(None) => {}
            Ok(Some(reason)) => {
                skip(path, reason);
                return;
            }
            Err(e) => {
                skip(path, SkipReason::Unreadable(e.to_string()));
                return;
            }
        }
        let Some(body) = decode::prepare(&raw) else {
            skip(path, SkipReason::Binary); // BOM 뒤 판정에서 이진
            return;
        };
        shared.files_searched.fetch_add(1, Ordering::Relaxed);
        let mut spans = Vec::new();
        matcher.find_all(&body, &mut spans, &mut || {
            !shared.cancel.load(Ordering::Relaxed)
        });
        if spans.is_empty() {
            return;
        }
        shared.files_matched.fetch_add(1, Ordering::Relaxed);
        shared
            .matches
            .fetch_add(spans.len() as u64, Ordering::Relaxed);
        let path: Arc<Path> = Arc::from(path);
        let mut batch = Vec::with_capacity(spans.len().min(BATCH_MAX));
        lines::resolve(&path, &body, &spans, &mut |m| {
            batch.push(m);
            if batch.len() == BATCH_MAX {
                send(Batch::Matches(std::mem::take(&mut batch)));
            }
            !shared.cancel.load(Ordering::Relaxed)
        });
        if !batch.is_empty() {
            send(Batch::Matches(batch));
        }
    });
}

/// 메모리 본문 검색(열린 탭 · 저장 안 된 버퍼). `label`은 결과의 `path`에 그대로(파일 탭이면 경로 · 메모리 탭이면 제목).
#[must_use]
pub fn search_text(text: &str, label: impl AsRef<Path>, matcher: &Matcher) -> Vec<Match> {
    let hay = text.as_bytes();
    let mut spans = Vec::new();
    matcher.find_all(hay, &mut spans, &mut || true);
    let path: Arc<Path> = Arc::from(label.as_ref());
    let mut out = Vec::with_capacity(spans.len());
    lines::resolve(&path, hay, &spans, &mut |m| {
        out.push(m);
        true
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicUsize;

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    /// 테스트별 임시 트리(테스트 이름 + 순번 · 끝나면 지운다).
    struct Tree(PathBuf);
    impl Tree {
        fn new(tag: &str) -> Tree {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let p =
                std::env::temp_dir().join(format!("nsql-search-{tag}-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).expect("mkdir");
            Tree(p)
        }
        fn file(&self, rel: &str, body: &[u8]) -> PathBuf {
            let p = self.0.join(rel);
            if let Some(d) = p.parent() {
                fs::create_dir_all(d).expect("mkdir");
            }
            fs::write(&p, body).expect("write");
            p
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn opts(t: &Tree, q: &str) -> SearchOpts {
        SearchOpts {
            query: q.into(),
            roots: vec![t.0.clone()],
            threads: 2,
            ..SearchOpts::default()
        }
    }

    /// 전부 받아 (일치 · 오류 · 끝 통계)로.
    fn collect(o: SearchOpts) -> (Vec<Match>, Vec<PathBuf>, Progress) {
        let (ms, errs, _, p) = collect_all(o);
        (ms, errs, p)
    }

    /// 전부 받아 (일치 · 폴더 오류 · 제외 로그 · 끝 통계)로.
    #[allow(clippy::type_complexity)]
    fn collect_all(
        o: SearchOpts,
    ) -> (
        Vec<Match>,
        Vec<PathBuf>,
        Vec<(PathBuf, SkipReason)>,
        Progress,
    ) {
        let (rx, _h) = Search::spawn(o).expect("spawn");
        let mut ms = Vec::new();
        let mut errs = Vec::new();
        let mut skipped = Vec::new();
        let mut done = None;
        for b in rx {
            match b {
                Batch::Matches(v) => ms.extend(v),
                Batch::Error { path, .. } => errs.push(path),
                Batch::Skipped { path, reason } => skipped.push((path, reason)),
                Batch::Done(p) => done = Some(p),
            }
        }
        (ms, errs, skipped, done.expect("Done은 반드시 온다"))
    }

    /// 제외 로그: 큰 파일·이진은 이유와 함께 `Skipped`로 오고 통계 `skipped`에 세어진다 · 일치·오류와 같은 채널(병합).
    #[test]
    fn skipped_files_are_logged_with_reason() {
        let t = Tree::new("skip");
        t.file("ok.sql", b"SELECT 1;\n");
        t.file("big.sql", &vec![b'S'; 3000]);
        t.file("bin.dat", b"SELECT\0binary\n");
        let mut o = opts(&t, "SELECT");
        o.max_file_kb = 2;
        let (ms, errs, skipped, p) = collect_all(o);
        assert!(errs.is_empty());
        assert_eq!(ms.len(), 1);
        let mut log: Vec<(String, &'static str)> = skipped
            .iter()
            .map(|(pth, r)| {
                (
                    pth.strip_prefix(&t.0)
                        .expect("under root")
                        .to_string_lossy()
                        .into_owned(),
                    r.code(),
                )
            })
            .collect();
        log.sort();
        assert_eq!(
            log,
            vec![
                ("big.sql".into(), "too_large"),
                ("bin.dat".into(), "binary")
            ]
        );
        assert!(matches!(
            skipped
                .iter()
                .find(|(p, _)| p.ends_with("big.sql"))
                .map(|(_, r)| r),
            Some(SkipReason::TooLarge {
                bytes: 3000,
                limit: 2048
            })
        ));
        assert_eq!(p.skipped, 2);
        assert_eq!(p.files_searched, 1);
    }

    fn rel(t: &Tree, m: &Match) -> String {
        m.path
            .strip_prefix(&t.0)
            .expect("under root")
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[test]
    fn ignore_rules_default_gitignore_and_binary() {
        let t = Tree::new("ignore");
        t.file("a.sql", b"SELECT 1;\n");
        t.file("sub/b.sql", b"select 2;\nSELECT 3;\n");
        t.file("target/c.sql", b"SELECT skipped\n");
        t.file(".git/d.sql", b"SELECT skipped\n");
        t.file("node_modules/e.sql", b"SELECT skipped\n");
        t.file("logs/x.log", b"SELECT logged\n");
        t.file("logs/keep.log", b"SELECT kept\n");
        t.file("sub/gen/z.sql", b"SELECT generated\n");
        t.file("bin.dat", b"SELECT\0binary\n");
        t.file(".gitignore", b"*.log\n!keep.log\n");
        t.file("sub/.gitignore", b"gen/\n");
        let (ms, errs, p) = collect(opts(&t, "SELECT"));
        assert!(errs.is_empty());
        let mut files: Vec<String> = ms.iter().map(|m| rel(&t, m)).collect();
        files.sort();
        files.dedup();
        assert_eq!(files, vec!["a.sql", "logs/keep.log", "sub/b.sql"]);
        assert_eq!(p.matches, 4, "대소문자 무시 기본");
        assert_eq!(p.files_matched(), 3);
        assert!(p.done && !p.cancelled);
        assert_eq!(
            p.files_searched,
            p.files_seen - 1,
            "이진 1개는 매칭에서 제외"
        );

        // .gitignore 끄기 + 사용자 제외.
        let mut o = opts(&t, "SELECT");
        o.gitignore = false;
        o.excludes = vec!["sub/".into()];
        let (ms, _, _) = collect(o);
        let mut files: Vec<String> = ms.iter().map(|m| rel(&t, m)).collect();
        files.sort();
        files.dedup();
        assert_eq!(files, vec!["a.sql", "logs/keep.log", "logs/x.log"]);
    }

    #[test]
    fn case_word_regex_and_context() {
        let t = Tree::new("modes");
        t.file(
            "q.sql",
            b"-- header\nSELECT emp_id FROM emp\nselect * from employees\nWHERE emp = 1\n",
        );
        let mut o = opts(&t, "emp");
        o.case = true;
        o.word = true;
        let (ms, _, p) = collect(o);
        assert_eq!(p.matches, 2);
        assert_eq!((ms[0].line_no, ms[0].col), (2, 20));
        assert_eq!(ms[0].before.as_deref(), Some("-- header"));
        assert_eq!(ms[0].after.as_deref(), Some("select * from employees"));
        assert_eq!(&ms[0].line[ms[0].span.0..ms[0].span.1], "emp");
        assert_eq!((ms[1].line_no, ms[1].col), (4, 7));
        assert_eq!(ms[1].after, None);

        let mut o = opts(&t, r"^select\s+\*");
        o.regex = true;
        let (ms, _, _) = collect(o);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].line_no, 3);

        let mut o = opts(&t, "(");
        o.regex = true;
        assert!(matches!(Search::spawn(o), Err(Error::Regex(_))));
        assert!(matches!(
            Search::spawn(opts(&t, "")),
            Err(Error::EmptyQuery)
        ));
    }

    #[test]
    fn utf16_bom_and_size_limit_and_file_root() {
        let t = Tree::new("enc");
        let mut le = vec![0xFF, 0xFE];
        for u in "한글 SELECT\n".encode_utf16() {
            le.extend_from_slice(&u.to_le_bytes());
        }
        t.file("u16.sql", &le);
        let big = t.file("big.sql", &vec![b'x'; 3000]);
        let (ms, _, _) = collect(opts(&t, "select"));
        assert_eq!(ms.len(), 1);
        assert_eq!(&*ms[0].line, "한글 SELECT");
        // 2KB 상한 → big.sql 제외.
        let mut o = opts(&t, "x");
        o.max_file_kb = 2;
        let (_, _, p) = collect(o);
        assert_eq!(p.files_seen, 1);
        // 루트가 파일이면 그 파일만.
        let mut o = opts(&t, "xxx");
        o.roots = vec![big];
        let (_, _, p) = collect(o);
        assert_eq!((p.files_seen, p.matches), (1, 1000));
    }

    #[test]
    fn streaming_order_batches_and_done_last() {
        let t = Tree::new("stream");
        let body: String = (1..=150).map(|i| format!("row {i} hit\n")).collect();
        t.file("many.txt", body.as_bytes());
        t.file("one.txt", b"hit\n");
        let mut o = opts(&t, "hit");
        o.threads = 1;
        let (rx, _h) = Search::spawn(o).expect("spawn");
        let all: Vec<Batch> = rx.iter().collect();
        assert!(matches!(all.last(), Some(Batch::Done(_))), "Done이 마지막");
        let mut sizes = Vec::new();
        let mut last_path: Option<Arc<Path>> = None;
        let mut seen_paths: Vec<Arc<Path>> = Vec::new();
        for b in &all {
            if let Batch::Matches(v) = b {
                assert!(!v.is_empty() && v.len() <= BATCH_MAX);
                assert!(
                    v.iter().all(|m| Arc::ptr_eq(&m.path, &v[0].path)),
                    "배치 = 한 파일"
                );
                assert!(
                    v.windows(2).all(|w| w[0].line_no <= w[1].line_no),
                    "오름차순"
                );
                if last_path
                    .as_ref()
                    .is_none_or(|p| !Arc::ptr_eq(p, &v[0].path))
                {
                    assert!(
                        !seen_paths.iter().any(|p| **p == *v[0].path),
                        "파일 배치는 연속"
                    );
                    seen_paths.push(Arc::clone(&v[0].path));
                    last_path = Some(Arc::clone(&v[0].path));
                }
                sizes.push(v.len());
            }
        }
        assert_eq!(sizes.iter().sum::<usize>(), 151);
        assert!(
            sizes.contains(&64),
            "150개 파일은 64·64·22로 쪼개진다: {sizes:?}"
        );
    }

    #[test]
    fn cancel_stops_early_and_still_sends_done() {
        let t = Tree::new("cancel");
        for i in 0..200 {
            t.file(&format!("d{}/f{i}.txt", i % 10), b"needle needle\n");
        }
        let mut o = opts(&t, "needle");
        o.threads = 1;
        let (rx, h) = Search::spawn(o).expect("spawn");
        h.cancel();
        let all: Vec<Batch> = rx.iter().collect();
        let Some(Batch::Done(p)) = all.last() else {
            panic!("Done")
        };
        assert!(p.cancelled && p.done);
        assert!(h.progress().cancelled);
        assert!(
            p.files_seen < 200,
            "취소 뒤 전부 읽지 않는다: {}",
            p.files_seen
        );
    }

    #[test]
    fn missing_root_reports_error_and_finishes() {
        let t = Tree::new("missing");
        let mut o = opts(&t, "x");
        o.roots.push(t.0.join("nope"));
        let (_, errs, p) = collect(o);
        assert_eq!(errs.len(), 1);
        assert_eq!(p.errors, 1);
        assert!(p.done);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loop_is_visited_once() {
        let t = Tree::new("loop");
        t.file("a/x.txt", b"loop\n");
        std::os::unix::fs::symlink(&t.0, t.0.join("a/back")).expect("symlink");
        let (ms, _, p) = collect(opts(&t, "loop"));
        assert_eq!(ms.len(), 1);
        assert_eq!(p.files_seen, 1);
    }

    #[test]
    fn search_text_for_open_tabs() {
        let m = Matcher::new("from", false, true, false).expect("m");
        let v = search_text("SELECT *\nFROM emp\n-- fromage\n", "Script_3", &m);
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].line_no, v[0].col), (2, 1));
        assert_eq!(v[0].path.to_str(), Some("Script_3"));
        assert_eq!(v[0].before.as_deref(), Some("SELECT *"));
        assert_eq!(v[0].after.as_deref(), Some("-- fromage"));
    }

    /// 벤치 — 이 저장소 전체에서 `cargo`(대소문자 무시) · `cargo test -p nsql-search -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bench_repo_cargo() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (label, o) in [
            (
                "literal -i",
                SearchOpts {
                    query: "cargo".into(),
                    roots: vec![root.clone()],
                    ..SearchOpts::default()
                },
            ),
            (
                "literal case",
                SearchOpts {
                    query: "cargo".into(),
                    case: true,
                    roots: vec![root.clone()],
                    ..SearchOpts::default()
                },
            ),
            (
                "regex",
                SearchOpts {
                    query: r"cargo\s+\w+".into(),
                    regex: true,
                    roots: vec![root.clone()],
                    ..SearchOpts::default()
                },
            ),
            (
                "literal 1 thread",
                SearchOpts {
                    query: "cargo".into(),
                    threads: 1,
                    roots: vec![root],
                    ..SearchOpts::default()
                },
            ),
        ] {
            let best = (0..3)
                .map(|_| collect(o.clone()).2)
                .min_by_key(|p| p.elapsed)
                .expect("runs");
            eprintln!(
                "{label:16} files_seen={} searched={} matched={} matches={} elapsed={:?}",
                best.files_seen,
                best.files_searched,
                best.files_matched(),
                best.matches,
                best.elapsed
            );
        }
    }
}
