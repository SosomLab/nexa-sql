//! 프로젝트(워크스페이스) 모델(docs/67 §2 · T-165 P1 · 사용자 09-22) — `<이름>.nsql-project`(JSON · **어디에나** · VCS에 올림).
//!
//! ```json
//! { "version": 1, "folders": [ { "path": "sql" }, { "path": "D:/other" } ] }
//! ```
//! 폴더 경로는 프로젝트 파일이 있는 폴더 기준 **상대**(그 아래일 때) · 아니면 절대. 읽을 때는 파일 위치로 되돌린다.
//! 파서·작성기 = `nsql-settings::json`(외부 crate 0 · DR-3). 접속 정보는 담지 않는다(D-149 ①).

use nsql_settings::json::{dump as json_dump, parse, Json};
use std::path::{Path, PathBuf};

pub(crate) const EXT: &str = "nsql-project";
/// 2 = 등록 폴더·홈 기준 경로 앵커(`${workspaceFolder:이름}/…` · `${userHome}/…` · T-172 P2 · 11-22) — 1은 그대로 읽는다.
const VERSION: i64 = 2;
/// 3 = 경로 변수(`"vars"` + `${var:이름}/…` · T-172 P3 · 11-22) — **변수를 쓰는 파일만** 3으로 적는다(변수를 모르는 판이 열어 저장하면
/// `vars`가 사라지므로 그 판은 열지 않게) · 변수가 없으면 2 그대로.
const VERSION_VARS: i64 = 3;

/// 열린 프로젝트 — `path` = 프로젝트 파일(없으면 "프로젝트 없음" = 기본 워크스페이스 · D-148).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Project {
    pub path: Option<PathBuf>,
    /// 탐색기 루트 폴더(절대 · 순서 = 표시 순서).
    pub folders: Vec<PathBuf>,
    /// ★ T-172 (A)(D-494): 프로젝트 고유 번호(파일 안 `id` · 복사·이동해도 따라간다) — PC별 폴더 대응표의 열쇠. 없으면 필요할 때 만든다.
    pub id: Option<String>,
    /// ★ T-172 (A): `folders`와 나란히 — 이 PC가 **다른 자리**를 쓰는 폴더의 정의 경로(프로젝트 파일에 적힌 것). 저장할 때는
    ///   이것을 적고 앵커 이름도 이것에서 얻는다(이 PC의 경로가 공유 파일에 새지 않는다). 짧을 수 있다(없음 = `None`).
    pub folder_src: Vec<Option<PathBuf>>,
    /// ★ T-172 P2(73 §4-1 `folders[].name`): 등록 폴더의 **적어 둔 이름**(정의 경로 → 이름 · 프로젝트 파일의 `"name"`) — 탭 경로 앵커
    ///   `${workspaceFolder:이름}`의 이름으로 폴더 이름(leaf) 대신 쓴다(leaf가 겹치는 두 폴더도 이름을 적으면 앵커를 쓸 수 있다).
    ///   순번이 아니라 정의 경로가 열쇠다(폴더 순서를 바꾸거나 다중 인스턴스가 목록을 다시 짜도 어긋나지 않는다) · 지금은 파일을
    ///   직접 고쳐 적는다(화면에서 이름을 바꾸는 명령은 없다) · 목록에 없는 폴더의 항목은 저장 때 버린다.
    pub folder_names: Vec<(PathBuf, String)>,
    /// ★ T-172 P3(73 §4-2): 경로 변수 = (이름, **적힌 글**) — 프로젝트 파일 `"vars": { "DATA": "${userHome}/data" }` · 값은 프로젝트
    ///   폴더 기준 상대·홈 앵커·절대(다른 변수·등록 폴더는 가리키지 않는다). 탭·폴더 경로가 그 아래면 `${var:DATA}/…`로 적힌다.
    ///   지금은 파일을 직접 고쳐 적는다(설정 표 · CLI · 이 PC만의 값 = 다음 단계) · 적힌 글을 그대로 보존한다.
    pub path_vars: Vec<(String, String)>,
    /// ★ T-172 P3 2단계: 경로 변수의 **이 PC만의 자리**(이름, 폴더) — 프로젝트 파일에는 적지 않는다(설정 `project.pc_paths`의
    ///   `var:이름` 열쇠 · D-494). 공유 정의(`path_vars`)가 없거나 그 자리가 이 PC에 없을 때 대신 쓴다 · 저장할 때 그 아래 경로는
    ///   `${var:이름}/…`로 적힌다(이 PC의 실제 경로가 공유 파일에 새지 않는다).
    pub var_pc: Vec<(String, PathBuf)>,
    /// 탐색기에서 마지막으로 고른 항목(절대 · 다시 열 때 그 자리에 선택 · 사용자 09-22).
    pub last_selected: Option<PathBuf>,
    /// ★ 작업 환경(사용자 09-23): 편집기 탭들(순서 · 파일 경로 또는 미저장 스크립트 본문 · 캐럿 + fuzzy 앵커) · 활성 탭 · 북마크(JSON).
    pub tabs: Vec<TabState>,
    pub active: usize,
    pub bookmarks: Option<String>,
    /// ★ 좌측 패널 상태(사용자 09-23 "각 좌측 기능별로 복원"): 프로젝트 탐색기에서 **펼친 폴더**(절대 · 저장은 상대) ·
    /// 보이던 좌측 패널(`project`/`bookmarks`/`search`/`ext`/`explorer`) · 파일 검색어 · 북마크 패널에서 접은 그룹 id ·
    /// 마지막 접속 프로필 이름(**표식만** — 복원 때 재접속하지 않는다 · docs/70 §2).
    pub expanded: Vec<PathBuf>,
    pub panel: Option<String>,
    pub search: Option<String>,
    pub bm_collapsed: Vec<u32>,
    pub profiles: Vec<String>,
    /// ★ T-39b 프로젝트 스코프 설정(10-30 · docs/67 §8 · D-406·D-407): 헤더 `settings` 절 = (구문 묶음 `Some("sql")` | 프로젝트 전체 `None`,
    ///   키, 원시 값 글자). 모르는 키도 **보존**(새 판 파일을 옛 판이 열어도 안 지움) · 적용은 `nsql_settings::project_allowed` 키만.
    pub settings: Vec<(Option<String>, String, String)>,
}

/// 편집기 탭 하나의 저장 상태(docs/67 §2-4 · 사용자 09-23).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TabState {
    /// 파일 탭이면 경로(파일 기준 상대로 저장) · 없으면 미저장 스크립트.
    pub path: Option<PathBuf>,
    pub title: String,
    /// 미저장 스크립트의 본문(파일 탭은 None — 원본을 만지지 않는다 · docs/70).
    pub text: Option<String>,
    /// ★ 무제 탭의 변수 표(T-153 ③ · 사용자 10-26 "프로젝트 모드 = 프로젝트 파일에 보관") — `vars_to_script` 글 · 파일 탭은 None(경로 해시 파일이 맡는다).
    pub vars: Option<String>,
    /// 캐럿(0 기준) + 북마크와 같은 fuzzy 앵커(줄 본문 · 앞뒤 문맥 · 빈 값 = 없음).
    pub line: usize,
    pub col: usize,
    pub anchor_text: String,
    pub before: String,
    pub after: String,
    /// 미리보기 탭이었나(북마크·탐색기 한 번 클릭) — 복원도 미리보기로(사용자 09-23).
    pub preview: bool,
    /// 파일 탭의 미저장 본문을 담았을 때 그 순간의 **디스크 해시**(`backups::disk_hash`) — 로드 때 디스크가 바뀌었으면 알린다
    /// (사용자 09-23 "미저장 탭 본문을 프로젝트 파일에 탭별로 저장해 로드 때 자동 복구 · 백업은 백업용으로만").
    pub disk_hash: u64,
    /// 저장 당시의 탭 id(0 = 없음) — 이름 없는 탭의 북마크(`DocKey::Scratch { tab }`)를 복원 때 새 id로 **재매핑**(사용자 09-23 검토).
    pub id: u64,
    /// ★ 탭별 들여쓰기 재정의(탭 폭, 공백 들여쓰기) — 상태줄 팝업으로 바꾼 값 · None = 설정 기본값(사용자 09-29 "폴더·프로젝트 모드에서는
    ///   탭별 상태를 저장하고 다시 열 때 복원"). 파일 모드(프로젝트 없음)는 저장할 곳이 없어 늘 설정값으로 시작한다.
    pub indent: Option<(u8, bool)>,
    /// 읽기 전용으로 열린 탭(큰 파일 "읽기 전용" · 라이선스 게이트 · 이진 안내) — 복원도 같은 기준으로(사용자 10-13).
    pub read_only: bool,
    /// ★ 뷰 탭 열쇠(`ext:<확장 id>` · 본문 없는 호스트 뷰) — 복원 때 같은 뷰를 다시 연다(사용자 09-30 "재시작하면 빈 편집 탭").
    pub view: Option<String>,
    /// ★ T-121 (e)(10-30): 저장 당시 이 탭이 묶여 있던 **공유 세션의 프로필**(접속 중일 때만 · 전용 세션·인라인 접속 = None) — 복원 때 같은
    ///   프로필의 공유 세션에 다시 묶는다(없으면 접속될 때까지 보관 · 자동 재접속 없음). 프로젝트 파일 키 `profile`(파일 하나 규칙).
    pub profile: Option<String>,
}

impl Project {
    pub(crate) fn is_open(&self) -> bool {
        self.path.is_some()
    }

    /// 표시 이름 = 파일 이름에서 확장자를 뺀 것.
    pub(crate) fn name(&self) -> Option<String> {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
    }

    /// 프로젝트 파일이 있는 폴더(상대 경로 기준).
    fn base(&self) -> Option<PathBuf> {
        self.path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
    }

    /// 파일에서 읽는다(경로는 절대로 되돌린다 · 없는 폴더도 목록에는 남긴다 — 탐색기가 "없음"으로 보인다).
    pub(crate) fn load(path: &Path) -> Result<Project, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        Self::from_bytes(path, &bytes)
    }

    /// 문서 바이트에서 읽는다(`path` = 상대 경로의 기준 · 파일은 읽지 않는다) — 다중 인스턴스 동기화(67 §9)가 디스크 본과
    /// 기준본(마지막으로 읽거나 쓴 문서)을 같은 길로 풀 때 쓴다.
    pub(crate) fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Project, String> {
        // 헤더(JSON) + 탭 payload 블록(`projfile` · 09-23) — 옛 파일(마커 없음)은 블록 0 · 본문은 헤더의 `text`.
        let (text, blobs) = nsql_settings::projfile::split(bytes)?;
        let json = parse(&text)?;
        let mut p = Project {
            path: Some(path.to_path_buf()),
            ..Project::default()
        };
        let base = p.base();
        let Json::Obj(fields) = json else {
            return Err("project file is not a JSON object".into());
        };
        // ★ 더 새 형식의 파일은 열지 않는다(T-172) — 이 판이 모르는 항목은 저장할 때 버려지므로, 열어서 저장하면 내용을 잃는다.
        if let Some(n) = fields.iter().find_map(|(k, v)| match (k.as_str(), v) {
            ("version", Json::Num(n)) => Some(*n as i64),
            _ => None,
        }) {
            if n > VERSION_VARS {
                return Err(format!(
                    "project file version {n} is newer than this app supports ({VERSION_VARS}) - update Nexa SQL to open it"
                ));
            }
        }
        p.id = fields.iter().find_map(|(k, v)| match (k.as_str(), v) {
            ("id", Json::Str(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        });
        let home = home_dir();
        // ★ 경로 변수는 폴더보다도 먼저(T-172 P3) — 등록 폴더 정의도 `${var:이름}/…`을 쓸 수 있다. 같은 이름은 뒤엣것이 이긴다.
        for (k, v) in &fields {
            if let ("vars", Json::Obj(items)) = (k.as_str(), v) {
                for (name, val) in items {
                    if let (Some(name), Json::Str(val)) = (valid_folder_name(name), val) {
                        if !val.trim().is_empty() {
                            p.path_vars.retain(|(n, _)| *n != name);
                            p.path_vars.push((name, val.trim().to_string()));
                        }
                    }
                }
            }
        }
        let vars = p.var_dirs();
        // ★ 폴더를 **먼저** 읽는다(T-172 P2) — 탭·선택 경로의 `${workspaceFolder:이름}` 앵커가 폴더 목록을 가리킨다(키 순서와 무관하게).
        for (k, v) in &fields {
            if k != "folders" {
                continue;
            }
            if let Json::Arr(items) = v {
                for it in items {
                    let field = |want: &str| match it {
                        Json::Obj(f) => f.iter().find_map(|(k, v)| match (k.as_str(), v) {
                            (k, Json::Str(s)) if k == want => Some(s.clone()),
                            _ => None,
                        }),
                        _ => None,
                    };
                    let rel = match it {
                        Json::Str(s) => Some(s.clone()),
                        _ => field("path"),
                    };
                    if let Some(rel) = rel.filter(|s| !s.trim().is_empty()) {
                        let cx = PathCtx {
                            base: base.as_deref(),
                            folders: &[],
                            names: &[],
                            home: home.as_deref(),
                            vars: &vars,
                        };
                        let dir = resolve(&rel, &cx);
                        if let Some(name) = field("name").and_then(|n| valid_folder_name(&n)) {
                            p.folder_names.retain(|(d, _)| *d != dir);
                            p.folder_names.push((dir.clone(), name));
                        }
                        p.folders.push(dir);
                    }
                }
            }
        }
        let folders = p.folders.clone();
        let names = anchor_names(&folders, &p.folder_names);
        let cx = PathCtx {
            base: base.as_deref(),
            folders: &folders,
            names: &names,
            home: home.as_deref(),
            vars: &vars,
        };
        for (k, v) in &fields {
            if k == "active" {
                if let Json::Num(n) = v {
                    p.active = (*n).max(0.0) as usize;
                }
                continue;
            }
            if k == "bookmarks" {
                if matches!(v, Json::Obj(_)) {
                    p.bookmarks = Some(json_dump(v));
                }
                continue;
            }
            if k == "tabs" {
                if let Json::Arr(items) = v {
                    for it in items {
                        let Json::Obj(f) = it else { continue };
                        let mut t = TabState::default();
                        for (fk, fv) in f {
                            match (fk.as_str(), fv) {
                                ("path", Json::Str(s)) if !s.trim().is_empty() => {
                                    t.path = Some(resolve(s, &cx))
                                }
                                ("title", Json::Str(s)) => t.title = s.clone(),
                                ("text", Json::Str(s)) => t.text = Some(s.clone()),
                                ("line", Json::Num(n)) => t.line = (*n).max(0.0) as usize,
                                ("col", Json::Num(n)) => t.col = (*n).max(0.0) as usize,
                                ("anchor", Json::Str(s)) => t.anchor_text = s.clone(),
                                ("before", Json::Str(s)) => t.before = s.clone(),
                                ("after", Json::Str(s)) => t.after = s.clone(),
                                ("preview", Json::Bool(b)) => t.preview = *b,
                                ("ro", Json::Bool(b)) => t.read_only = *b,
                                // 64비트 해시는 JSON 숫자(f64)로는 정밀도가 깨진다 → 글자열.
                                ("hash", Json::Str(s)) => t.disk_hash = s.parse().unwrap_or(0),
                                ("id", Json::Num(n)) => t.id = (*n).max(0.0) as u64,
                                ("vars", Json::Str(s)) if !s.is_empty() => t.vars = Some(s.clone()),
                                ("view", Json::Str(s)) => t.view = Some(s.clone()),
                                ("profile", Json::Str(s)) if !s.is_empty() => {
                                    t.profile = Some(s.clone())
                                }
                                ("tab_size", Json::Num(n)) => {
                                    let sp = t.indent.is_some_and(|x| x.1);
                                    t.indent = Some(((*n).clamp(1.0, 64.0) as u8, sp));
                                }
                                ("spaces", Json::Bool(b)) => {
                                    let ts = t.indent.map_or(4, |x| x.0);
                                    t.indent = Some((ts, *b));
                                }
                                _ => {}
                            }
                        }
                        p.tabs.push(t);
                    }
                }
                continue;
            }
            if k == "expanded" {
                if let Json::Arr(items) = v {
                    for it in items {
                        if let Json::Str(s) = it {
                            if !s.trim().is_empty() {
                                p.expanded.push(resolve(s, &cx));
                            }
                        }
                    }
                }
                continue;
            }
            if k == "panel" {
                if let Json::Str(s) = v {
                    if !s.trim().is_empty() {
                        p.panel = Some(s.clone());
                    }
                }
                continue;
            }
            if k == "search" {
                if let Json::Str(s) = v {
                    if !s.is_empty() {
                        p.search = Some(s.clone());
                    }
                }
                continue;
            }
            if k == "bm_collapsed" {
                if let Json::Arr(items) = v {
                    for it in items {
                        if let Json::Num(n) = it {
                            p.bm_collapsed.push((*n).max(0.0) as u32);
                        }
                    }
                }
                continue;
            }
            if k == "profiles" {
                if let Json::Arr(items) = v {
                    for it in items {
                        if let Json::Str(s) = it {
                            if !s.trim().is_empty() {
                                p.profiles.push(s.clone());
                            }
                        }
                    }
                }
                continue;
            }
            // ★ T-39b(10-30 · 67 §8-1): `settings` 절 = 평키 + `[문법]` 묶음 한 단계 · 값은 글자로 보관(모르는 키도 보존).
            if k == "settings" {
                if let Json::Obj(items) = v {
                    for (sk, sv) in items {
                        match (syntax_group(sk), sv) {
                            (Some(g), Json::Obj(inner)) => {
                                for (ik, iv) in inner {
                                    if let Some(raw) = json_raw(iv) {
                                        p.settings.push((Some(g.clone()), ik.clone(), raw));
                                    }
                                }
                            }
                            (None, other) => {
                                if let Some(raw) = json_raw(other) {
                                    p.settings.push((None, sk.clone(), raw));
                                }
                            }
                            (Some(_), _) => {}
                        }
                    }
                }
                continue;
            }
            if k == "selected" {
                if let Json::Str(s) = v {
                    if !s.trim().is_empty() {
                        p.last_selected = Some(resolve(s, &cx));
                    }
                }
                continue;
            }
        }
        // 탭 payload 블록 → 같은 `id`의 탭 본문(블록이 있으면 헤더의 옛 `text`보다 우선).
        for b in blobs {
            if b.tab == 0 {
                continue;
            }
            if let Some(t) = p.tabs.iter_mut().find(|t| t.id == b.tab) {
                t.text = Some(String::from_utf8_lossy(&b.data).into_owned());
            }
        }
        Ok(p)
    }

    /// JSON 본문(폴더 = 파일 위치 기준 상대 · 아니면 절대 · 구분자 `/`).
    pub(crate) fn to_json(&self) -> String {
        let base = self.base();
        let home = home_dir();
        let vars = self.var_dirs();
        // 폴더 자신은 폴더 앵커를 쓰지 않는다(프로젝트 기준 상대 · 홈 기준 · 절대) · 그 밖의 경로는 등록 폴더 앵커까지.
        let fcx = PathCtx {
            base: base.as_deref(),
            folders: &[],
            names: &[],
            home: home.as_deref(),
            vars: &vars,
        };
        // 정의 경로(이 PC가 다른 자리를 쓰는 폴더는 원래 경로) — 폴더 목록과 앵커 이름은 이쪽으로 적는다(T-172 (A)).
        let defs = self.folder_defs();
        let names = anchor_names(&defs, &self.folder_names);
        let cx = PathCtx {
            base: base.as_deref(),
            folders: &self.folders,
            names: &names,
            home: home.as_deref(),
            vars: &vars,
        };
        let mut out = String::new();
        out.push_str("{\n  \"version\": ");
        let version = if self.path_vars.is_empty() {
            VERSION
        } else {
            VERSION_VARS
        };
        out.push_str(&version.to_string());
        if let Some(id) = &self.id {
            out.push_str(",\n  \"id\": \"");
            out.push_str(&escape(id));
            out.push('"');
        }
        // 경로 변수(T-172 P3) — 적힌 글 그대로(있을 때만 · 옛 파일과 같은 꼴 유지).
        if !self.path_vars.is_empty() {
            out.push_str(",\n  \"vars\": {");
            for (i, (name, val)) in self.path_vars.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!("\n    \"{}\": \"{}\"", escape(name), escape(val)));
            }
            out.push_str("\n  }");
        }
        out.push_str(",\n  \"folders\": [");
        for (i, f) in defs.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("\n    { \"path\": \"");
            out.push_str(&escape(&relativize(f, &fcx)));
            out.push('"');
            // 적어 둔 이름(T-172 P2 `folders[].name`) — 있을 때만 쓴다(옛 파일과 같은 꼴 유지).
            if let Some(name) = explicit_folder_name(&self.folder_names, f) {
                out.push_str(", \"name\": \"");
                out.push_str(&escape(name));
                out.push('"');
            }
            out.push_str(" }");
        }
        if !self.folders.is_empty() {
            out.push('\n');
        }
        out.push_str("  ]");
        if let Some(sel) = &self.last_selected {
            out.push_str(",\n  \"selected\": \"");
            out.push_str(&escape(&relativize(sel, &cx)));
            out.push('"');
        }
        // 작업 환경(사용자 09-23): 탭 순서대로 · 파일은 경로만(원본 본문은 쓰지 않는다 · docs/70) · 스크립트는 본문 · 캐럿 앵커.
        if !self.tabs.is_empty() {
            out.push_str(&format!(",\n  \"active\": {}", self.active));
            out.push_str(",\n  \"tabs\": [");
            for (i, t) in self.tabs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str("\n    { ");
                if let Some(p) = &t.path {
                    out.push_str(&format!("\"path\": \"{}\", ", escape(&relativize(p, &cx))));
                }
                out.push_str(&format!(
                    "\"title\": \"{}\", \"line\": {}, \"col\": {}",
                    escape(&t.title),
                    t.line,
                    t.col
                ));
                if !t.anchor_text.is_empty() {
                    out.push_str(&format!(
                        ", \"anchor\": \"{}\", \"before\": \"{}\", \"after\": \"{}\"",
                        escape(&t.anchor_text),
                        escape(&t.before),
                        escape(&t.after)
                    ));
                }
                // 본문(`text`)은 헤더에 넣지 않는다 — 마커 아래 탭별 payload 블록(`projfile` · 사용자 09-23 "메타는 텍스트 · payload는 바이너리").
                if t.text.is_some() && t.id != 0 {
                    out.push_str(", \"blob\": true");
                }
                if t.read_only {
                    out.push_str(", \"ro\": true");
                }
                if t.preview {
                    out.push_str(", \"preview\": true");
                }
                if t.disk_hash != 0 {
                    out.push_str(&format!(", \"hash\": \"{}\"", t.disk_hash));
                }
                // 무제 탭의 변수 표(파일 탭은 경로 해시 파일 · T-153 ③).
                if let Some(v) = t
                    .vars
                    .as_deref()
                    .filter(|v| !v.is_empty() && t.path.is_none())
                {
                    out.push_str(&format!(", \"vars\": \"{}\"", escape(v)));
                }
                if t.id != 0 {
                    out.push_str(&format!(", \"id\": {}", t.id));
                }
                if let Some((ts, sp)) = t.indent {
                    out.push_str(&format!(", \"tab_size\": {ts}, \"spaces\": {sp}"));
                }
                if let Some(v) = &t.view {
                    out.push_str(&format!(", \"view\": \"{}\"", escape(v)));
                }
                // ★ T-121 (e): 저장 당시 공유 세션의 프로필(접속 중이던 탭만).
                if let Some(p) = t.profile.as_deref().filter(|p| !p.is_empty()) {
                    out.push_str(&format!(", \"profile\": \"{}\"", escape(p)));
                }
                out.push_str(" }");
            }
            out.push_str("\n  ]");
        }
        // 좌측 패널 상태(사용자 09-23) — 비어 있으면 키를 쓰지 않는다(파일은 짧게 · 옛 파일과 호환).
        if !self.expanded.is_empty() {
            out.push_str(",\n  \"expanded\": [");
            for (i, d) in self.expanded.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("\"{}\"", escape(&relativize(d, &cx))));
            }
            out.push(']');
        }
        if let Some(p) = self.panel.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!(",\n  \"panel\": \"{}\"", escape(p)));
        }
        if let Some(s) = self.search.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!(",\n  \"search\": \"{}\"", escape(s)));
        }
        if !self.bm_collapsed.is_empty() {
            let ids: Vec<String> = self.bm_collapsed.iter().map(u32::to_string).collect();
            out.push_str(&format!(",\n  \"bm_collapsed\": [{}]", ids.join(", ")));
        }
        if !self.profiles.is_empty() {
            let names: Vec<String> = self
                .profiles
                .iter()
                .map(|n| format!("\"{}\"", escape(n)))
                .collect();
            out.push_str(&format!(",\n  \"profiles\": [{}]", names.join(", ")));
        }
        // ★ T-39b: 설정 절(평키 먼저 · 그 다음 `[문법]` 묶음 · 들어온 순서 유지 · 모르는 키도 그대로 다시 쓴다).
        if !self.settings.is_empty() {
            out.push_str(",\n  \"settings\": {");
            let mut first = true;
            let mut sep = |out: &mut String| {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str("\n    ");
            };
            for (g, k, raw) in &self.settings {
                if g.is_none() {
                    sep(&mut out);
                    out.push_str(&format!("\"{}\": {}", escape(k), setting_json(k, raw)));
                }
            }
            let mut groups: Vec<&str> = Vec::new();
            for g in self.settings.iter().filter_map(|(g, _, _)| g.as_deref()) {
                if !groups.contains(&g) {
                    groups.push(g);
                }
            }
            for g in groups {
                sep(&mut out);
                out.push_str(&format!("\"[{}]\": {{ ", escape(g)));
                let mut inner_first = true;
                for (_, k, raw) in self
                    .settings
                    .iter()
                    .filter(|(x, _, _)| x.as_deref() == Some(g))
                {
                    if !inner_first {
                        out.push_str(", ");
                    }
                    inner_first = false;
                    out.push_str(&format!("\"{}\": {}", escape(k), setting_json(k, raw)));
                }
                out.push_str(" }");
            }
            out.push_str("\n  }");
        }
        if let Some(b) = &self.bookmarks {
            out.push_str(",\n  \"bookmarks\": ");
            out.push_str(b.trim());
        }
        out.push_str("\n}\n");
        out
    }

    /// 파일에 쓴다(`path`가 없으면 오류).
    pub(crate) fn save(&self) -> Result<(), String> {
        let Some(p) = &self.path else {
            return Err("no project path".into());
        };
        write_atomic(p, &self.to_document())
    }

    /// 파일 전체 = JSON 헤더(`to_json`) + `%%NSQL-BLOBS%%` + 탭별 payload 블록(본문이 있는 탭 · `tab=<id> len=<n>` + 원문 바이트 ·
    /// [`nsql_settings::projfile`] · 사용자 09-23). 바뀜 비교(`project_last_json`)도 이것으로.
    pub(crate) fn to_document(&self) -> Vec<u8> {
        let blobs: Vec<nsql_settings::projfile::Blob> = self
            .tabs
            .iter()
            .filter_map(|t| {
                t.text.as_ref().map(|txt| nsql_settings::projfile::Blob {
                    tab: t.id,
                    data: txt.as_bytes().to_vec(),
                })
            })
            .collect();
        nsql_settings::projfile::join(&self.to_json(), &blobs)
    }

    /// 다른 이름으로 — 경로만 바꾼다(상대 경로는 저장 때 새 위치 기준으로 다시 계산).
    pub(crate) fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.to_path_buf());
        self
    }

    /// 폴더 추가(중복 무시) — 추가됐으면 true.
    pub(crate) fn add_folder(&mut self, dir: &Path) -> bool {
        if self.folders.iter().any(|f| f == dir) {
            return false;
        }
        self.folders.push(dir.to_path_buf());
        true
    }

    pub(crate) fn remove_folder(&mut self, i: usize) -> Option<PathBuf> {
        if i < self.folder_src.len() {
            self.folder_src.remove(i);
        }
        (i < self.folders.len()).then(|| self.folders.remove(i))
    }

    /// ★ T-172 (A): 폴더 `i`의 **정의 경로**(프로젝트 파일에 적힌 것) — 이 PC가 다른 자리를 쓰고 있으면 그 원래 경로, 아니면 지금 경로.
    pub(crate) fn folder_def(&self, i: usize) -> PathBuf {
        self.folder_src
            .get(i)
            .cloned()
            .flatten()
            .or_else(|| self.folders.get(i).cloned())
            .unwrap_or_default()
    }

    /// ★ T-172 P3: 경로 변수의 **이 PC 자리**(이름, 폴더) — 적힌 글을 프로젝트 폴더·홈 기준으로 푼다(변수끼리·등록 폴더는 가리키지 않는다).
    pub(crate) fn var_dirs(&self) -> Vec<(String, PathBuf)> {
        let (base, home) = (self.base(), home_dir());
        let cx = PathCtx {
            base: base.as_deref(),
            folders: &[],
            names: &[],
            home: home.as_deref(),
            vars: &[],
        };
        let mut out: Vec<(String, PathBuf)> = self
            .path_vars
            .iter()
            .map(|(n, v)| (n.clone(), resolve(v, &cx)))
            .collect();
        // 이 PC만의 자리가 공유 정의를 덮는다(없는 이름은 추가).
        for (n, d) in &self.var_pc {
            match out.iter_mut().find(|(x, _)| x == n) {
                Some(hit) => hit.1 = d.clone(),
                None => out.push((n.clone(), d.clone())),
            }
        }
        out
    }

    /// 변수 `name`이 지금 가리키는 자리 — 정의(공유·이 PC)가 있으면 그 폴더, 없으면 미정의 앵커가 글자 그대로 풀린 자리
    /// (`<프로젝트 폴더>/${var:이름}` · `resolve`가 보존하는 꼴).
    fn var_place(&self, name: &str) -> Option<PathBuf> {
        if let Some((_, d)) = self.var_dirs().into_iter().find(|(n, _)| n == name) {
            return Some(d);
        }
        Some(self.base()?.join(format!("{ANCHOR_VAR}{name}}}")))
    }

    /// 변수 `name`의 지금 자리가 이 PC에 **없는가**(미정의 앵커 포함) — 이럴 때만 자리 탭 [찾아보기]가 변수 자리를 추론한다.
    pub(crate) fn var_missing_here(&self, name: &str) -> bool {
        !self.var_place(name).is_some_and(|d| d.is_dir())
    }

    /// ★ 못 찾은 경로가 어느 경로 변수 아래인가 — `(이름, 변수 폴더 뒤의 꼬리)`. 정의된 변수(가장 깊게 맞는 것) 또는 미정의
    /// 앵커(`${var:이름}/…`가 글자 그대로 남은 경로). 꼬리가 비면(변수 폴더 자신) `None`.
    pub(crate) fn var_of_path(&self, p: &Path) -> Option<(String, PathBuf)> {
        let hit = self
            .var_dirs()
            .into_iter()
            .filter_map(|(n, d)| {
                let r = p.strip_prefix(&d).ok()?.to_path_buf();
                Some((d.components().count(), n, r))
            })
            .max_by_key(|(depth, _, _)| *depth)
            .map(|(_, n, r)| (n, r));
        let hit = hit.or_else(|| {
            let rel = p.strip_prefix(self.base()?).ok()?;
            let mut it = rel.components();
            let first = it.next()?.as_os_str().to_string_lossy().into_owned();
            let name = first.strip_prefix(ANCHOR_VAR)?.strip_suffix('}')?;
            Some((valid_folder_name(name)?, it.as_path().to_path_buf()))
        })?;
        (!hit.1.as_os_str().is_empty()).then_some(hit)
    }

    /// ★ T-172 P3 2단계: 변수 `name`의 **이 PC 자리**를 `dir`로 — 그 변수 아래를 가리키던 탭·펼침·선택·등록 폴더 경로를 새 자리로
    /// 옮긴다(프로젝트 파일의 `vars`는 그대로). 바뀌었으면 (옛 자리, 새 자리).
    pub(crate) fn set_var_pc(&mut self, name: &str, dir: &Path) -> Option<(PathBuf, PathBuf)> {
        let name = valid_folder_name(name)?;
        let old = self.var_place(&name)?;
        if old == dir {
            return None;
        }
        self.var_pc.retain(|(n, _)| *n != name);
        self.var_pc.push((name, dir.to_path_buf()));
        let re = |p: &mut PathBuf| {
            if let Ok(r) = p.strip_prefix(&old) {
                *p = if r.as_os_str().is_empty() {
                    dir.to_path_buf()
                } else {
                    dir.join(r)
                };
            }
        };
        self.tabs
            .iter_mut()
            .filter_map(|t| t.path.as_mut())
            .for_each(re);
        self.expanded.iter_mut().for_each(re);
        self.last_selected.iter_mut().for_each(re);
        self.folders.iter_mut().for_each(re);
        // 등록 폴더의 정의 쪽 기록(다른 자리를 쓰는 폴더의 원래 경로 · `folders[].name`의 열쇠)도 같이 옮긴다 — 안 옮기면 저장 때
        //   적어 둔 폴더 이름이 조용히 빠진다(검토 11-23).
        self.folder_src.iter_mut().flatten().for_each(re);
        self.folder_names.iter_mut().for_each(|(d, _)| re(d));
        Some((old, dir.to_path_buf()))
    }

    fn folder_defs(&self) -> Vec<PathBuf> {
        (0..self.folders.len())
            .map(|i| self.folder_def(i))
            .collect()
    }

    /// 이 실제 경로를 쓰는 폴더의 정의 경로(다른 자리를 쓰는 폴더만 `Some`) — 다중 인스턴스 병합이 폴더 목록을 다시 짤 때.
    pub(crate) fn folder_src_of(&self, f: &Path) -> Option<PathBuf> {
        let i = self.folders.iter().position(|x| x == f)?;
        self.folder_src.get(i).cloned().flatten()
    }

    /// PC별 대응표의 열쇠 = 폴더가 프로젝트 파일에 적히는 글(정의 경로 · 프로젝트 기준 상대/홈 앵커/절대).
    pub(crate) fn folder_key(&self, i: usize) -> String {
        let (base, home) = (self.base(), home_dir());
        let vars = self.var_dirs();
        let fcx = PathCtx {
            base: base.as_deref(),
            folders: &[],
            names: &[],
            home: home.as_deref(),
            vars: &vars,
        };
        relativize(&self.folder_def(i), &fcx)
    }

    /// ★ T-172 (D-188): 프로젝트 파일에 **절대 경로로 적히는** 것의 수(등록 폴더 정의 + 탭 경로 · 같은 글은 한 번) — 프로젝트 폴더
    /// 기준 상대 · 등록 폴더 앵커 · 홈 앵커로 적히는 것은 세지 않는다. 폴더째 옮기거나 다른 PC에서 열면 따라오지 않는 경로다
    /// (저장은 막지 않는다 · 호출 쪽이 `project.warn_absolute`일 때 로그 한 줄).
    pub(crate) fn absolute_path_count(&self) -> usize {
        let (base, home) = (self.base(), home_dir());
        let vars = self.var_dirs();
        let defs = self.folder_defs();
        let fcx = PathCtx {
            base: base.as_deref(),
            folders: &[],
            names: &[],
            home: home.as_deref(),
            vars: &vars,
        };
        let names = anchor_names(&defs, &self.folder_names);
        let cx = PathCtx {
            base: base.as_deref(),
            folders: &self.folders,
            names: &names,
            home: home.as_deref(),
            vars: &vars,
        };
        let tabs = self.tabs.iter().filter_map(|t| t.path.as_deref());
        let mut seen: Vec<String> = Vec::new();
        for s in defs
            .iter()
            .map(|f| relativize(f, &fcx))
            .chain(tabs.map(|p| relativize(p, &cx)))
        {
            if is_absolute_text(&s) && !seen.contains(&s) {
                seen.push(s);
            }
        }
        seen.len()
    }

    /// 고유 번호(없으면 지금 만든다 · OS 난수 8바이트 = 16진 16자) — PC별 대응표의 열쇠(D-494). 난수를 못 얻으면 `None`.
    pub(crate) fn ensure_id(&mut self) -> Option<String> {
        if self.id.is_none() {
            let b = nsql_vault::random_bytes(8).ok()?;
            self.id = Some(b.iter().map(|x| format!("{x:02x}")).collect());
        }
        self.id.clone()
    }

    /// ★ T-172 (B): 폴더 `i`를 **이 PC에서만** 다른 자리로 — 프로젝트 파일에는 정의 경로가 그대로 적힌다(`folder_src`). 그 폴더
    ///   아래를 가리키던 탭·펼침·선택 경로를 새 자리로 옮긴다. 바뀌었으면 (옛 실제 경로, 새 경로).
    pub(crate) fn relocate_folder(&mut self, i: usize, new: &Path) -> Option<(PathBuf, PathBuf)> {
        let old = self.folders.get(i)?.clone();
        if old == new {
            return None;
        }
        let def = self.folder_def(i);
        if self.folder_src.len() < self.folders.len() {
            self.folder_src.resize(self.folders.len(), None);
        }
        self.folder_src[i] = (def != new).then_some(def);
        self.folders[i] = new.to_path_buf();
        let re = |p: &mut PathBuf| {
            if let Ok(r) = p.strip_prefix(&old) {
                *p = if r.as_os_str().is_empty() {
                    new.to_path_buf()
                } else {
                    new.join(r)
                };
            }
        };
        self.tabs
            .iter_mut()
            .filter_map(|t| t.path.as_mut())
            .for_each(re);
        self.expanded.iter_mut().for_each(re);
        self.last_selected.iter_mut().for_each(re);
        Some((old, new.to_path_buf()))
    }

    /// ★ T-172 (A): 이 PC의 대응표(설정 `project.pc_paths`)를 입힌다 — **못 찾은 폴더만**, 표의 자리가 실제로 있을 때만.
    pub(crate) fn apply_pc_paths(&mut self, raw: &str) {
        let Some(id) = self.id.clone() else { return };
        // 경로 변수의 이 PC 자리(T-172 P3 2단계 · 열쇠 `var:이름`) — 폴더보다 먼저(등록 폴더가 변수 아래일 수 있다) ·
        //   폴더와 같은 규칙: 지금 자리가 이 PC에 없고 표의 자리가 실제로 있을 때만.
        for (name, to) in pc_paths_vars(raw, &id) {
            let here = self.var_place(&name).is_some_and(|d| d.is_dir());
            if !here && to.is_dir() {
                self.set_var_pc(&name, &to);
            }
        }
        for i in 0..self.folders.len() {
            if self.folders[i].is_dir() {
                continue;
            }
            if let Some(to) = pc_paths_get(raw, &id, &self.folder_key(i)).filter(|p| p.is_dir()) {
                self.relocate_folder(i, &to);
            }
        }
    }

    /// 읽고 이 PC의 대응표까지 입힌다(앱이 여는 길은 전부 이것).
    pub(crate) fn load_pc(path: &Path, pc_paths: &str) -> Result<Project, String> {
        let mut p = Self::load(path)?;
        p.apply_pc_paths(pc_paths);
        Ok(p)
    }

    pub(crate) fn from_bytes_pc(
        path: &Path,
        bytes: &[u8],
        pc_paths: &str,
    ) -> Result<Project, String> {
        let mut p = Self::from_bytes(path, bytes)?;
        p.apply_pc_paths(pc_paths);
        Ok(p)
    }
}

/// ★ 작업 모드 셋(사용자 09-23): **파일 모드**(인자 없음 · 로컬 상태 = 전역 설정 폴더 `%APPDATA%`) · **폴더 모드**(`nexa-sql .` ·
/// `nexa-sql <폴더>` · 로컬 상태 = `<폴더>/.nsql/`) · **프로젝트 모드**(`.nsql-project` · 상태는 프로젝트 파일 안).
/// 지금 모드가 나누는 것은 **북마크**뿐 — 설정 오버라이드·최근 파일·되돌리기 기록 같은 다른 로컬 상태도 앞으로는 [`WorkMode::local_dir`]
/// 한 자리에서 나눈다(67 §6). 프로젝트 모드는 `local_dir = None`(파일 안에 담는다 · 기기별 오버라이드는 73 §4-2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkMode {
    File,
    Folder(PathBuf),
    Project(PathBuf),
}

impl WorkMode {
    /// 폴더 모드의 로컬 상태 폴더 이름.
    pub(crate) const LOCAL_DIR: &'static str = ".nsql";

    /// 우선순위 = 프로젝트 > 폴더 > 파일.
    pub(crate) fn of(project: Option<&Path>, folder: Option<&Path>) -> WorkMode {
        match (project, folder) {
            (Some(p), _) => WorkMode::Project(p.to_path_buf()),
            (None, Some(d)) => WorkMode::Folder(d.to_path_buf()),
            (None, None) => WorkMode::File,
        }
    }

    /// 로컬 상태를 두는 폴더 — 파일 모드 = 전역 설정 폴더 · 폴더 모드 = `<폴더>/.nsql` · 프로젝트 모드 = None.
    pub(crate) fn local_dir(&self) -> Option<PathBuf> {
        match self {
            WorkMode::File => nsql_settings::config_dir(),
            WorkMode::Folder(d) => Some(d.join(Self::LOCAL_DIR)),
            WorkMode::Project(_) => None,
        }
    }
}

/// 상대 경로 → 절대(`base` 아래) · 이미 절대면 그대로.
/// `[sql]` → `Some("sql")`(소문자 · 67 §8-1 VS Code `[언어]` 관례) · 평키 = None.
/// ★ 원자적 쓰기(67 §9 · 다중 인스턴스): 같은 폴더의 임시 파일에 다 쓴 뒤 바꿔치기 — 쓰는 도중 다른 인스턴스가 읽어도 반쪽 문서를
///   보지 않는다. 임시 이름에 프로세스 id를 넣어 두 인스턴스의 임시 파일이 겹치지 않게 한다. 바꿔치기가 막히면(읽는 쪽이 잡고 있음 ·
///   읽기 전용 폴더의 기존 파일) 종전처럼 그 자리에 쓴다.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    if std::fs::write(&tmp, data).is_ok() {
        if std::fs::rename(&tmp, path).is_ok() {
            return Ok(());
        }
        let _ = std::fs::remove_file(&tmp);
    }
    std::fs::write(path, data).map_err(|e| e.to_string())
}

fn syntax_group(key: &str) -> Option<String> {
    let inner = key.strip_prefix('[')?.strip_suffix(']')?.trim();
    (!inner.is_empty()).then(|| inner.to_lowercase())
}

/// JSON 값 → 설정 원시 글자(레지스트리 꼴): 불 = on/off · 정수는 소수점 없이 · 문자열 그대로 · 배열·객체·null = 없음.
pub(crate) fn json_raw(v: &Json) -> Option<String> {
    match v {
        Json::Bool(b) => Some(if *b { "on" } else { "off" }.to_string()),
        Json::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => Some(format!("{}", *n as i64)),
        Json::Num(n) => Some(n.to_string()),
        Json::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// 설정 원시 글자 → JSON 조각(레지스트리 종류대로: 불 → true/false · 정수 → 숫자 · 그 밖·모르는 키 = 문자열).
fn setting_json(key: &str, raw: &str) -> String {
    use nsql_settings::SettingKind;
    match nsql_settings::entry(key).map(|e| e.kind) {
        Some(SettingKind::Bool) if matches!(raw, "on" | "true") => "true".into(),
        Some(SettingKind::Bool) if matches!(raw, "off" | "false") => "false".into(),
        Some(SettingKind::Int { .. }) if raw.parse::<i64>().is_ok() => raw.to_string(),
        _ => format!("\"{}\"", escape(raw)),
    }
}

/// 경로를 풀고 적을 때의 기준(T-172 P2): 프로젝트 파일 폴더 · 등록 폴더들 · 홈.
struct PathCtx<'a> {
    base: Option<&'a Path>,
    folders: &'a [PathBuf],
    /// 폴더별 앵커 이름(`folders`와 나란히 · [`anchor_names`]가 **정의 경로**에서 얻는다 — 이 PC가 다른 자리를 써도 이름은 공유
    /// 정의 쪽) · `None` = 그 폴더에는 앵커를 쓰지 않는다.
    names: &'a [Option<String>],
    home: Option<&'a Path>,
    /// 경로 변수(이름, 이 PC의 폴더 · T-172 P3) — `${var:이름}/…`.
    vars: &'a [(String, PathBuf)],
}

const ANCHOR_HOME: &str = "${userHome}";
const ANCHOR_PROJECT: &str = "${workspaceFolder}";
const ANCHOR_FOLDER: &str = "${workspaceFolder:";
const ANCHOR_VAR: &str = "${var:";

fn home_dir() -> Option<PathBuf> {
    crate::sessions::home_dir_str().map(PathBuf::from)
}

/// 앵커 이름으로 쓸 수 있는 글(앞뒤 공백 제거 · 비어 있지 않고 `}`가 없다) — 아니면 `None`.
fn valid_folder_name(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty() && !s.contains('}')).then(|| s.to_string())
}

/// 정의 경로 `def`에 적어 둔 이름(`folders[].name`).
fn explicit_folder_name<'a>(explicit: &'a [(PathBuf, String)], def: &Path) -> Option<&'a str> {
    explicit
        .iter()
        .find(|(d, _)| d == def)
        .map(|(_, n)| n.as_str())
}

/// 등록 폴더별 앵커 이름(`defs`와 나란히): 적어 둔 이름(`folders[].name`)이 있으면 그것, 없으면 폴더 이름(leaf) — 쓸 수 없는 글이거나
/// **다른 등록 폴더의 이름과 겹치면**(대소문자 무시) 그 폴더들에는 앵커를 쓰지 않는다(`None` · 어느 쪽인지 알 수 없다).
fn anchor_names(defs: &[PathBuf], explicit: &[(PathBuf, String)]) -> Vec<Option<String>> {
    let raw: Vec<Option<String>> = defs
        .iter()
        .map(|d| match explicit_folder_name(explicit, d) {
            Some(n) => valid_folder_name(n),
            None => d
                .file_name()
                .and_then(|n| valid_folder_name(&n.to_string_lossy())),
        })
        .collect();
    (0..raw.len())
        .map(|i| {
            let name = raw[i].as_ref()?;
            let dup = raw
                .iter()
                .enumerate()
                .any(|(j, o)| j != i && o.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)));
            (!dup).then(|| name.clone())
        })
        .collect()
}

/// 저장된 글 → 경로. 접두 없는 상대 = 프로젝트 파일 폴더 기준(옛 파일 그대로) · 앵커 = 스크립트 내장 변수와 같은 이름
/// (`${workspaceFolder}` · `${workspaceFolder:이름}` · `${userHome}` · 사용자 11-22). 풀 수 없는 앵커(이 PC에 그 폴더가 없다)는
/// 글자 그대로 프로젝트 폴더 아래 경로로 두어, 못 찾은 탭으로 보존되고 저장하면 같은 글로 되돌아간다.
fn resolve(rel: &str, cx: &PathCtx<'_>) -> PathBuf {
    let tail = |rest: &str| rest.trim_start_matches(['/', '\\']).to_string();
    if let (Some(rest), Some(h)) = (rel.strip_prefix(ANCHOR_HOME), cx.home) {
        return h.join(tail(rest));
    }
    if let (Some(rest), Some(b)) = (rel.strip_prefix(ANCHOR_PROJECT), cx.base) {
        return b.join(tail(rest));
    }
    if let Some(after) = rel.strip_prefix(ANCHOR_FOLDER) {
        if let Some((name, rest)) = after.split_once('}') {
            let hit = (0..cx.folders.len())
                .find(|&i| cx.names.get(i).and_then(|n| n.as_deref()) == Some(name));
            if let Some(i) = hit {
                return cx.folders[i].join(tail(rest));
            }
        }
    }
    // 경로 변수(T-172 P3) — 정의되지 않은 이름은 아래로 떨어져 글자 그대로 보존된다(못 찾은 탭 · 저장하면 같은 글).
    if let Some(after) = rel.strip_prefix(ANCHOR_VAR) {
        if let Some((name, rest)) = after.split_once('}') {
            if let Some((_, dir)) = cx.vars.iter().find(|(n, _)| n == name) {
                return dir.join(tail(rest));
            }
        }
    }
    let p = PathBuf::from(rel);
    if p.is_absolute() {
        return p;
    }
    match cx.base {
        Some(b) => b.join(p),
        None => p,
    }
}

/// 경로 → 저장할 글(`/` 구분): 프로젝트 파일 폴더 아래 = 상대 · 등록 폴더 아래 = `${workspaceFolder:이름}/…`(가장 깊게 맞는
/// 폴더) · 홈 아래 = `${userHome}/…` · 그 밖 = 절대. 프로젝트를 폴더째 옮기거나 다른 PC에서 열어도 경로가 따라온다.
fn relativize(p: &Path, cx: &PathCtx<'_>) -> String {
    let slash = |r: &Path| r.to_string_lossy().replace('\\', "/");
    if let Some(r) = cx.base.and_then(|b| p.strip_prefix(b).ok()) {
        return if r.as_os_str().is_empty() {
            ".".into()
        } else {
            slash(r)
        };
    }
    // 후보 = 등록 폴더 앵커 + 경로 변수 — 가장 깊게 맞는 것(깊이가 같으면 등록 폴더 · `max_by_key`는 뒤엣것을 고르므로 변수를 앞에 둔다).
    let var_hits = cx.vars.iter().filter_map(|(name, dir)| {
        let r = p.strip_prefix(dir).ok()?;
        Some((dir.components().count(), format!("{ANCHOR_VAR}{name}}}"), r))
    });
    let folder_hits = (0..cx.folders.len()).filter_map(|i| {
        let r = p.strip_prefix(&cx.folders[i]).ok()?;
        let name = cx.names.get(i).cloned().flatten()?;
        Some((
            cx.folders[i].components().count(),
            format!("{ANCHOR_FOLDER}{name}}}"),
            r,
        ))
    });
    let best = var_hits
        .chain(folder_hits)
        .max_by_key(|(depth, _, _)| *depth);
    let join = |anchor: String, r: &Path| {
        if r.as_os_str().is_empty() {
            anchor
        } else {
            format!("{anchor}/{}", slash(r))
        }
    };
    if let Some((_, anchor, r)) = best {
        return join(anchor, r);
    }
    if let Some(r) = cx.home.and_then(|h| p.strip_prefix(h).ok()) {
        return join(ANCHOR_HOME.to_string(), r);
    }
    slash(p)
}

/// `relativize`가 낸 글이 절대 경로인가(`/…` · `//서버/…` · `X:/…`) — 상대·앵커(`${…}`)가 아니면 절대다.
fn is_absolute_text(s: &str) -> bool {
    s.starts_with('/') || s.as_bytes().get(1) == Some(&b':')
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// PC별 폴더 대응표(설정 `project.pc_paths` · T-172 (A) · D-494) — `고유 번호|폴더 열쇠|이 PC의 경로` 셋씩 `|`로 잇는다(최신 먼저).
fn pc_paths_rows(raw: &str) -> Vec<(&str, &str, &str)> {
    let mut it = raw.split('|');
    let mut out = Vec::new();
    while let (Some(a), Some(b), Some(c)) = (it.next(), it.next(), it.next()) {
        if !a.is_empty() && !b.is_empty() && !c.is_empty() {
            out.push((a, b, c));
        }
    }
    out
}

/// PC별 대응표에서 프로젝트 `id`의 경로 변수 행(열쇠 `var:이름`) — (이름, 이 PC의 폴더).
pub(crate) fn pc_paths_vars(raw: &str, id: &str) -> Vec<(String, PathBuf)> {
    pc_paths_rows(raw)
        .into_iter()
        .filter(|(i, _, _)| *i == id)
        .filter_map(|(_, k, p)| Some((k.strip_prefix(PC_VAR_KEY)?.to_string(), PathBuf::from(p))))
        .collect()
}

/// PC별 대응표의 경로 변수 열쇠 접두(`var:이름`).
pub(crate) const PC_VAR_KEY: &str = "var:";

pub(crate) fn pc_paths_get(raw: &str, id: &str, key: &str) -> Option<PathBuf> {
    pc_paths_rows(raw)
        .into_iter()
        .find(|(a, b, _)| *a == id && *b == key)
        .map(|(_, _, p)| PathBuf::from(p))
}

/// 한 줄을 넣거나 바꾼다(맨 앞으로 · 200줄 상한 = 오래된 것부터 버린다). 열쇠·경로에 `|`가 있으면 적지 않는다.
pub(crate) fn pc_paths_set(raw: &str, id: &str, key: &str, path: &Path) -> String {
    let path = path.to_string_lossy();
    if id.is_empty() || key.is_empty() || key.contains('|') || path.contains('|') {
        return raw.to_string();
    }
    let mut rows: Vec<(&str, &str, &str)> = pc_paths_rows(raw)
        .into_iter()
        .filter(|(a, b, _)| !(*a == id && *b == key))
        .collect();
    rows.insert(0, (id, key, &path));
    rows.truncate(200);
    rows.iter()
        .map(|(a, b, c)| format!("{a}|{b}|{c}"))
        .collect::<Vec<_>>()
        .join("|")
}

/// 최근 프로젝트 목록(설정 `project.recent` · `|` 구분 · 최신 먼저 · 존재하는 파일만 · 최대 10).
pub(crate) fn recent_list(raw: &str) -> Vec<PathBuf> {
    raw.split('|')
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .take(10)
        .collect()
}

/// 최근 목록 앞에 넣는다(중복 제거 · 10개).
pub(crate) fn push_recent(raw: &str, path: &Path) -> String {
    let mut v = recent_list(raw);
    v.retain(|p| p != path);
    v.insert(0, path.to_path_buf());
    v.truncate(10);
    v.iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {

    #![allow(clippy::unwrap_used)]
    use super::*;

    /// T-153 ③: 무제 탭의 변수 표는 프로젝트 파일에 실려 왕복한다(파일 탭은 안 실림).
    #[test]
    fn untitled_tab_vars_round_trip_in_project_file() {
        let d = std::env::temp_dir().join(format!("nsql-proj-vars-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        let path = d.join("t.nsql-project");
        let mut p = Project::default().with_path(&path);
        p.tabs.push(TabState {
            title: "Untitled-1".into(),
            text: Some("SELECT :A;".into()),
            id: 7,
            vars: Some("VARIABLE A NUMBER\nEXEC :A := 1\n".into()),
            ..TabState::default()
        });
        p.tabs.push(TabState {
            path: Some(d.join("f.sql")),
            title: "f.sql".into(),
            id: 8,
            vars: Some("should not be written".into()),
            ..TabState::default()
        });
        p.save().unwrap();
        let back = Project::load(&path).unwrap();
        assert_eq!(back.tabs.len(), 2);
        assert_eq!(
            back.tabs[0].vars.as_deref(),
            Some("VARIABLE A NUMBER\nEXEC :A := 1\n")
        );
        assert_eq!(back.tabs[0].text.as_deref(), Some("SELECT :A;"));
        assert_eq!(
            back.tabs[1].vars, None,
            "파일 탭 변수는 프로젝트에 싣지 않는다"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 작업 모드 셋(사용자 09-23): 프로젝트 > 폴더 > 파일 · 로컬 상태 폴더 = 전역 / `<폴더>/.nsql` / 없음.
    #[test]
    fn work_mode_priority_and_local_dir() {
        let pj = Path::new("D:/w/x.nsql-project");
        let dir = Path::new("D:/w");
        assert_eq!(
            WorkMode::of(Some(pj), Some(dir)),
            WorkMode::Project(pj.to_path_buf())
        );
        assert_eq!(
            WorkMode::of(None, Some(dir)),
            WorkMode::Folder(dir.to_path_buf())
        );
        assert_eq!(WorkMode::of(None, None), WorkMode::File);
        assert_eq!(
            WorkMode::Folder(dir.to_path_buf()).local_dir(),
            Some(dir.join(".nsql"))
        );
        assert_eq!(WorkMode::Project(pj.to_path_buf()).local_dir(), None);
        assert_eq!(WorkMode::File.local_dir(), nsql_settings::config_dir());
    }

    #[test]
    fn folder_and_home_anchors_round_trip() {
        let root = PathBuf::from(if cfg!(windows) { "Q:\\w" } else { "/w" });
        let (proj, lib, lib2, home) = (
            root.join("proj"),
            root.join("ext").join("lib"),
            root.join("other").join("lib"),
            root.join("home"),
        );
        let folders = vec![proj.join("sql"), lib.clone()];
        let names = anchor_names(&folders, &[]);
        let cx = PathCtx {
            base: Some(&proj),
            folders: &folders,
            names: &names,
            home: Some(&home),
            vars: &[],
        };
        let cases = [
            (proj.join("sql").join("a.sql"), "sql/a.sql"),
            (
                lib.join("x").join("b.sql"),
                "${workspaceFolder:lib}/x/b.sql",
            ),
            (lib.clone(), "${workspaceFolder:lib}"),
            (home.join("n.sql"), "${userHome}/n.sql"),
        ];
        for (p, want) in &cases {
            assert_eq!(relativize(p, &cx), *want);
            assert_eq!(resolve(want, &cx), *p, "{want}");
        }
        assert_eq!(resolve("${workspaceFolder}/sql/a.sql", &cx), cases[0].0);
        // 이름이 겹치는 등록 폴더에는 앵커를 쓰지 않는다(어느 쪽인지 알 수 없다) → 절대.
        let dup = vec![lib.clone(), lib2.clone()];
        let dup_names = anchor_names(&dup, &[]);
        let cx2 = PathCtx {
            base: Some(&proj),
            folders: &dup,
            names: &dup_names,
            home: None,
            vars: &[],
        };
        assert!(!relativize(&lib.join("b.sql"), &cx2).starts_with("${"));
        // ★ `folders[].name`: 겹치는 leaf도 이름을 적어 두면 앵커를 쓴다 · 적은 이름끼리 겹치거나 쓸 수 없는 글이면 다시 절대.
        let named = [(lib2.clone(), "lib-other".to_string())];
        let n2 = anchor_names(&dup, &named);
        assert_eq!(n2, vec![Some("lib".into()), Some("lib-other".into())]);
        let cx3 = PathCtx {
            base: Some(&proj),
            folders: &dup,
            names: &n2,
            home: None,
            vars: &[],
        };
        for (p, want) in [
            (lib.join("b.sql"), "${workspaceFolder:lib}/b.sql"),
            (lib2.join("c.sql"), "${workspaceFolder:lib-other}/c.sql"),
        ] {
            assert_eq!(relativize(&p, &cx3), want);
            assert_eq!(resolve(want, &cx3), p);
        }
        let clash = [(lib2.clone(), "LIB".to_string())];
        assert_eq!(anchor_names(&dup, &clash), vec![None, None]);
        let bad = [(lib2, "a}b".to_string())];
        assert_eq!(anchor_names(&dup, &bad), vec![Some("lib".into()), None]);
        // 이 PC에 없는 폴더 앵커 = 글자 그대로 프로젝트 폴더 아래로 → 저장하면 같은 글로 되돌아간다(잃지 않는다).
        let raw = "${workspaceFolder:gone}/c.sql";
        assert_eq!(relativize(&resolve(raw, &cx), &cx), raw);
    }

    /// ★ T-172 P2 `folders[].name`: 파일에 적힌 이름이 왕복하고(없는 폴더는 `"name"` 키 자체가 없다) · 그 이름으로 탭 앵커가 풀린다.
    #[test]
    fn folder_name_round_trips_and_names_the_anchor() {
        let root = PathBuf::from(if cfg!(windows) { "Q:\\w" } else { "/w" });
        let file = root.join("proj").join("p.nsql-project");
        let (a, b) = (root.join("x").join("lib"), root.join("y").join("lib"));
        let s = |p: &Path| p.to_string_lossy().replace('\\', "/");
        let src = format!(
            r#"{{"version": 2, "folders": [{{"path": "."}}, {{"path": "{}", "name": "lib-x"}}, {{"path": "{}", "name": " lib-y "}}], "tabs": [{{"path": "${{workspaceFolder:lib-y}}/t.sql", "title": "t.sql", "id": 1}}]}}"#,
            s(&a),
            s(&b)
        );
        let p = Project::from_bytes(&file, src.as_bytes()).unwrap();
        assert_eq!(p.folders.len(), 3);
        assert_eq!(
            p.folder_names,
            vec![(a, "lib-x".to_string()), (b.clone(), "lib-y".to_string())]
        );
        assert_eq!(p.tabs[0].path.as_deref(), Some(b.join("t.sql").as_path()));
        let js = p.to_json();
        assert!(
            js.contains(r#""name": "lib-x""#) && js.contains(r#""name": "lib-y""#),
            "{js}"
        );
        assert!(js.contains(r#"{ "path": "." }"#), "{js}");
        assert!(js.contains("${workspaceFolder:lib-y}/t.sql"), "{js}");
        assert_eq!(Project::from_bytes(&file, js.as_bytes()).unwrap(), p);
        // 폴더를 빼면 그 이름 항목은 저장 때 따라 나가지 않는다.
        let mut q = p;
        q.remove_folder(1);
        assert!(!q.to_json().contains("lib-x"));
    }

    /// ★ T-172 (D-188): 절대로 남는 경로만 센다 — 프로젝트 폴더 아래(상대) · 등록 폴더 아래(앵커)는 빼고 · 같은 경로는 한 번.
    #[test]
    fn absolute_path_count_counts_only_non_portable_paths() {
        let root = PathBuf::from(if cfg!(windows) { "Q:\\w" } else { "/w" });
        let (proj, lib, far) = (
            root.join("proj"),
            root.join("ext").join("lib"),
            root.join("far").join("c.sql"),
        );
        let tab = |p: PathBuf| TabState {
            path: Some(p),
            ..TabState::default()
        };
        let mut p = Project::default().with_path(&proj.join("p.nsql-project"));
        assert_eq!(p.absolute_path_count(), 0);
        p.folders = vec![proj.join("sql")];
        p.tabs = vec![tab(proj.join("sql").join("a.sql")), TabState::default()];
        assert_eq!(p.absolute_path_count(), 0, "프로젝트 폴더 아래 = 상대");
        // 프로젝트 밖 등록 폴더 = 폴더 정의 자신은 절대(1) · 그 아래 탭은 앵커(0).
        p.folders.push(lib.clone());
        p.tabs.push(tab(lib.join("b.sql")));
        assert_eq!(p.absolute_path_count(), 1);
        // 어느 앵커에도 안 걸리는 탭 = 절대 · 같은 경로 두 탭은 한 번.
        p.tabs.push(tab(far.clone()));
        p.tabs.push(tab(far));
        assert_eq!(p.absolute_path_count(), 2);
        assert!(is_absolute_text("//srv/share/a.sql") && is_absolute_text("C:/a.sql"));
        assert!(!is_absolute_text("sql/a.sql") && !is_absolute_text("${userHome}/a.sql"));
        assert!(!is_absolute_text(".") && !is_absolute_text(""));
    }

    /// ★ T-172 (A)(B): 고유 번호 왕복 · 다시 지정한 폴더는 이 PC에서만 바뀌고(파일에는 정의 경로 그대로 · 앵커 이름도 정의 쪽) ·
    ///   대응표로 다시 열면 같은 자리로 풀린다.
    #[test]
    fn pc_paths_relocate_keeps_shared_definition() {
        let root = std::env::temp_dir().join(format!("nsql-pcpaths-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (pdir, there) = (root.join("p"), root.join("elsewhere").join("lib2"));
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::create_dir_all(&there).unwrap();
        std::fs::write(there.join("b.sql"), "select 1").unwrap();
        let file = pdir.join("my.nsql-project");
        let gone = root.join("gone").join("lib");
        let gone_txt = gone.to_string_lossy().replace('\\', "/");
        let doc = format!(
            r#"{{"version":2,"id":"00aa11bb22cc33dd","folders":[{{"path":"{gone_txt}"}}],"active":0,"tabs":[{{"path":"${{workspaceFolder:lib}}/b.sql","title":"b.sql"}}]}}"#
        );
        let mut p = Project::from_bytes(&file, doc.as_bytes()).unwrap();
        assert_eq!(p.id.as_deref(), Some("00aa11bb22cc33dd"));
        assert_eq!(p.tabs[0].path, Some(gone.join("b.sql")));
        let key = p.folder_key(0);
        // 열쇠 = 파일에 적히는 글(홈 아래면 `${userHome}/…` · 아니면 절대).
        assert!(key.ends_with("gone/lib"), "{key}");
        // 다시 지정: 실제 경로만 바뀐다 · 탭 경로가 따라온다.
        assert_eq!(p.relocate_folder(0, &there), Some((gone, there.clone())));
        assert_eq!(p.folders[0], there);
        assert_eq!(p.tabs[0].path, Some(there.join("b.sql")));
        // 저장할 글 = 정의 경로 그대로 · 앵커 이름도 정의 쪽(lib) · 이 PC의 자리(lib2 · elsewhere)는 새지 않는다.
        let json = p.to_json();
        assert!(
            json.contains(&key) && json.contains("${workspaceFolder:lib}/b.sql"),
            "{json}"
        );
        assert!(
            !json.contains("elsewhere") && json.contains("\"id\": \"00aa11bb22cc33dd\""),
            "{json}"
        );
        // 대응표 → 다시 열면 같은 자리.
        let table = pc_paths_set("", "00aa11bb22cc33dd", &key, &there);
        let again = Project::from_bytes_pc(&file, json.as_bytes(), &table).unwrap();
        assert_eq!(again.folders[0], there);
        assert_eq!(again.tabs[0].path, Some(there.join("b.sql")));
        assert_eq!(again.to_json(), json);
        // 다른 고유 번호의 줄은 쓰지 않는다 · 같은 열쇠는 바꿔 쓴다 · 폴더 제거는 정의 경로도 함께.
        let other = pc_paths_set(&table, "ffff", &key, &root);
        assert_eq!(
            pc_paths_get(&other, "00aa11bb22cc33dd", &key),
            Some(there.clone())
        );
        let moved = pc_paths_set(&other, "00aa11bb22cc33dd", &key, &root);
        assert_eq!(
            pc_paths_get(&moved, "00aa11bb22cc33dd", &key),
            Some(root.clone())
        );
        assert_eq!(moved.split('|').count(), 6);
        let mut q = again;
        assert_eq!(q.remove_folder(0), Some(there));
        assert!(q.folder_src.is_empty() && q.folders.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// ★ T-172: 더 새 형식의 프로젝트 파일은 열지 않는다 · 폴더는 키 순서와 무관하게 먼저 읽힌다.
    /// T-172 P3: 경로 변수 — `vars`의 글은 그대로 보존 · 그 아래 탭·등록 폴더는 `${var:이름}/…`로 적히고 다시 풀린다 ·
    /// 더 깊게 맞는 쪽이 이긴다 · 정의되지 않은 변수는 글자 그대로 보존 · 변수를 쓰는 파일만 version 3.
    #[test]
    fn path_vars_round_trip_and_version() {
        let (file, data, deep) = if cfg!(windows) {
            ("Q:\\w\\p\\my.nsql-project", "Q:/w/data", "Q:/w/data/deep")
        } else {
            ("/w/p/my.nsql-project", "/w/data", "/w/data/deep")
        };
        let file = PathBuf::from(file);
        let doc = format!(
            r#"{{"version":3,"vars":{{"DATA":"{data}","REL":"../shared","DEEP":"{deep}","bad}}":"x"}},"folders":[{{"path":"${{var:DATA}}/lib"}}],"tabs":[{{"path":"${{var:DATA}}/a.sql","title":"a"}},{{"path":"${{var:DEEP}}/b.sql","title":"b"}},{{"path":"${{var:REL}}/c.sql","title":"c"}},{{"path":"${{var:NOPE}}/d.sql","title":"d"}}]}}"#
        );
        let p = Project::from_bytes(&file, doc.as_bytes()).unwrap();
        assert_eq!(p.path_vars.len(), 3, "{:?}", p.path_vars);
        let data = PathBuf::from(data);
        assert_eq!(p.folders, vec![data.join("lib")]);
        assert_eq!(
            p.tabs[0].path.as_deref(),
            Some(data.join("a.sql").as_path())
        );
        assert_eq!(
            p.tabs[1].path.as_deref(),
            Some(data.join("deep").join("b.sql").as_path())
        );
        let base = file.parent().unwrap();
        assert_eq!(
            p.tabs[2].path.as_deref(),
            Some(base.join("../shared").join("c.sql").as_path())
        );
        // 정의되지 않은 변수 = 글자 그대로(프로젝트 폴더 아래) → 저장하면 같은 글.
        assert_eq!(
            p.tabs[3].path.as_deref(),
            Some(base.join("${var:NOPE}/d.sql").as_path())
        );
        let js = p.to_json();
        assert!(js.contains("\"version\": 3"), "{js}");
        assert!(
            js.contains(&format!(
                "\"DATA\": \"{}\"",
                data.to_string_lossy().replace('\\', "/")
            )),
            "{js}"
        );
        assert!(js.contains("\"REL\": \"../shared\""), "{js}");
        for want in [
            "\"path\": \"${var:DATA}/lib\"",
            "\"path\": \"${var:DATA}/a.sql\"",
            "\"path\": \"${var:DEEP}/b.sql\"",
            // 프로젝트 폴더 기준 상대로 정의한 변수 = 프로젝트 기준 상대가 먼저 맞는다(똑같이 따라오는 경로).
            "\"path\": \"../shared/c.sql\"",
            "\"path\": \"${var:NOPE}/d.sql\"",
        ] {
            assert!(
                js.contains(want),
                "{want}
{js}"
            );
        }
        assert_eq!(
            Project::from_bytes(&file, js.as_bytes()).unwrap().to_json(),
            js
        );
        // 변수가 없으면 2 그대로(옛 판이 계속 연다) · 4는 거절.
        let plain = Project::from_bytes(&file, br#"{"version":2,"folders":[]}"#).unwrap();
        assert!(plain.to_json().contains("\"version\": 2"));
        assert!(Project::from_bytes(&file, br#"{"version":4,"folders":[]}"#).is_err());
    }

    /// T-172 P3 2단계: 변수의 이 PC 자리 — 미정의 변수·이 PC에 없는 공유 자리를 대응표(`var:이름`)가 채운다 · 탭이 새 자리로
    /// 풀리고 · 저장하면 `${var:이름}/…` 그대로(이 PC의 경로는 파일에 새지 않는다) · 공유 `vars` 글은 보존.
    #[test]
    fn var_pc_override_fills_missing_and_keeps_shared_text() {
        let root = std::env::temp_dir().join(format!("nsql-varpc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (proj, here, gone) = (root.join("p"), root.join("here"), root.join("gone"));
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&here).unwrap();
        // 두 변수는 서로 다른 자리(같은 폴더를 가리키면 어느 이름으로 적을지 정해지지 않는다).
        let here2 = root.join("here2");
        std::fs::create_dir_all(&here2).unwrap();
        let file = proj.join("my.nsql-project");
        let gone_txt = gone.to_string_lossy().replace('\\', "/");
        let doc = format!(
            r#"{{"version":3,"id":"00aa11bb22cc33dd","vars":{{"DATA":"{gone_txt}"}},"tabs":[{{"path":"${{var:DATA}}/a.sql","title":"a"}},{{"path":"${{var:NOPE}}/sub/b.sql","title":"b"}}]}}"#
        );
        let p0 = Project::from_bytes(&file, doc.as_bytes()).unwrap();
        // 못 찾은 경로가 어느 변수 아래인지(정의된 것 · 미정의 앵커).
        assert_eq!(
            p0.var_of_path(p0.tabs[0].path.as_ref().unwrap()),
            Some(("DATA".into(), PathBuf::from("a.sql")))
        );
        assert_eq!(
            p0.var_of_path(p0.tabs[1].path.as_ref().unwrap()),
            Some(("NOPE".into(), PathBuf::from("sub").join("b.sql")))
        );
        assert_eq!(p0.var_of_path(&proj.join("plain.sql")), None);
        let table = pc_paths_set("", "00aa11bb22cc33dd", "var:DATA", &here);
        let table = pc_paths_set(&table, "00aa11bb22cc33dd", "var:NOPE", &here2);
        assert_eq!(pc_paths_vars(&table, "00aa11bb22cc33dd").len(), 2);
        assert!(pc_paths_vars(&table, "ffff").is_empty());
        let p = Project::from_bytes_pc(&file, doc.as_bytes(), &table).unwrap();
        assert_eq!(
            p.tabs[0].path.as_deref(),
            Some(here.join("a.sql").as_path())
        );
        assert_eq!(
            p.tabs[1].path.as_deref(),
            Some(here2.join("sub").join("b.sql").as_path())
        );
        let js = p.to_json();
        // 변수 아래 등록 폴더의 적어 둔 이름은 변수 자리를 옮겨도 남는다.
        let named = format!(
            r#"{{"version":3,"id":"00aa11bb22cc33dd","vars":{{"DATA":"{gone_txt}"}},"folders":[{{"path":"${{var:DATA}}/lib","name":"lib-x"}}]}}"#
        );
        let pn = Project::from_bytes_pc(&file, named.as_bytes(), &table).unwrap();
        assert_eq!(pn.folders, vec![here.join("lib")]);
        let jn = pn.to_json();
        assert!(
            jn.contains("\"path\": \"${var:DATA}/lib\", \"name\": \"lib-x\""),
            "{jn}"
        );
        assert!(js.contains(&format!("\"DATA\": \"{gone_txt}\"")), "{js}");
        assert!(js.contains("\"path\": \"${var:DATA}/a.sql\""), "{js}");
        assert!(js.contains("\"path\": \"${var:NOPE}/sub/b.sql\""), "{js}");
        assert!(!js.contains("here"), "{js}");
        // 공유 자리가 이 PC에 실제로 있으면 대응표를 쓰지 않는다(폴더와 같은 규칙).
        std::fs::create_dir_all(&gone).unwrap();
        let p2 = Project::from_bytes_pc(&file, doc.as_bytes(), &table).unwrap();
        assert_eq!(
            p2.tabs[0].path.as_deref(),
            Some(gone.join("a.sql").as_path())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn newer_version_is_refused_and_folders_resolve_first() {
        let file = PathBuf::from(if cfg!(windows) {
            "Q:\\w\\p\\my.nsql-project"
        } else {
            "/w/p/my.nsql-project"
        });
        assert!(Project::from_bytes(&file, br#"{"version":99,"folders":[]}"#).is_err());
        let ext = if cfg!(windows) {
            "Q:/w/ext/lib"
        } else {
            "/w/ext/lib"
        };
        let doc = format!(
            r#"{{"version":2,"tabs":[{{"path":"${{workspaceFolder:lib}}/b.sql","title":"b"}}],"folders":[{{"path":"{ext}"}}]}}"#
        );
        let p = Project::from_bytes(&file, doc.as_bytes()).unwrap();
        assert_eq!(p.tabs[0].path, Some(PathBuf::from(ext).join("b.sql")));
    }

    #[test]
    fn json_round_trip_keeps_folders_relative_under_base() {
        let dir = std::env::temp_dir().join(format!("nsql-proj-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("sql"));
        let file = dir.join("my.nsql-project");
        let mut p = Project {
            path: Some(file.clone()),
            ..Project::default()
        };
        assert!(p.add_folder(&dir.join("sql")));
        assert!(!p.add_folder(&dir.join("sql")));
        let other = if cfg!(windows) {
            PathBuf::from("Q:\\elsewhere")
        } else {
            PathBuf::from("/elsewhere")
        };
        p.add_folder(&other);
        // 마지막 선택 위치도 상대 경로로 저장·복원(사용자 09-22).
        p.last_selected = Some(dir.join("sql/a.sql"));
        let text = p.to_json();
        assert!(text.contains("\"path\": \"sql\""), "{text}");
        assert!(text.contains("\"selected\": \"sql/a.sql\""), "{text}");
        assert!(!text.contains('\\'), "slashes only: {text}");
        p.save().unwrap();
        let back = Project::load(&file).unwrap();
        assert_eq!(back.folders[0], dir.join("sql"));
        assert_eq!(back.folders[1], other);
        assert_eq!(back.name().as_deref(), Some("my"));
        assert_eq!(back.last_selected, Some(dir.join("sql/a.sql")));
        // 작업 환경(탭 · 활성 · 북마크 JSON) 왕복(사용자 09-23).
        p.tabs = vec![
            TabState {
                path: Some(dir.join("sql/a.sql")),
                title: "a.sql".into(),
                line: 3,
                col: 2,
                anchor_text: "SELECT 1;".into(),
                before: "-- a".into(),
                after: "".into(),
                ..TabState::default()
            },
            TabState {
                title: "Script_2".into(),
                text: Some("select \"q\"\n from dual".into()),
                ..TabState::default()
            },
        ];
        p.active = 1;
        p.bookmarks = Some("{\"version\": 1, \"items\": []}".into());
        // 좌측 패널 상태 + 미리보기 표식(사용자 09-23) — 상대 경로로 쓰고 절대로 돌아온다.
        p.tabs[0].preview = true;
        // 파일 탭의 미저장 본문 + 디스크 해시(64비트 → 글자열로 왕복) + 탭 id(사용자 09-23).
        p.tabs[0].text = Some("-- unsaved edit".into());
        p.tabs[0].disk_hash = 0xFFFF_FFFF_FFFF_FF01;
        p.tabs[0].id = 77;
        // 본문(payload)은 `id`로 탭에 되붙는다(`projfile` 블록 · 09-23) — 스크립트 탭도 id가 있어야 본문이 왕복한다.
        p.tabs[1].id = 78;
        // 탭별 들여쓰기 재정의(사용자 09-29) — 있는 탭만 쓰고 그대로 돌아온다.
        p.tabs[1].indent = Some((2, true));
        // 뷰 탭 열쇠(사용자 09-30) — 있는 탭만 쓰고 그대로 돌아온다.
        p.tabs[1].view = Some("ext:sql-formatter-kiros33".into());
        p.expanded = vec![dir.join("sql"), other.join("x")];
        p.panel = Some("bookmarks".into());
        p.search = Some("select \"q\"".into());
        p.bm_collapsed = vec![0, 7];
        p.profiles = vec!["APPDB".into(), "Local".into()];
        p.save().unwrap();
        let text = p.to_json();
        assert!(text.contains("\"preview\": true"), "{text}");
        assert!(text.contains("\"tab_size\": 2, \"spaces\": true"), "{text}");
        assert!(
            text.contains("\"view\": \"ext:sql-formatter-kiros33\""),
            "{text}"
        );
        assert!(text.contains("\"expanded\": [\"sql\", "), "{text}");
        let back = Project::load(&file).unwrap();
        assert_eq!(back.tabs, p.tabs);
        assert_eq!(back.active, 1);
        assert_eq!(back.expanded, p.expanded);
        assert_eq!(back.panel.as_deref(), Some("bookmarks"));
        assert_eq!(back.search.as_deref(), Some("select \"q\""));
        assert_eq!(back.bm_collapsed, vec![0, 7]);
        assert_eq!(back.profiles, p.profiles);
        assert!(back
            .bookmarks
            .as_deref()
            .is_some_and(|b| b.contains("\"version\":1")));
        assert!(p.remove_folder(5).is_none());
        assert_eq!(p.remove_folder(0), Some(dir.join("sql")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_non_object_and_accepts_plain_strings() {
        let dir = std::env::temp_dir().join(format!("nsql-proj2-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let bad = dir.join("bad.nsql-project");
        std::fs::write(&bad, "[1,2]").unwrap();
        assert!(Project::load(&bad).is_err());
        let ok = dir.join("ok.nsql-project");
        std::fs::write(
            &ok,
            "{\"version\":1,\"folders\":[\"a\", {\"path\":\"b\"}, 3]}",
        )
        .unwrap();
        let p = Project::load(&ok).unwrap();
        assert_eq!(p.folders, vec![dir.join("a"), dir.join("b")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_list_rules() {
        let dir = std::env::temp_dir().join(format!("nsql-proj3-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("a.nsql-project");
        let b = dir.join("b.nsql-project");
        std::fs::write(&a, "{}").unwrap();
        std::fs::write(&b, "{}").unwrap();
        let raw = push_recent("", &a);
        let raw = push_recent(&raw, &b);
        let raw = push_recent(&raw, &a);
        let v = recent_list(&raw);
        assert_eq!(v, vec![a.clone(), b.clone()]);
        // 없는 파일은 걸러진다.
        let raw2 = format!("{}|{}", dir.join("gone.nsql-project").display(), raw);
        assert_eq!(recent_list(&raw2), vec![a, b]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ★ T-39b(10-30 · 67 §8-5): 설정 절 왕복 — 불/정수/문자열 꼴 · `[sql]` 묶음 · **모르는 키 보존** · 비어 있으면 키를 쓰지 않는다.
    #[test]
    fn settings_section_round_trip_preserves_unknown_keys() {
        let dir = std::env::temp_dir().join(format!("nsql-proj4-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("s.nsql-project");
        std::fs::write(
            &file,
            r#"{"version":1,"settings":{"editor.tab_size":2,"editor.indent_spaces":true,
                "format.keyword_case":"upper","future.unknown":"keep","[SQL]":{"editor.tab_size":4},
                "[json]":{"editor.indent_spaces":false},"[]":{"x":1},"bad":[1,2]}}"#,
        )
        .unwrap();
        let p = Project::load(&file).unwrap();
        assert_eq!(
            p.settings,
            vec![
                (None, "editor.tab_size".to_string(), "2".to_string()),
                (None, "editor.indent_spaces".to_string(), "on".to_string()),
                (None, "format.keyword_case".to_string(), "upper".to_string()),
                (None, "future.unknown".to_string(), "keep".to_string()),
                (
                    Some("sql".to_string()),
                    "editor.tab_size".to_string(),
                    "4".to_string()
                ),
                (
                    Some("json".to_string()),
                    "editor.indent_spaces".to_string(),
                    "off".to_string()
                ),
            ]
        );
        let js = p.to_json();
        assert!(js.contains("\"editor.tab_size\": 2"), "{js}");
        assert!(js.contains("\"editor.indent_spaces\": true"), "{js}");
        assert!(js.contains("\"future.unknown\": \"keep\""), "{js}");
        assert!(js.contains("\"[sql]\": { \"editor.tab_size\": 4 }"), "{js}");
        assert!(
            js.contains("\"[json]\": { \"editor.indent_spaces\": false }"),
            "{js}"
        );
        std::fs::write(&file, js.as_bytes()).unwrap();
        let again = Project::load(&file).unwrap();
        assert_eq!(again.settings, p.settings);
        let empty = Project {
            path: Some(file),
            ..Project::default()
        };
        assert!(!empty.to_json().contains("settings"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
