//! 검색어 이력 부품(사용자 09-23 "각 검색 영역별(박스별)로 최대 20개(설정)의 검색어를 보관 · 프로그램 전역 속성").
//!
//! - **전역 한 파일** `<설정 폴더>/search-history.json`(`NSQL_HOME` 격리 규약 · 프로젝트·폴더 모드와 무관) — 상자 이름 → 최근순 목록.
//! - **상자마다 이름**(`find.query` · `find.replace` · `search.query` · `search.where` · `filter.project` · `filter.bookmarks` ·
//!   `filter.extensions` · `filter.outline` · `prefs.search`) — 같은 이름을 쓰는 상자는 이력을 공유한다.
//! - **기록 시점** = Enter(찾기 실행 · 파일 검색 실행 · 필터 확정) · 상자가 포커스를 잃을 때(비어 있지 않으면) — 타이핑마다 쌓지 않는다.
//!   같은 글은 앞으로 옮기고(중복 없음) 상한(`search.history_max` · 0 = 끔)을 넘으면 오래된 것부터 버린다 · 바뀌면 바로 쓴다(작은 파일).
//! - **되부르기** = 상자에서 ↑(오래된 쪽) / ↓(새로운 쪽 → 마지막엔 치던 글로 복귀) — Sublime·VS Code 찾기 상자와 같다([`Recall`]).
//!   상자 글이 밖에서 바뀌면(타이핑) 위치를 처음으로(`TextBox::text_rev`).
//! - 호스트는 [`SharedHistory`] 하나를 만들어 상자를 가진 패널마다 건네고, 패널은 [`Recall`]로 키·확정을 잇는다(30 §2 부품).

use nexa_ctl::{InputEvent, Invalidations, Key as CtlKey, TextBox};
use nsql_settings::json::{dump, parse, Json};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// 전역 이력 파일 이름(설정 폴더 바로 아래) · 상한 기본값 = 설정 `search.history_max`(20).
pub(crate) const FILE_NAME: &str = "search-history.json";

/// 상자 이름 → 최근순 검색어.
pub(crate) struct SearchHistory {
    max: usize,
    boxes: Vec<(String, Vec<String>)>,
    /// 없으면 메모리 전용(시험 · 설정 폴더를 모를 때).
    path: Option<PathBuf>,
}

pub(crate) type SharedHistory = Rc<RefCell<SearchHistory>>;

impl SearchHistory {
    pub(crate) fn new(max: usize) -> Self {
        SearchHistory {
            max,
            boxes: Vec::new(),
            path: None,
        }
    }

    /// 설정 폴더에서 읽는다(없거나 깨지면 빈 이력 · 쓰기는 같은 자리).
    pub(crate) fn load_in(dir: Option<&Path>, max: usize) -> Self {
        let mut h = Self::new(max);
        let Some(dir) = dir else {
            return h;
        };
        let p = dir.join(FILE_NAME);
        if let Ok(text) = std::fs::read_to_string(&p) {
            match Self::from_json(&text, max) {
                Ok(loaded) => h = loaded,
                Err(e) => eprintln!("search-history: {}: {e}", p.display()),
            }
        }
        h.path = Some(p);
        h
    }

    pub(crate) fn shared(self) -> SharedHistory {
        Rc::new(RefCell::new(self))
    }

    /// 상한 변경(설정 창) — 줄어들면 잘라 내고 저장 · 0 = 끔(목록 비움 · 기록 안 함).
    pub(crate) fn set_max(&mut self, max: usize) {
        if self.max == max {
            return;
        }
        self.max = max;
        let mut changed = false;
        for (_, v) in &mut self.boxes {
            if v.len() > max {
                v.truncate(max);
                changed = true;
            }
        }
        self.boxes.retain(|(_, v)| !v.is_empty());
        if changed {
            self.save();
        }
    }

    /// 그 상자의 최근순 목록(없으면 빈 슬라이스).
    pub(crate) fn list(&self, key: &str) -> &[String] {
        self.boxes
            .iter()
            .find(|(k, _)| k == key)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    /// 검색어 기록 — 앞에 넣고 같은 글은 하나만 · 빈 글(공백뿐)은 무시 · 바뀌었으면 저장하고 true.
    pub(crate) fn push(&mut self, key: &str, term: &str) -> bool {
        if self.max == 0 || term.trim().is_empty() {
            return false;
        }
        let idx = match self.boxes.iter().position(|(k, _)| k == key) {
            Some(i) => i,
            None => {
                self.boxes.push((key.to_string(), Vec::new()));
                self.boxes.len() - 1
            }
        };
        let v = &mut self.boxes[idx].1;
        if v.first().is_some_and(|f| f == term) {
            return false;
        }
        v.retain(|t| t != term);
        v.insert(0, term.to_string());
        v.truncate(self.max);
        self.save();
        true
    }

    /// 한 상자 비우기(메뉴용 · 저장).
    #[allow(dead_code)]
    pub(crate) fn clear(&mut self, key: &str) {
        let before = self.boxes.len();
        self.boxes.retain(|(k, _)| k != key);
        if self.boxes.len() != before {
            self.save();
        }
    }

    fn save(&self) {
        let Some(p) = &self.path else {
            return;
        };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, self.to_json()).is_ok() {
            let _ = std::fs::rename(&tmp, p);
        }
    }

    pub(crate) fn to_json(&self) -> String {
        let boxes = self
            .boxes
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| {
                (
                    k.clone(),
                    Json::Arr(v.iter().map(|t| Json::Str(t.clone())).collect()),
                )
            })
            .collect();
        dump(&Json::Obj(vec![
            ("version".into(), Json::Num(1.0)),
            ("boxes".into(), Json::Obj(boxes)),
        ]))
    }

    /// 모르는 키는 무시 · 문자열이 아닌 항목은 그 항목만 버림 · 읽을 때도 상한을 적용.
    pub(crate) fn from_json(text: &str, max: usize) -> Result<Self, String> {
        let root = parse(text)?;
        let Json::Obj(fields) = root else {
            return Err("root must be an object".into());
        };
        let mut h = Self::new(max);
        if let Some((_, Json::Obj(boxes))) = fields.iter().find(|(k, _)| k == "boxes") {
            for (name, v) in boxes {
                let Json::Arr(items) = v else { continue };
                let mut list: Vec<String> = Vec::new();
                for it in items {
                    if let Json::Str(s) = it {
                        if !s.trim().is_empty() && !list.contains(s) {
                            list.push(s.clone());
                        }
                    }
                }
                list.truncate(max);
                if !list.is_empty() {
                    h.boxes.push((name.clone(), list));
                }
            }
        }
        Ok(h)
    }
}

/// 상자 하나의 되부르기 상태(패널이 상자마다 하나 든다).
pub(crate) struct Recall {
    key: String,
    /// 지금 보이는 이력 위치(0 = 가장 최근) · None = 치던 글.
    pos: Option<usize>,
    /// 되부르기 시작 전에 치던 글(↓로 끝까지 내려오면 복귀).
    draft: String,
    /// 마지막으로 우리가 넣은 글의 본문 세대 — 다르면 사용자가 그 뒤 타이핑한 것.
    rev: u64,
}

impl Recall {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Recall {
            key: key.into(),
            pos: None,
            draft: String::new(),
            rev: u64::MAX,
        }
    }

    /// ↑/↓(수식키 없이)면 이력을 상자에 넣고 true(사건 소비 · 호출자는 상자 글로 다시 거른다). 그 외 false.
    /// 이력이 비었으면 소비하지 않는다(단일행 상자에서 ↑/↓는 어차피 하는 일이 없다).
    pub(crate) fn on_key(
        &mut self,
        ev: &InputEvent,
        tb: &mut TextBox,
        h: &SharedHistory,
        inv: &mut Invalidations,
    ) -> bool {
        let InputEvent::Key {
            key: key @ (CtlKey::Up | CtlKey::Down),
            shift: false,
            primary: false,
        } = ev
        else {
            return false;
        };
        let list: Vec<String> = h.borrow().list(&self.key).to_vec();
        if list.is_empty() {
            return false;
        }
        if tb.text_rev() != self.rev {
            // 그 사이 타이핑했다 — 지금 글이 새 초안.
            self.pos = None;
            self.draft = tb.text();
        }
        let next = match (*key, self.pos) {
            (CtlKey::Up, None) => Some(0),
            (CtlKey::Up, Some(p)) => Some((p + 1).min(list.len() - 1)),
            (CtlKey::Down, None) => return true,
            (CtlKey::Down, Some(0)) => None,
            (CtlKey::Down, Some(p)) => Some(p - 1),
            _ => return false,
        };
        let text = next.map_or_else(|| self.draft.clone(), |i| list[i].clone());
        self.pos = next;
        tb.set_text(&text);
        let n = text.chars().count();
        tb.select_range(n, n, inv);
        self.rev = tb.text_rev();
        true
    }

    /// 상자의 글을 이력에 올린다(Enter · 포커스 잃음) — 되부르기 위치는 처음으로.
    pub(crate) fn commit(&mut self, tb: &TextBox, h: &SharedHistory) {
        let text = tb.text();
        self.pos = None;
        self.rev = u64::MAX;
        h.borrow_mut().push(&self.key, &text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_dedups_fronts_and_caps() {
        let mut h = SearchHistory::new(3);
        assert!(h.push("find.query", "a"));
        assert!(h.push("find.query", "b"));
        assert!(!h.push("find.query", "  "), "빈 글은 무시");
        assert!(!h.push("find.query", "b"), "맨 앞과 같으면 변화 없음");
        assert!(h.push("find.query", "a"), "같은 글은 앞으로");
        assert_eq!(h.list("find.query"), ["a", "b"]);
        h.push("find.query", "c");
        h.push("find.query", "d");
        assert_eq!(
            h.list("find.query"),
            ["d", "c", "a"],
            "상한 3 = 오래된 b 탈락"
        );
        assert!(h.list("other").is_empty());
        h.set_max(1);
        assert_eq!(h.list("find.query"), ["d"]);
        h.set_max(0);
        assert!(!h.push("find.query", "x"), "0 = 끔");
    }

    #[test]
    fn json_roundtrip_and_file() {
        let mut h = SearchHistory::new(20);
        h.push("find.query", "select \"a\"\n");
        h.push("find.query", "한글 ㄱ");
        h.push("filter.project", "*.sql");
        let text = h.to_json();
        let back = SearchHistory::from_json(&text, 20).expect("parse");
        assert_eq!(back.list("find.query"), ["한글 ㄱ", "select \"a\"\n"]);
        assert_eq!(back.list("filter.project"), ["*.sql"]);
        // 읽을 때 상한 적용 · 모르는 키 무시 · 문자열 아닌 항목만 버림.
        let odd = r#"{"version":1,"boxes":{"find.query":["x",1,"y","z"]},"extra":true}"#;
        let b = SearchHistory::from_json(odd, 2).expect("parse");
        assert_eq!(b.list("find.query"), ["x", "y"]);
        // 파일: 격리 임시 폴더에만 쓴다(실제 설정 폴더 금지).
        let dir = std::env::temp_dir().join(format!("nsql-sh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut on_disk = SearchHistory::load_in(Some(&dir), 20);
        assert!(on_disk.list("find.query").is_empty());
        on_disk.push("find.query", "q1");
        assert!(dir.join(FILE_NAME).exists(), "바뀌면 바로 쓴다");
        let again = SearchHistory::load_in(Some(&dir), 20);
        assert_eq!(again.list("find.query"), ["q1"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn key(k: CtlKey) -> InputEvent {
        InputEvent::Key {
            key: k,
            shift: false,
            primary: false,
        }
    }

    /// ↑ = 최근부터 오래된 쪽 · 끝에서 더 올라가지 않음 · ↓ = 새로운 쪽 → 치던 글 복귀 · 타이핑하면 위치 초기화 · 이력 없으면 소비 안 함.
    #[test]
    fn recall_steps_through_history_and_restores_draft() {
        let h = SearchHistory::new(20).shared();
        let mut tb = TextBox::new("");
        let mut inv = Invalidations::default();
        let mut r = Recall::new("find.query");
        assert!(
            !r.on_key(&key(CtlKey::Up), &mut tb, &h, &mut inv),
            "이력 없음 = 통과"
        );
        tb.set_text("old");
        r.commit(&tb, &h);
        tb.set_text("new");
        r.commit(&tb, &h);
        tb.set_text("typing");
        assert!(r.on_key(&key(CtlKey::Up), &mut tb, &h, &mut inv));
        assert_eq!(tb.text(), "new");
        assert!(r.on_key(&key(CtlKey::Up), &mut tb, &h, &mut inv));
        assert_eq!(tb.text(), "old");
        assert!(r.on_key(&key(CtlKey::Up), &mut tb, &h, &mut inv));
        assert_eq!(tb.text(), "old", "가장 오래된 것에서 멈춤");
        assert!(r.on_key(&key(CtlKey::Down), &mut tb, &h, &mut inv));
        assert_eq!(tb.text(), "new");
        assert!(r.on_key(&key(CtlKey::Down), &mut tb, &h, &mut inv));
        assert_eq!(tb.text(), "typing", "치던 글로 복귀");
        // 되부른 뒤 타이핑 → 다음 ↑는 다시 최근부터.
        assert!(r.on_key(&key(CtlKey::Up), &mut tb, &h, &mut inv));
        tb.set_text("new2");
        assert!(r.on_key(&key(CtlKey::Up), &mut tb, &h, &mut inv));
        assert_eq!(tb.text(), "new");
        // Shift+↑는 손대지 않는다.
        let shifted = InputEvent::Key {
            key: CtlKey::Up,
            shift: true,
            primary: false,
        };
        assert!(!r.on_key(&shifted, &mut tb, &h, &mut inv));
    }
}
