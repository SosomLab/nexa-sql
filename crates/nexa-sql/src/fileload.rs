//! **파일 적재 — 탭 안에 격리된 진행 막**(사용자 09-20): 기준 이상 파일은 작업 스레드가 읽고 풀고 **편집기 본문까지 준비**한다.
//!
//! - **격리**: 적재는 그 파일의 **자리 탭** 안에서만 보인다(비어 있고 읽기 전용 · 경로 없음). 진행 막은 그 탭이 활성일 때만
//!   편집 영역 가운데에 뜨고, 100%가 된 뒤에 편집 화면으로 바뀐다. **다른 탭 추가·편집·실행은 그대로 된다**
//!   (처음에는 "적재 중 모든 입력 차단"이었으나 같은 날 뒤집혔다 — 차단 범위 = 적재 중인 탭 하나).
//! - **기준 둘**: ① 크기 ≥ `file.async_load_mb`(8 MB) = 스레드 적재 ② 그 적재가 `file.load_progress_ms`(300 ms) 넘게
//!   걸리면 막을 보인다 — 빠른 디스크에서 0.1초 만에 끝나는 파일에 막이 번쩍이지 않게(지연 표시).
//! - **진척도는 실제 값**: 읽은 바이트 ÷ 크기가 0~80% · 글자 풀이 85% · 편집기 본문 준비(글자 버퍼 · 줄 표) 92% · 받은 뒤 100%.
//!   무거운 일을 전부 스레드에서 끝내므로 UI 스레드는 옮겨 받기만 한다(65 MB에서 0.5초 멎던 것이 없어진다).
//! - 이 파일 = 진행 상태(원자값) · 읽기 · 순수 판정 · 그리기. 배선(언제 시작·수거·취소)은 호스트(`main.rs`).

use crate::{enc, eol};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::Rect;
use nexa_ctl::theme::Theme;
use nsql_i18n::{t, tf, Msg};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, Instant};

/// 한 번에 읽는 덩어리 — 진척도 갱신·취소 확인의 단위(1 MB = 빠른 SSD에서 1 ms 안쪽 · 느린 공유 폴더에서도 0.1초 안쪽).
const CHUNK: usize = 1 << 20;

/// 적재 단계.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Reading,
    Decoding,
    /// 편집기 본문 준비(글자 버퍼 · 줄 표) — 스레드에서.
    Preparing,
    /// 스레드는 끝났다 — 호스트가 탭에 옮겨 넣을 차례.
    Opening,
}

/// 스레드와 UI가 나눠 보는 진행 상태(잠금 없음).
#[derive(Debug, Default)]
pub(crate) struct Progress {
    read: AtomicU64,
    stage: AtomicU8,
    cancel: AtomicBool,
}

impl Progress {
    pub(crate) fn stage(&self) -> Stage {
        match self.stage.load(Ordering::Relaxed) {
            0 => Stage::Reading,
            1 => Stage::Decoding,
            2 => Stage::Preparing,
            _ => Stage::Opening,
        }
    }

    fn set_stage(&self, s: Stage) {
        self.stage.store(s as u8, Ordering::Relaxed);
    }

    pub(crate) fn read(&self) -> u64 {
        self.read.load(Ordering::Relaxed)
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// (본문 `\n` 정규화, 줄끝, 실제 인코딩, 대체 문자 발생, 잘렸는가).
pub(crate) type FileText = (String, eol::Eol, &'static str, bool, bool);

/// 적재 결과의 본문 — 편집기에 실을 것은 **준비본**(글자 버퍼 + 줄 표까지 스레드에서) · 실행만 할 것은 문자열.
pub(crate) enum Body {
    Text(String),
    Prepared(nexa_ctl::PreparedText),
}

/// 적재 결과(본문 외).
pub(crate) struct Loaded {
    pub(crate) body: Body,
    pub(crate) eol: eol::Eol,
    pub(crate) used: &'static str,
    pub(crate) lossy: bool,
    pub(crate) truncated: bool,
}

/// 읽기 + 풀기 + (`prepare`면) 편집기 본문 준비 — 스레드에서도, 작은 파일의 동기 경로에서도 같은 함수다.
pub(crate) fn load(
    path: &Path,
    enc: &str,
    limit: Option<u64>,
    prepare: bool,
    prog: Option<&Progress>,
) -> Result<Loaded, String> {
    let (text, eol, used, lossy, truncated) = read_file(path, enc, limit, prog)?;
    let body = if prepare {
        if let Some(p) = prog {
            p.set_stage(Stage::Preparing);
        }
        let b = Body::Prepared(nexa_ctl::PreparedText::new(text));
        if prog.is_some_and(Progress::cancelled) {
            return Err(CANCELLED.into());
        }
        b
    } else {
        Body::Text(text)
    };
    if let Some(p) = prog {
        p.set_stage(Stage::Opening);
    }
    Ok(Loaded {
        body,
        eol,
        used,
        lossy,
        truncated,
    })
}

/// 적재 중인 탭에서 막는 명령 — **그 탭의 본문을 쓰는 것**(편집 · 찾기/바꾸기 · 실행 · 저장)만. 새 탭·파일 열기·보기·창·접속은
/// 그대로 된다(다른 탭으로 가면 이 판정 자체를 하지 않는다). `edit.prefs`(설정 창)는 본문과 무관하다.
pub(crate) fn blocked_while_loading(id: &str) -> bool {
    if id == "edit.prefs" {
        return false;
    }
    ["edit.", "find.", "run."].iter().any(|p| id.starts_with(p))
        || matches!(
            id,
            "file.save" | "file.save_as" | "file.follow" | "file.large_force"
        )
}

/// 취소된 적재의 오류 글(호스트는 이 값이면 오류 안내 대신 "취소됨"만 알린다).
pub(crate) const CANCELLED: &str = "\u{0}cancelled";

/// 파일 읽기 + 인코딩 풀기 + 줄끝 판정(스레드에서도 부른다 · `limit` = 앞부분만 — 마지막 줄바꿈에서 자른다).
/// `prog`가 있으면 덩어리마다 읽은 양을 올리고 취소를 본다.
pub(crate) fn read_file(
    path: &Path,
    enc: &str,
    limit: Option<u64>,
    prog: Option<&Progress>,
) -> Result<FileText, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let total = f.metadata().map(|m| m.len()).unwrap_or(0);
    let want = limit.map_or(total, |n| n.min(total));
    let mut bytes: Vec<u8> = Vec::with_capacity(want as usize + 1);
    let mut chunk = vec![0u8; CHUNK.min(want as usize + 1).max(4096)];
    while (bytes.len() as u64) < want {
        if prog.is_some_and(Progress::cancelled) {
            return Err(CANCELLED.into());
        }
        let room = ((want - bytes.len() as u64) as usize).min(chunk.len());
        let n = f.read(&mut chunk[..room]).map_err(|e| e.to_string())?;
        if n == 0 {
            break; // 읽는 사이 파일이 줄었다 — 읽은 데까지만.
        }
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(p) = prog {
            p.read.store(bytes.len() as u64, Ordering::Relaxed);
        }
    }
    drop(chunk);
    let mut truncated = false;
    if limit.is_some_and(|n| total > n) {
        truncated = true;
        if let Some(p) = bytes.iter().rposition(|b| *b == b'\n') {
            bytes.truncate(p + 1);
        }
    }
    if let Some(p) = prog {
        p.set_stage(Stage::Decoding);
    }
    // 소유권 판 = 온전한 UTF-8 · LF 파일이면 읽은 버퍼가 그대로 본문이 된다(사본 0 — 종전에는 사본이 셋 생겼다).
    let (text, lossy, used) = enc::decode_owned(bytes, enc);
    let (eol, text) = eol::detect_owned(text);
    if prog.is_some_and(Progress::cancelled) {
        return Err(CANCELLED.into());
    }
    Ok((text, eol, used, lossy, truncated))
}

/// 진척도(0.0~1.0) — 읽기 = 0~0.80(바이트 비율) · 풀기 = 0.85 · 본문 준비 = 0.92 · 받은 뒤 = 1.0.
pub(crate) fn fraction(stage: Stage, read: u64, total: u64) -> f32 {
    match stage {
        Stage::Reading => {
            if total == 0 {
                0.0
            } else {
                (read.min(total) as f64 / total as f64 * 0.80) as f32
            }
        }
        Stage::Decoding => 0.85,
        Stage::Preparing => 0.92,
        Stage::Opening => 1.0,
    }
}

/// 막을 보일 때인가 — 적재가 `delay`보다 오래 걸렸을 때만(빠르게 끝나는 파일은 막 없이 바로 편집 화면).
pub(crate) fn overlay_due(elapsed: Duration, delay: Duration) -> bool {
    elapsed >= delay
}

/// 진행 막에 그릴 값.
pub(crate) struct View<'a> {
    pub(crate) name: &'a str,
    pub(crate) stage: Stage,
    pub(crate) read: u64,
    pub(crate) total: u64,
    pub(crate) started: Instant,
    /// 기다리는 파일이 더 있으면 그 수(여러 개를 한꺼번에 열 때).
    pub(crate) more: usize,
}

/// 편집 영역 `area`를 덮고 가운데에 카드(파일 이름 · 막대 · % · 읽은 양 · 단계 · Esc 안내)를 그린다.
pub(crate) fn paint(dc: &mut dyn DrawCtx, th: &Theme, area: Rect, scale: f32, v: &View<'_>) {
    if area.w <= 0 || area.h <= 0 {
        return;
    }
    let sc = |x: f32| (x * scale).round() as i32;
    // 편집 영역 전체를 덮는다 — 아래의 (다른 탭) 본문이 "지금 편집할 수 있는 것"처럼 보이지 않게.
    dc.fill_rect_alpha(area, th.panel_bg, 0.86);
    let frac = fraction(v.stage, v.read, v.total).clamp(0.0, 1.0);
    let pct = format!("{}%", (frac * 100.0).round() as i32);
    let title = tf(Msg::LoadTitle, &[v.name]);
    let stage = match v.stage {
        Stage::Reading => t(Msg::LoadReading),
        Stage::Decoding => t(Msg::LoadDecoding),
        Stage::Preparing => t(Msg::LoadPreparing),
        Stage::Opening => t(Msg::LoadOpening),
    };
    let amount = format!(
        "{} / {}",
        nsql_core::fmt_bytes(v.read.min(v.total)),
        nsql_core::fmt_bytes(v.total)
    );
    let secs = v.started.elapsed().as_secs();
    let mut foot = tf(Msg::VeilElapsed, &[&secs.to_string()]);
    if v.more > 0 {
        foot = format!("{foot} · {}", tf(Msg::LoadMore, &[&v.more.to_string()]));
    }
    let hint = t(Msg::LoadCancelHint);

    dc.select_font(FontSlot::Base, true);
    let lh = dc.text_height();
    let tw = dc.text_width(&title);
    dc.select_font(FontSlot::Base, false);
    let pad = sc(20.0);
    let cw = (tw + pad * 2)
        .max(sc(360.0))
        .min(area.w - pad * 2)
        .max(sc(200.0));
    let bar_h = sc(8.0);
    let ch = pad + lh + sc(12.0) + bar_h + sc(8.0) + lh + sc(6.0) + lh + sc(10.0) + lh + pad / 2;
    let card = Rect::new(
        area.x + (area.w - cw) / 2,
        area.y + (area.h - ch) / 2,
        cw,
        ch,
    );
    dc.fill_round_rect(card, sc(8.0), th.field_bg);
    dc.stroke_round_rect(card, sc(8.0), th.accent, 1.0);
    let clip = Rect::new(card.x + pad / 2, card.y, card.w - pad, card.h);
    let inner_x = card.x + pad;
    let inner_w = card.w - pad * 2;
    let mut y = card.y + pad;
    // 제목(굵게 · 가운데).
    dc.select_font(FontSlot::Base, true);
    let w = dc.text_width(&title);
    dc.text(
        card.x + ((card.w - w) / 2).max(pad / 2),
        y,
        clip,
        &title,
        th.text,
    );
    y += lh + sc(12.0);
    // 막대: 바탕 + 채움.
    let track = Rect::new(inner_x, y, inner_w, bar_h);
    dc.fill_round_rect(track, bar_h / 2, th.border);
    let fill_w = ((inner_w as f32) * frac).round() as i32;
    if fill_w > 0 {
        dc.fill_round_rect(
            Rect::new(inner_x, y, fill_w.max(bar_h), bar_h),
            bar_h / 2,
            th.accent,
        );
    }
    y += bar_h + sc(8.0);
    // 왼쪽 = 단계 · 오른쪽 = %.
    dc.select_font(FontSlot::Base, false);
    dc.text(inner_x, y, clip, stage, th.accent);
    dc.select_font(FontSlot::Base, true);
    let pw = dc.text_width(&pct);
    dc.text(inner_x + inner_w - pw, y, clip, &pct, th.text);
    y += lh + sc(6.0);
    // 읽은 양 · 경과.
    dc.select_font(FontSlot::Base, false);
    dc.text(inner_x, y, clip, &amount, th.text_dim);
    let fw = dc.text_width(&foot);
    dc.text(inner_x + inner_w - fw, y, clip, &foot, th.text_dim);
    y += lh + sc(10.0);
    let hw = dc.text_width(hint);
    dc.text(card.x + (card.w - hw) / 2, y, clip, hint, th.text_dim);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 진척도: 읽기 = 바이트 비율의 90%까지 · 풀기 95% · 열기 100% · 뒤로 가지 않는다(단계 순서대로 단조 증가).
    #[test]
    fn fraction_is_monotonic_over_stages() {
        assert_eq!(fraction(Stage::Reading, 0, 100), 0.0);
        assert!((fraction(Stage::Reading, 50, 100) - 0.40).abs() < 1e-6);
        assert!((fraction(Stage::Reading, 100, 100) - 0.80).abs() < 1e-6);
        // 파일이 읽는 사이 커져 read > total이어도 80%를 넘지 않는다 · 크기 0 = 0%.
        assert!((fraction(Stage::Reading, 500, 100) - 0.80).abs() < 1e-6);
        assert_eq!(fraction(Stage::Reading, 0, 0), 0.0);
        let seq = [
            fraction(Stage::Reading, 10, 100),
            fraction(Stage::Reading, 100, 100),
            fraction(Stage::Decoding, 100, 100),
            fraction(Stage::Preparing, 100, 100),
            fraction(Stage::Opening, 100, 100),
        ];
        assert!(seq.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(seq[4], 1.0);
    }

    /// 적재 중인 탭에서 막는 명령 = 그 탭의 본문을 쓰는 것만(새 탭·열기·보기·접속·설정은 통과).
    #[test]
    fn loading_tab_blocks_only_content_commands() {
        for id in [
            "edit.undo",
            "edit.paste",
            "find.replace",
            "run.all",
            "run.stmt",
            "file.save",
            "file.save_as",
            "file.large_force",
        ] {
            assert!(blocked_while_loading(id), "{id}");
        }
        for id in [
            "file.new",
            "file.open",
            "file.run_file",
            "file.exit",
            "edit.prefs",
            "view.log",
            "view.palette",
            "conn.open",
            "tab.next",
            "mem.trim_now",
            "file.load_cancel",
        ] {
            assert!(!blocked_while_loading(id), "{id}");
        }
    }

    /// 막은 지연 뒤에만 — 경계값 포함.
    #[test]
    fn overlay_waits_for_delay() {
        let d = Duration::from_millis(300);
        assert!(!overlay_due(Duration::from_millis(299), d));
        assert!(overlay_due(Duration::from_millis(300), d));
        assert!(
            overlay_due(Duration::ZERO, Duration::ZERO),
            "지연 0 = 곧바로"
        );
    }

    /// 읽기: 덩어리 읽기가 통째 읽기와 같은 본문을 준다 · 진행값 = 파일 크기 · 단계 = 열기 · `limit` = 마지막 줄바꿈에서 자름.
    #[test]
    fn chunked_read_matches_and_reports() {
        let dir = std::env::temp_dir().join(format!("nsql-fileload-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("a.sql");
        let mut body = String::new();
        for i in 0..60_000 {
            body.push_str(&format!("select {i} from dual; -- 한글 주석\r\n"));
        }
        std::fs::write(&p, &body).expect("write temp file");
        let prog = Progress::default();
        let (text, eol, _, lossy, truncated) =
            read_file(&p, "utf-8", None, Some(&prog)).expect("read");
        assert_eq!(text, body.replace("\r\n", "\n"));
        assert_eq!(eol, eol::Eol::Crlf);
        assert!(!lossy && !truncated);
        assert_eq!(prog.read(), body.len() as u64);
        assert_eq!(prog.stage(), Stage::Decoding, "단계의 끝은 load가 올린다");
        // load: 준비본 = 같은 본문 · 줄 수 · 단계 = 열기. 실행용 = 문자열 그대로.
        let prog = Progress::default();
        let l = load(&p, "utf-8", None, true, Some(&prog)).expect("load");
        assert_eq!(prog.stage(), Stage::Opening);
        match l.body {
            Body::Prepared(pt) => {
                assert_eq!(pt.text(), text);
                assert_eq!(pt.lines(), 60_001);
            }
            Body::Text(_) => panic!("prepared expected"),
        }
        assert!(matches!(
            load(&p, "utf-8", None, false, None).expect("load").body,
            Body::Text(t) if t == text
        ));
        // 앞부분만: 잘림 표시 + 온전한 줄로 끝난다.
        let (head, _, _, _, cut) = read_file(&p, "utf-8", Some(1000), None).expect("read head");
        assert!(cut && head.ends_with('\n') && head.len() <= 1000);
        // 취소: 읽기 전에 걸어 두면 본문 없이 끝난다.
        let prog = Progress::default();
        prog.cancel();
        assert_eq!(
            read_file(&p, "utf-8", None, Some(&prog)).expect_err("cancelled"),
            CANCELLED
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
