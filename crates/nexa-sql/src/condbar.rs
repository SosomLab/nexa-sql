//! ★ 결과 그리드 **인라인 조건 입력란**(DBeaver 조건 바 · 사용자 10-06 · T-181 후속) — 필터 줄(칩) 자리를 조건 바로 쓴다.
//! 한 줄 입력(`TextBox`)에 `WHERE` 뒤에 올 식을 치면 **Enter = 실행**: 호스트가 출처 문장을 `SELECT * FROM ( <출처> ) q WHERE 1=1
//! AND (<조건>)`으로 감싸 **같은 결과 탭**에서 다시 돈다(오류는 실행 오류 그대로 = 토스트). 자동 완성 = 결과 열 이름 + 연산 낱말
//! (`AND` `OR` `NOT` `LIKE` `IN` `IS NULL` …) + 방언 내장 함수 — 캐럿 앞 낱말로 거른 팝업(`ContextMenu`) · Tab/Enter/클릭 = 넣기 · Esc = 닫기.
//! 맨 오른쪽 = **조회 SQL 복사** 버튼(`copybtn` 부품 · 눌림 애니메이션). 설정 `grid.condition_bar`(기본 보임).
//! **검증(사용자 10-06)**: Enter 때 연결된 탭의 **방언** 기준으로 식을 본다 — 괄호·따옴표 짝 · `;`·문장 시작 낱말 금지 · 식별자는
//! 결과 열(`q.열`도) · 연산 낱말 · 방언 내장 함수(nsql-script `builtins` · 뒤에 `(`)만 · PostgreSQL 전용 `::`은 다른 방언에서 오류.
//! 틀리면 실행하지 않고 상자 테두리 빨강 + 상태줄 원인. 완성 후보도 같은 기준(열 → 낱말 → 방언 함수).
use crate::copybtn::CopyBtn;
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key, TextBox, Widget};
use nsql_core::Dialect;
use nsql_i18n::{t, tf, Msg};
use std::time::Instant;

/// 자동 완성에 넣는 연산 낱말(열 이름 뒤에 · 접두가 맞을 때만).
const KEYWORDS: &[&str] = &[
    "AND",
    "OR",
    "NOT",
    "LIKE",
    "IN",
    "IS NULL",
    "IS NOT NULL",
    "BETWEEN",
    "EXISTS",
];

/// 식 안에서 허용하는 낱말(검증 · 대소문자 무시) — 연산·논리·CASE·리터럴.
const ALLOWED_WORDS: &[&str] = &[
    "AND",
    "OR",
    "NOT",
    "LIKE",
    "ILIKE",
    "IN",
    "IS",
    "NULL",
    "BETWEEN",
    "EXISTS",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "ESCAPE",
    "TRUE",
    "FALSE",
    "ANY",
    "ALL",
    "SOME",
    "DISTINCT",
    "AS",
    "Q",
    "REGEXP",
    "RLIKE",
    "GLOB",
    "DATE",
    "TIMESTAMP",
    "INTERVAL",
    "COLLATE",
    "SIMILAR",
    "TO",
];

/// 조건 자리에 올 수 없는 문장 낱말(다른 문장 끼워 넣기 방지).
const STATEMENT_WORDS: &[&str] = &[
    "SELECT", "INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER", "TRUNCATE", "GRANT",
    "REVOKE", "MERGE", "UNION", "WITH", "EXEC", "EXECUTE", "CALL", "BEGIN", "COMMIT", "ROLLBACK",
];

pub(crate) struct CondBar {
    tb: TextBox,
    copy: CopyBtn,
    menu: CtxMenu,
    rect: Rect,
    scale: f32,
    /// 결과 열 이름(완성 후보 · 검증).
    cols: Vec<String>,
    /// 완성 중인 낱말의 글자 범위 `[start, caret)`.
    comp: Option<(usize, usize)>,
    /// Enter로 확정된 조건 글(호스트가 가져가 감싸서 실행).
    run: Option<String>,
    /// 복사 버튼 눌림(호스트가 감싼 SQL을 클립보드로).
    copy_req: bool,
    /// 연결된 탭의 방언(검증 · 함수 후보).
    dialect: Dialect,
    /// 마지막 검증 오류(상자 테두리 빨강 · 글이 바뀌면 지움) · 호스트에 한 번 알렸는가(상태줄 1회).
    err: Option<String>,
    err_reported: bool,
    /// 보기 상태(사용자 10-06 개념 변경): 상자는 늘 여러 줄 · `expanded` = 최대 줄 수(`max_lines` · 설정 `grid.cond_max_lines`)까지
    /// 펼쳐 보임 · 아니면 한 줄만(여러 줄이어도 캐럿 줄 하나 · 키·휠로 이동) · `user_collapsed` = 사용자가 ▴로 접었다(줄 바꿈이
    /// 있어도 한 줄 보기 유지 · 비우면 해제).
    expanded: bool,
    user_collapsed: bool,
    max_lines: usize,
    /// 포인터가 바 안에 있다(휠 전달).
    hover_in: bool,
    /// 버튼 자리: ▾/▴(펼치기·접기) · ×(전체 지우기 · 글이 있을 때) · 완성 팝업이 들어갈 호스트(창).
    btn_mode: Rect,
    btn_clear: Rect,
    host: Rect,
    /// 열 머리를 끌어 조건 바 위에 있는 동안(놓을 자리 강조 · 사용자 10-06 "컬럼 DnD").
    drop_hot: bool,
    /// 설정 `grid.cond_drop_template` — DnD 때 `AND` 연결 + 타입별 기본값(`= ''`/`= 0`)을 붙인다(기본 켬).
    drop_template: bool,
    /// 완성 = 편집기와 같은 기준(사용자 10-07): `intel.key_passthrough`(고른 항목 없으면 Enter/Tab은 팝업만 닫고 상자로) ·
    /// `intel.min_chars`(이만큼 치기 전엔 팝업 없음) · 숫자로 시작하는 낱말(`1=1`)엔 팝업 없음.
    key_passthrough: bool,
    min_chars: usize,
}

impl Default for CondBar {
    fn default() -> Self {
        Self::new()
    }
}

impl CondBar {
    pub(crate) fn new() -> Self {
        // 상자는 늘 여러 줄(보이는 줄 수 = 높이) · 줄 바꿈 없음(가로 스크롤) · 지우기 ×는 바가 직접 그린다(오른쪽 위 고정).
        let mut tb = TextBox::new(t(Msg::CondPlaceholder)).with_multiline();
        tb.set_focus_ring(false);
        CondBar {
            tb,
            copy: CopyBtn::new(),
            menu: CtxMenu::new(),
            rect: Rect::default(),
            scale: 1.0,
            cols: Vec::new(),
            comp: None,
            run: None,
            copy_req: false,
            dialect: Dialect::Sqlite,
            err: None,
            err_reported: false,
            expanded: false,
            user_collapsed: false,
            max_lines: 3,
            hover_in: false,
            btn_mode: Rect::default(),
            btn_clear: Rect::default(),
            host: Rect::default(),
            drop_hot: false,
            drop_template: true,
            key_passthrough: true,
            min_chars: 2,
        }
    }

    /// 완성 팝업이 들어갈 영역(창 전체 · 호스트가 그리기 전에).
    pub(crate) fn set_host(&mut self, host: Rect) {
        self.host = host;
    }

    /// 조건 바가 원하는 높이 — 상자(멀티라인) 안쪽 = 위 여백 8 + 줄 20×N(TextBox `paint_multiline`과 같은 값) + 바 위아래 여백 2.
    /// 접힘 = 한 줄(32) · 펼침 = `max_lines`줄. (협업 V1 bin26 E = 26이면 한 줄이 위로 잘렸다.)
    pub(crate) fn wanted_height(&self, scale: f32) -> i32 {
        let s = |v: f32| (v * scale).round() as i32;
        // TextBox 멀티라인은 보이는 줄 수를 `(높이 - 12) / 20`으로 센다 — N줄이 다 보이려면 안쪽 20N+12(사용자 10-07 "3줄이 안 보임").
        //   접힘은 한 줄 = 위 여백 8 + 줄 20 = 28(캐럿 줄 하나).
        if self.expanded {
            s(20.0) * self.max_lines as i32 + s(12.0) + s(2.0) * 2
        } else {
            s(28.0) + s(2.0) * 2
        }
    }

    /// 설정 `grid.cond_max_lines` — 펼쳤을 때 보이는 최대 줄 수(2~12).
    pub(crate) fn set_max_lines(&mut self, n: usize) {
        self.max_lines = n.clamp(2, 12);
    }

    #[cfg(test)]
    pub(crate) fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// 글의 줄 수(빈 글 = 1).
    fn line_count(&self) -> usize {
        self.tb.text().matches('\n').count() + 1
    }

    fn set_expanded(&mut self, on: bool) {
        if self.expanded != on {
            self.expanded = on;
            self.menu.close();
            self.comp = None;
        }
    }

    /// ▾/▴ 버튼 — 펼침 ↔ 접힘. 사용자가 접으면 줄 바꿈이 있어도 한 줄 보기를 유지한다(`user_collapsed`).
    fn toggle_expand(&mut self) {
        if self.expanded {
            self.set_expanded(false);
            self.user_collapsed = true;
        } else {
            self.set_expanded(true);
            self.user_collapsed = false;
        }
    }

    /// 글이 바뀐 뒤 보기 상태 — 두 줄 이상이 되면(Shift+Enter · 여러 줄 붙여넣기) 사용자가 접어 두지 않은 한 자동으로 펼친다 ·
    /// 비면 접고 `user_collapsed`도 푼다.
    fn after_text_changed(&mut self) {
        if self.tb.text().is_empty() {
            self.set_expanded(false);
            self.user_collapsed = false;
        } else if self.line_count() > 1 && !self.expanded && !self.user_collapsed {
            self.set_expanded(true);
        }
    }

    /// × = 전체 지우기(상자 오른쪽 위 · 글이 있을 때만 보인다).
    fn clear_text(&mut self) {
        self.tb.set_text("");
        let _ = self.tb.take_changed();
        self.err = None;
        self.menu.close();
        self.comp = None;
        self.after_text_changed();
    }

    /// 결과가 바뀌면 열 이름을 새로(완성 후보 · 검증).
    pub(crate) fn set_columns(&mut self, cols: Vec<String>) {
        self.cols = cols;
    }

    /// 연결된 탭의 방언(검증·함수 후보의 기준).
    pub(crate) fn set_dialect(&mut self, d: Dialect) {
        self.dialect = d;
    }

    /// 자리(조건 바 사각형) — 오른쪽 위 = [×][▾/▴][복사].
    pub(crate) fn set_rect(&mut self, r: Rect, scale: f32) {
        self.rect = r;
        self.scale = scale;
        let s = |v: f32| (v * scale).round() as i32;
        let (pad_x, pad_y) = (s(4.0), s(2.0));
        let bh = s(18.0);
        // 여러 줄이어도 버튼은 첫 줄 높이(28) 안 가운데 = 오른쪽 위(사용자 10-06 "우측 상단 X 유지").
        let by = r.y + pad_y + (s(28.0) - bh) / 2;
        let btn = Rect::new(r.right() - pad_x - bh, by, bh, bh);
        self.copy.set_rect(btn);
        self.btn_mode = Rect::new(btn.x - s(4.0) - bh, by, bh, bh);
        let mut inv = Invalidations::default();
        self.tb.set_scale(scale);
        // 상자는 ▾ 버튼 바로 앞까지 — × 지우기는 상자 **안** 오른쪽 위에 겹쳐 그린다(사용자 10-07 "X를 밖에 빼지 말고 상자 안에").
        let tb_rect = Rect::new(
            r.x + pad_x,
            r.y + pad_y,
            (self.btn_mode.x - s(6.0) - (r.x + pad_x)).max(s(40.0)),
            (r.h - pad_y * 2).max(s(28.0)),
        );
        self.tb.set_bounds(tb_rect, &mut inv);
        self.btn_clear = Rect::new(tb_rect.right() - s(4.0) - bh, by, bh, bh);
    }

    /// 조건 바 자리(열 머리 DnD 놓임 판정).
    pub(crate) fn rect(&self) -> Rect {
        self.rect
    }

    /// 끌어 놓을 자리 강조 켬/끔 — 바뀌었으면 true(다시 그리기).
    pub(crate) fn set_drop_hot(&mut self, on: bool) -> bool {
        if self.drop_hot == on {
            return false;
        }
        self.drop_hot = on;
        true
    }

    /// 열 머리를 끌어 놓음 = 열 이름을 캐럿(선택) 자리에 넣는다(사용자 10-06 "컬럼을 조건 바에 DnD").
    /// `drop_template`(설정 `grid.cond_drop_template` · 기본 켬)이면 **구문 오류 예방** — 앞에 식이 있고 연결어(AND/OR/NOT/`(`)로
    /// 끝나지 않으면 ` AND `로 잇고, 열 타입에 맞는 기본 술어를 붙인다(숫자 = `열 = 0` · 그 밖 = `열 = ''`) · 캐럿 = 값 자리
    /// (숫자는 `0`을 선택해 바로 치면 바뀜 · 문자는 따옴표 사이). 끄면 열 이름만(낱말에 붙으면 공백 하나).
    pub(crate) fn insert_column(&mut self, name: &str, numeric: bool) {
        let mut inv = Invalidations::default();
        let chars: Vec<char> = self.tb.text().chars().collect();
        // 놓음은 늘 **글 끝에 잇는다**(협업 V1 bin27 C = 앞 놓기의 캐럿이 따옴표 사이라 둘째 열이 `' AND … '` 안에 들어갔다) —
        //   조건 식은 뒤에 붙는 것이 자연스럽고 캐럿 자리 삽입은 식을 깨뜨린다.
        let from = chars.len();
        let head: String = chars.iter().collect();
        let tail = String::new();
        let mut text = String::new();
        let head_t = head.trim_end();
        if self.drop_template && !head_t.is_empty() && !Self::ends_with_connector(head_t) {
            if !head.ends_with(char::is_whitespace) {
                text.push(' ');
            }
            text.push_str("AND ");
        } else if !head.is_empty() && !head.ends_with(|c: char| c.is_whitespace() || c == '(') {
            text.push(' ');
        }
        text.push_str(name);
        // 삽입 글 안에서 캐럿(선택) 위치.
        let mut sel: Option<(usize, usize)> = None;
        if self.drop_template {
            text.push_str(" = ");
            if numeric {
                let a = text.chars().count();
                text.push('0');
                sel = Some((a, a + 1));
            } else {
                text.push('\'');
                let a = text.chars().count();
                text.push('\'');
                sel = Some((a, a));
            }
        }
        if !tail.is_empty() && !tail.starts_with(char::is_whitespace) {
            text.push(' ');
        }
        // `TextBox::paste`는 포커스일 때만 받는다 — 놓임 = 상자로 포커스가 오는 동작이니 먼저 준다.
        self.tb.set_focused(true);
        // `paste`는 캐럿 자리에 넣는다 — 캐럿을 글 끝으로(선택 해제) 옮긴 뒤.
        self.tb.select_range(from, from, &mut inv);
        self.tb.paste(&text, &mut inv);
        if let Some((a, b)) = sel {
            self.tb.select_range(from + a, from + b, &mut inv);
        }
        self.tb.set_focused(true);
        self.err = None;
        self.menu.close();
        self.comp = None;
        let _ = self.tb.take_changed();
    }

    /// 앞 글이 연결어로 끝나는가(`AND`·`OR`·`NOT`·`(`) — 그러면 AND를 덧붙이지 않는다.
    fn ends_with_connector(head: &str) -> bool {
        if head.ends_with('(') {
            return true;
        }
        let last = head
            .rsplit(|c: char| c.is_whitespace() || c == '(')
            .next()
            .unwrap_or("");
        matches!(
            last.to_ascii_uppercase().as_str(),
            "AND" | "OR" | "NOT" | "WHERE" | "ON"
        )
    }

    /// 설정 `grid.cond_drop_template` — DnD 때 연결어·기본값을 붙일지.
    pub(crate) fn set_drop_template(&mut self, on: bool) {
        self.drop_template = on;
    }

    /// 완성 규칙 = 편집기 인텔리센스 설정 그대로(`intel.key_passthrough` · `intel.min_chars`).
    pub(crate) fn set_intel_cfg(&mut self, key_passthrough: bool, min_chars: usize) {
        self.key_passthrough = key_passthrough;
        self.min_chars = min_chars.max(1);
    }

    pub(crate) fn is_focused(&self) -> bool {
        self.tb.is_focused()
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.tb.set_focused(on);
        if !on {
            self.menu.close();
            self.comp = None;
        }
    }

    pub(crate) fn textbox_mut(&mut self) -> &mut TextBox {
        &mut self.tb
    }

    pub(crate) fn text(&self) -> String {
        self.tb.text()
    }

    pub(crate) fn set_text(&mut self, s: &str) {
        self.tb.set_text(s);
        self.menu.close();
        self.comp = None;
        self.err = None;
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open()
    }

    /// Enter로 확정된 조건(한 번).
    pub(crate) fn take_run(&mut self) -> Option<String> {
        self.run.take()
    }

    /// 지금 글로 실행 요청(Enter · 기동 명령 `grid.cond.run`) — 검증을 거친다 · 틀리면 오류만 남긴다.
    pub(crate) fn request_run(&mut self) {
        let text = self.tb.text();
        match validate(&text, &self.cols, self.dialect) {
            Ok(()) => {
                self.err = None;
                self.run = Some(text);
            }
            Err(e) => {
                self.err = Some(e);
                self.err_reported = false;
            }
        }
    }

    /// 서버가 거부한 조건(실행 뒤 DBMS 오류 · 사용자 10-06 "빨간 테두리 없이 토스트만") — 테두리는 켜되 상태줄 알림은 이미
    /// 실행 카드·토스트가 했으므로 다시 올리지 않는다(글이 바뀌면 지움).
    pub(crate) fn set_server_error(&mut self, msg: &str) {
        self.err = Some(msg.to_string());
        self.err_reported = true;
    }

    pub(crate) fn take_copy_req(&mut self) -> bool {
        std::mem::take(&mut self.copy_req)
    }

    /// 상자 우클릭 편집 메뉴에서 고른 동작(복사·잘라내기·붙여넣기 · 호스트가 클립보드로 잇는다 · 사용자 10-06).
    pub(crate) fn take_edit_ctx(&mut self) -> Option<nexa_ctl::EditCtxAction> {
        self.tb.take_edit_ctx()
    }

    /// 새 검증 오류를 **한 번** 알린다(호스트 상태줄) — 오류 자체는 남아 테두리·덤프에 보인다(글이 바뀌면 지움 · 협업 V1 c5/c6).
    pub(crate) fn take_error(&mut self) -> Option<String> {
        if self.err_reported {
            return None;
        }
        self.err_reported = true;
        self.err.clone()
    }

    /// 상자의 오버레이 스크롤바가 보이는 중(자동 숨김 페이드 → 호스트가 틱을 돌린다).
    pub(crate) fn bars_visible(&self) -> bool {
        self.tb.scrollbars_visible()
    }

    /// 애니메이션(복사 눌림 · 캐럿) 중인가 — 호스트가 다음 프레임을 예약.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        let a = self.tb.tick(now_ms);
        let b = self.copy.next_tick(Instant::now()).is_some();
        a || b
    }

    /// 상자 글이 상자 밖 길(IME 확정·붙여넣기)로 바뀐 뒤 — 완성 후보를 다시.
    pub(crate) fn query_changed(&mut self) {
        let _ = self.tb.take_changed();
        self.err = None;
        self.after_text_changed();
        self.refresh_completion();
    }

    /// 캐럿 앞 낱말(식별자 글자 · `.` 포함) 범위.
    fn word_at_caret(&self) -> (usize, usize) {
        let text: Vec<char> = self.tb.text().chars().collect();
        let caret = self.tb.caret().min(text.len());
        let mut start = caret;
        while start > 0
            && (text[start - 1].is_alphanumeric()
                || matches!(text[start - 1], '_' | '$' | '#' | '.'))
        {
            start -= 1;
        }
        (start, caret)
    }

    /// 완성 후보 = 접두로 시작하는 열(원래 순서) → 접두를 품은 열 → 연산 낱말 → 방언 함수(접두 2자부터 · `NAME(`). 접두가 비면 없음.
    fn candidates(&self, prefix: &str) -> Vec<String> {
        if prefix.is_empty() {
            return Vec::new();
        }
        let p = prefix.to_lowercase();
        let mut out: Vec<String> = Vec::new();
        for c in &self.cols {
            if c.to_lowercase().starts_with(&p) {
                out.push(c.clone());
            }
        }
        for c in &self.cols {
            if !out.contains(c) && c.to_lowercase().contains(&p) {
                out.push(c.clone());
            }
        }
        for k in KEYWORDS {
            if k.to_lowercase().starts_with(&p) {
                out.push((*k).to_string());
            }
        }
        if p.chars().count() >= 2 {
            for f in nsql_script::builtins::functions(Some(self.dialect)) {
                if f.name.to_lowercase().starts_with(&p) {
                    out.push(format!("{}(", f.name));
                }
            }
        }
        out.truncate(12);
        out
    }

    /// 완성 팝업 갱신(글이 바뀐 뒤) — 후보가 없으면 닫는다.
    fn refresh_completion(&mut self) {
        let (start, caret) = self.word_at_caret();
        let prefix: String = self
            .tb
            .text()
            .chars()
            .skip(start)
            .take(caret - start)
            .collect();
        // 편집기와 같은 기준(사용자 10-07): `intel.min_chars` 전엔 없음 · 숫자로 시작하는 낱말(`1=1`의 1)은 리터럴 = 없음.
        if prefix.chars().count() < self.min_chars
            || prefix.starts_with(|c: char| c.is_ascii_digit())
        {
            self.menu.close();
            self.comp = None;
            return;
        }
        let items = self.candidates(&prefix);
        if items.is_empty() || !self.tb.is_focused() {
            self.menu.close();
            self.comp = None;
            return;
        }
        self.comp = Some((start, caret));
        // ★ 팝업은 입력 글을 가리지 않는다(사용자 10-06): 조건 바 **아래**(그리드 쪽) · 아래에 자리가 없으면 조건 바 **위**.
        //   가로 = 캐럿 x(접두 시작 근처) · 호스트 = 창(그리드가 준다 · 모르면 조건 바 아래 400).
        let x = self.tb.caret_point().map_or(self.tb.bounds().x, |p| {
            p.x - (prefix.chars().count() as i32 * 7)
        });
        let host = if self.host.w > 0 && self.host.h > 0 {
            self.host
        } else {
            Rect::new(
                self.rect.x,
                self.rect.y,
                self.rect.w,
                (400.0 * self.scale) as i32,
            )
        };
        let est_h = items.len() as i32 * (24.0 * self.scale) as i32 + (8.0 * self.scale) as i32;
        let y = if self.rect.bottom() + est_h <= host.bottom() {
            self.rect.bottom()
        } else {
            (self.rect.y - est_h).max(host.y)
        };
        let entries: Vec<CtxItem> = items
            .iter()
            .map(|c| CtxItem::item(c.clone(), c.clone()))
            .collect();
        self.menu.set_scale(self.scale);
        self.menu.open_at(
            x.max(self.rect.x),
            y,
            entries,
            host,
            (160.0 * self.scale) as i32,
        );
        // 선택 없이 연다(편집기와 같음 — 고른 항목이 없으면 Enter/Tab은 통과 · ↓로 고른다).
    }

    /// 후보 넣기 — 완성 중 낱말을 바꾼다.
    fn accept(&mut self, pick: &str, inv: &mut Invalidations) {
        if let Some((a, b)) = self.comp.take() {
            self.tb.replace_range(a, b, pick, inv);
        }
        self.menu.close();
        let _ = self.tb.take_changed();
    }

    /// 사건 — 돌려주는 값 = 먹었는가. 상자 밖 MouseDown은 포커스를 거두고 **false**(그리드로 간다).
    pub(crate) fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> bool {
        if self.rect.h <= 0 {
            return false;
        }
        // 완성 팝업이 떠 있으면 이동·확정·닫기·마우스만 팝업이(글자·Backspace는 상자로 가서 다시 거른다) · 바깥 클릭은 닫고 통과.
        if self.menu.is_open() {
            if let InputEvent::Key {
                key: Key::Escape, ..
            } = ev
            {
                self.menu.close();
                self.comp = None;
                return true;
            }
            // ★ 키 통과(`intel.key_passthrough` · 편집기와 같은 규칙 · 사용자 10-07): 고른 항목이 없으면 Enter/Tab은 팝업만 닫고
            //   그 키는 상자로 그대로(Enter = 실행 · Tab = 글자) · 끄면 팝업이 삼킨다(두 번 입력).
            let accept_key = matches!(ev, InputEvent::Char { c: '\t', .. })
                || matches!(
                    ev,
                    InputEvent::Key {
                        key: Key::Enter,
                        ..
                    }
                );
            if accept_key && self.menu.hovered().is_none() {
                self.menu.close();
                self.comp = None;
                if !self.key_passthrough {
                    return true;
                }
            }
            // Tab(글자 '\t'로 온다 · input.rs 번역) = 선택 후보 넣기.
            if self.menu.is_open() && matches!(ev, InputEvent::Char { c: '\t', .. }) {
                let pick = self
                    .menu
                    .hovered()
                    .and_then(|i| self.menu.item_ids().get(i).map(|s| (*s).to_string()));
                if let Some(p) = pick {
                    self.accept(&p, inv);
                }
                return true;
            }
            let nav = self.menu.is_open()
                && matches!(
                    ev,
                    InputEvent::Key {
                        key: Key::Up | Key::Down | Key::Enter | Key::PageUp | Key::PageDown,
                        ..
                    } | InputEvent::MouseMove { .. }
                        | InputEvent::MouseDown { .. }
                        | InputEvent::MouseUp { .. }
                        | InputEvent::Wheel { .. }
                );
            if self.menu.is_outside_click(ev) {
                self.menu.close();
                self.comp = None;
            } else if nav && self.menu.on_event(ev) {
                if let Some(p) = self.menu.take_picked() {
                    self.accept(&p, inv);
                }
                return true;
            }
        }
        // 상자의 우클릭 편집 메뉴가 떠 있으면(상자 밖 아래에 그려진다) 마우스 사건은 전부 상자로(항목이 안 눌리던 결함 · 사용자 10-06).
        if self.tb.popup_open()
            && matches!(
                ev,
                InputEvent::MouseMove { .. }
                    | InputEvent::MouseDown { .. }
                    | InputEvent::MouseUp { .. }
                    | InputEvent::RightDown { .. }
            )
        {
            self.tb.on_event(ev, inv);
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                self.hover_in = self.rect.contains(p);
                self.copy.set_hover(self.copy.hit(p));
                if self.hover_in || self.tb.is_dragging() {
                    self.tb.on_event(ev, inv);
                }
                self.hover_in
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if std::env::var_os("NSQL_TRACE_COND").is_some() {
                    eprintln!(
                        "[cond] down p={},{} rect={:?} mode={:?} copy={:?} expanded={}",
                        x, y, self.rect, self.btn_mode, self.copy.rect, self.expanded
                    );
                }
                if self.copy.hit(p) {
                    self.copy.press(Instant::now());
                    self.copy_req = true;
                    return true;
                }
                if self.btn_mode.contains(p) {
                    self.toggle_expand();
                    return true;
                }
                if self.btn_clear.contains(p) && !self.tb.text().is_empty() {
                    self.clear_text();
                    return true;
                }
                if self.rect.contains(p) {
                    self.tb.on_event(ev, inv);
                    return true;
                }
                if self.tb.is_focused() {
                    self.set_focused(false);
                }
                false
            }
            InputEvent::MouseUp { x, y } => {
                let p = Point { x, y };
                if self.rect.contains(p) || self.tb.is_dragging() {
                    self.tb.on_event(ev, inv);
                    return true;
                }
                false
            }
            InputEvent::RightDown { x, y } => {
                let p = Point { x, y };
                if self.rect.contains(p) {
                    self.tb.on_event(ev, inv);
                    return true;
                }
                false
            }
            // 휠 = 바 안에 포인터가 있으면 상자가 스크롤(접힘 = 줄 이동 · 펼침 = 세로 · Shift/가로 휠 = 가로) · 그리드로 안 간다.
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } if self.hover_in => {
                self.tb.on_event(ev, inv);
                true
            }
            InputEvent::Key { .. }
            | InputEvent::Char { .. }
            | InputEvent::SelectAll
            | InputEvent::Undo
            | InputEvent::Redo
                if self.tb.is_focused() =>
            {
                if let InputEvent::Key {
                    key: Key::Escape, ..
                } = ev
                {
                    self.set_focused(false);
                    return true;
                }
                // Enter = 실행(Ctrl+Enter도) · **Shift+Enter** = 줄 바꿈(사용자 10-06 개념 변경 · 두 줄째가 되면 자동 펼침).
                if let InputEvent::Key {
                    key: Key::Enter,
                    shift,
                    ..
                } = *ev
                {
                    self.menu.close();
                    self.comp = None;
                    if shift {
                        self.tb.on_event(
                            &InputEvent::Key {
                                key: Key::Enter,
                                shift: false,
                                primary: false,
                            },
                            inv,
                        );
                        let _ = self.tb.take_changed();
                        self.err = None;
                        self.after_text_changed();
                    } else {
                        self.request_run();
                    }
                    return true;
                }
                self.tb.on_event(ev, inv);
                if self.tb.take_changed().is_some() {
                    self.err = None;
                    self.after_text_changed();
                    self.refresh_completion();
                }
                true
            }
            _ => false,
        }
    }

    /// 조건 바 그리기(그리드 위 한 줄) — 바탕 + 상자(오류 = 빨간 테두리) + 복사 버튼.
    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme, now: Instant) {
        if self.rect.h <= 0 {
            return;
        }
        dc.fill_rect(self.rect, th.chrome_bg);
        dc.fill_rect(
            Rect::new(self.rect.x, self.rect.bottom() - 1, self.rect.w, 1),
            th.border,
        );
        self.tb.paint(dc, th);
        if self.err.is_some() {
            dc.stroke_round_rect(self.tb.bounds(), (4.0 * self.scale) as i32, th.danger, 1.5);
        }
        // 열 머리를 끌어 위에 있는 동안 = 놓을 자리 강조(강조색 테두리 + 옅은 채움).
        if self.drop_hot {
            let r = self.tb.bounds();
            let rad = (4.0 * self.scale) as i32;
            dc.fill_round_rect_alpha(r, rad, th.accent, 0.10);
            dc.stroke_round_rect(r, rad, th.accent, 1.5);
        }
        self.copy.paint(dc, th, 1.0, self.scale, now);
        // 보기 버튼 = ▾(펼치기 · 최대 줄까지) / ▴(접기 · 한 줄 보기) · 복사 버튼과 같은 상자 꼴.
        let b = self.btn_mode;
        dc.fill_round_rect_alpha(b, (3.0 * self.scale) as i32, th.text_dim, 0.12);
        let g = (b.w / 2).max(4);
        let (cx, cy) = (b.x + b.w / 2, b.y + b.h / 2);
        if self.expanded {
            dc.fill_triangle(
                (cx - g / 2, cy + g / 4),
                (cx + g / 2, cy + g / 4),
                (cx, cy - g / 4),
                th.text,
            );
        } else {
            dc.fill_triangle(
                (cx - g / 2, cy - g / 4),
                (cx + g / 2, cy - g / 4),
                (cx, cy + g / 4),
                th.text,
            );
        }
        // × 전체 지우기 — 글이 있을 때만(원 배경 없이 두 획 · 상자 오른쪽 위).
        if !self.tb.text().is_empty() {
            let r = self.btn_clear;
            let m = (5.0 * self.scale).round() as i32;
            let (x0, y0, x1, y1) = (r.x + m, r.y + m, r.right() - m, r.bottom() - m);
            dc.polyline(&[(x0, y0), (x1, y1)], th.text_dim, 1.5);
            dc.polyline(&[(x0, y1), (x1, y0)], th.text_dim, 1.5);
        }
    }

    /// 팝업 층(상자 편집 메뉴 · 완성 팝업).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        self.tb.paint_popup(dc, th);
        self.menu.paint(dc, th);
    }

    /// 자체 시험 덤프: `text=… focused=… menu=[a|b|…] err=…`.
    pub(crate) fn dump(&self) -> String {
        let items: Vec<String> = if self.menu.is_open() {
            self.menu
                .item_ids()
                .into_iter()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let m = self.btn_mode;
        let c = self.copy.rect;
        format!(
            "text={} focused={} menu=[{}] err={} expanded={} lines={} h={} mode={},{},{},{} copy={},{},{},{}",
            self.tb.text(),
            self.tb.is_focused(),
            items.join("|"),
            self.err.as_deref().unwrap_or(""),
            self.expanded,
            self.line_count(),
            self.wanted_height(self.scale),
            m.x,
            m.y,
            m.w,
            m.h,
            c.x,
            c.y,
            c.w,
            c.h
        )
    }
}

/// 출처 문장을 조건으로 감싼다(순수 · 조건이 비면 출처 그대로) — `SELECT * FROM ( <출처> ) q WHERE 1=1 AND (<조건>)`.
pub(crate) fn wrap_condition(source: &str, cond: &str) -> String {
    let src = source.trim().trim_end_matches(';').trim();
    let c = cond.trim().trim_end_matches(';').trim();
    if c.is_empty() {
        return src.to_string();
    }
    format!("SELECT *\nFROM (\n{src}\n) q\nWHERE 1=1\nAND ({c})")
}

/// 식 토큰(검증용 · 순수).
#[derive(Debug, PartialEq, Eq)]
enum Tok {
    /// 식별자(따옴표 없음 · `.` 포함) · 뒤에 `(`가 오는지.
    Ident(String, bool),
    /// `"…"` `[…]` `` `…` `` 인용 식별자(안의 글).
    QIdent(String),
    Other,
}

/// 식을 토큰으로(문자열 리터럴·숫자·연산자는 `Other`) — 괄호·따옴표 짝과 `;`·`::`도 여기서 판정해 오류로.
fn tokenize(cond: &str, dialect: Dialect) -> Result<Vec<Tok>, String> {
    let cs: Vec<char> = cond.chars().collect();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut out = Vec::new();
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '\'' => {
                // 문자열: `''` = 따옴표 하나.
                let mut j = i + 1;
                loop {
                    if j >= cs.len() {
                        return Err(t(Msg::CondErrQuote).to_string());
                    }
                    if cs[j] == '\'' {
                        if j + 1 < cs.len() && cs[j + 1] == '\'' {
                            j += 2;
                            continue;
                        }
                        break;
                    }
                    j += 1;
                }
                out.push(Tok::Other);
                i = j + 1;
            }
            '"' | '[' | '`' => {
                let close = match c {
                    '[' => ']',
                    other => other,
                };
                let mut j = i + 1;
                while j < cs.len() && cs[j] != close {
                    j += 1;
                }
                if j >= cs.len() {
                    return Err(t(Msg::CondErrQuote).to_string());
                }
                out.push(Tok::QIdent(cs[i + 1..j].iter().collect()));
                i = j + 1;
            }
            '(' => {
                depth += 1;
                out.push(Tok::Other);
                i += 1;
            }
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err(t(Msg::CondErrParen).to_string());
                }
                out.push(Tok::Other);
                i += 1;
            }
            ';' => return Err(t(Msg::CondErrStatement).to_string()),
            ':' if i + 1 < cs.len() && cs[i + 1] == ':' => {
                if dialect != Dialect::Postgres {
                    return Err(t(Msg::CondErrPgCast).to_string());
                }
                out.push(Tok::Other);
                i += 2;
                // 뒤의 형 이름(`::text` `::numeric(10,2)`)은 열이 아니다 — 식별자 글자를 통째로 건너뛴다.
                while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                    i += 1;
                }
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut j = i;
                while j < cs.len()
                    && (cs[j].is_alphanumeric() || matches!(cs[j], '_' | '$' | '#' | '.'))
                {
                    j += 1;
                }
                let word: String = cs[i..j].iter().collect();
                let mut k = j;
                while k < cs.len() && cs[k].is_whitespace() {
                    k += 1;
                }
                let call = k < cs.len() && cs[k] == '(';
                out.push(Tok::Ident(word, call));
                i = j;
            }
            _ => {
                out.push(Tok::Other);
                i += 1;
            }
        }
    }
    if depth != 0 {
        return Err(t(Msg::CondErrParen).to_string());
    }
    Ok(out)
}

/// 조건식 검증(순수 · 방언 기준) — 비면 OK(= 조건 없음). 오류 = 사용자에게 보일 한 줄.
pub(crate) fn validate(cond: &str, cols: &[String], dialect: Dialect) -> Result<(), String> {
    let c = cond.trim();
    if c.is_empty() {
        return Ok(());
    }
    let toks = tokenize(c, dialect)?;
    let is_col = |name: &str| -> bool {
        // `q.열`(감싼 서브쿼리 별칭)도 허용 · 그 밖의 한정자(`A.열`)는 바깥 WHERE에서 안 보인다.
        let bare = name
            .strip_prefix("q.")
            .or_else(|| name.strip_prefix("Q."))
            .unwrap_or(name);
        !bare.contains('.') && cols.iter().any(|col| col.eq_ignore_ascii_case(bare))
    };
    let funcs = nsql_script::builtins::functions(Some(dialect));
    let is_func = |name: &str| funcs.iter().any(|f| f.name.eq_ignore_ascii_case(name));
    for tk in &toks {
        match tk {
            Tok::Ident(w, call) => {
                let up = w.to_ascii_uppercase();
                if STATEMENT_WORDS.contains(&up.as_str()) {
                    return Err(t(Msg::CondErrStatement).to_string());
                }
                if ALLOWED_WORDS.contains(&up.as_str()) || is_col(w) {
                    continue;
                }
                if *call {
                    if is_func(w) {
                        continue;
                    }
                    return Err(tf(Msg::CondErrFunc, &[w]));
                }
                return Err(tf(Msg::CondErrIdent, &[w]));
            }
            Tok::QIdent(w) => {
                if !is_col(w) {
                    return Err(tf(Msg::CondErrIdent, &[w]));
                }
            }
            Tok::Other => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_rules() {
        assert_eq!(wrap_condition("SELECT * FROM t;", ""), "SELECT * FROM t");
        assert_eq!(
            wrap_condition("SELECT * FROM t;", "a = 1 OR b LIKE 'x%';"),
            "SELECT *\nFROM (\nSELECT * FROM t\n) q\nWHERE 1=1\nAND (a = 1 OR b LIKE 'x%')"
        );
    }

    /// 완성 후보 = 접두 시작 열 → 품은 열 → 낱말 → 방언 함수(접두 2자부터 · `NAME(`) · 접두 없으면 없음 · 상한 12.
    #[test]
    fn candidates_order() {
        let mut c = CondBar::new();
        c.set_columns(vec![
            "ITEM_CD".into(),
            "ITEM_NM".into(),
            "PROJECT_CD".into(),
        ]);
        assert_eq!(c.candidates("it"), vec!["ITEM_CD", "ITEM_NM"]);
        assert_eq!(c.candidates("_cd"), vec!["ITEM_CD", "PROJECT_CD"]);
        assert_eq!(
            c.candidates("i"),
            vec!["ITEM_CD", "ITEM_NM", "IN", "IS NULL", "IS NOT NULL"]
        );
        assert!(c.candidates("").is_empty());
        c.set_dialect(Dialect::Oracle);
        assert!(
            c.candidates("nv").iter().any(|s| s == "NVL("),
            "{:?}",
            c.candidates("nv")
        );
    }

    /// 검증(방언 기준): 열·낱말·함수·`q.열`은 통과 · 모르는 열/함수 · 괄호·따옴표 짝 · `;` · 문장 낱말 · PG 밖 `::`는 오류.
    #[test]
    fn validate_rules() {
        let cols: Vec<String> = vec!["ITEM_CD".into(), "ITEM_NM".into()];
        let ok = |s: &str, d: Dialect| validate(s, &cols, d).is_ok();
        assert!(ok("", Dialect::Oracle));
        assert!(ok(
            "ITEM_CD LIKE 'MH%' OR item_nm = 'a''b'",
            Dialect::Oracle
        ));
        assert!(ok(
            "q.ITEM_CD IN ('A', 'B') AND ITEM_NM IS NOT NULL",
            Dialect::Postgres
        ));
        assert!(ok(
            "UPPER(ITEM_NM) = 'X' AND NVL(ITEM_CD, 'z') <> 'z'",
            Dialect::Oracle
        ));
        assert!(ok("ITEM_CD::text = '1'", Dialect::Postgres));
        assert!(ok("\"ITEM_CD\" = 1", Dialect::Postgres));
        assert!(!ok("ITEM_CD::text = '1'", Dialect::Oracle), "PG 전용 ::");
        assert!(!ok("NO_SUCH = 1", Dialect::Oracle));
        assert!(!ok("A.ITEM_CD = 1", Dialect::Oracle), "다른 한정자");
        assert!(!ok("FOO(ITEM_CD) = 1", Dialect::Oracle), "모르는 함수");
        assert!(!ok("ITEM_CD = 'x", Dialect::Oracle), "따옴표 짝");
        assert!(!ok("(ITEM_CD = 1", Dialect::Oracle), "괄호 짝");
        assert!(!ok("ITEM_CD = 1; DROP TABLE t", Dialect::Oracle), ";");
        assert!(!ok("SELECT 1", Dialect::Oracle), "문장");
    }

    /// 개념 변경(사용자 10-06): Enter = 실행(한 줄·여러 줄 모두) · Shift+Enter = 줄 바꿈 → 두 줄째가 되면 자동 펼침(3줄 높이) ·
    /// ▴ 접기 = 한 줄 보기 유지(줄 바꿈이 늘어도) · ▾ = 다시 펼침 · 최대 줄 수 설정 · 비우면 접힘·해제.
    #[test]
    fn enter_runs_and_shift_enter_newline_expands() {
        let mut c = CondBar::new();
        c.set_columns(vec!["ITEM_CD".into()]);
        c.set_rect(Rect::new(0, 0, 400, 26), 1.0);
        c.set_focused(true);
        c.set_text("ITEM_CD = 1");
        assert_eq!(c.wanted_height(1.0), 32);
        let mut inv = Invalidations::default();
        let enter = |shift: bool, primary: bool| InputEvent::Key {
            key: Key::Enter,
            shift,
            primary,
        };
        c.on_event(&enter(false, false), &mut inv);
        assert!(c.take_run().is_some(), "Enter = 실행");
        assert!(!c.is_expanded());
        c.on_event(&enter(true, false), &mut inv);
        assert!(c.take_run().is_none(), "Shift+Enter = 줄 바꿈");
        assert_eq!(c.text().matches('\n').count(), 1, "{}", c.dump());
        assert!(c.is_expanded(), "{}", c.dump());
        assert_eq!(c.wanted_height(1.0), 20 * 3 + 12 + 4);
        c.on_event(&enter(false, true), &mut inv);
        assert!(c.take_run().is_some(), "Ctrl+Enter도 실행");
        // 접기 = 한 줄 보기 유지.
        c.toggle_expand();
        assert!(!c.is_expanded() && c.user_collapsed);
        c.on_event(&enter(true, false), &mut inv);
        assert!(
            !c.is_expanded(),
            "사용자가 접었으면 줄 바꿈이 늘어도 한 줄 보기"
        );
        c.toggle_expand();
        assert!(c.is_expanded() && !c.user_collapsed);
        c.set_max_lines(5);
        assert_eq!(c.wanted_height(1.0), 20 * 5 + 12 + 4);
        c.clear_text();
        assert!(c.text().is_empty() && !c.is_expanded() && !c.user_collapsed);
    }

    /// 버튼 자리 = [×][▾/▴][복사] 오른쪽 위 · ▾ 클릭(MouseDown) = 펼침 · × 클릭 = 전체 지우기(글이 있을 때만).
    #[test]
    fn mode_and_clear_button_click() {
        let mut c = CondBar::new();
        c.set_rect(Rect::new(0, 0, 400, 26), 1.0);
        let m = c.btn_mode;
        assert!(
            m.w > 0 && m.right() < c.copy.rect.x && c.btn_clear.right() < m.x,
            "{m:?} {:?}",
            c.copy.rect
        );
        assert!(
            c.tb.bounds().contains(Point {
                x: c.btn_clear.x + 1,
                y: c.btn_clear.y + 1
            }),
            "× = 상자 안 오른쪽 위"
        );
        let mut inv = Invalidations::default();
        let down = |r: Rect| InputEvent::MouseDown {
            x: r.x + r.w / 2,
            y: r.y + r.h / 2,
            shift: false,
            primary: false,
        };
        let hit = c.on_event(&down(m), &mut inv);
        assert!(hit && c.is_expanded(), "{}", c.dump());
        c.set_text("a = 1");
        assert!(c.on_event(&down(c.btn_clear), &mut inv));
        assert!(c.text().is_empty() && !c.is_expanded());
        let d = c.dump();
        assert!(d.contains("mode=") && d.contains("copy="));
    }

    /// 열 머리 DnD(사용자 10-06): 빈 상자 = `열 = ''`(캐럿 따옴표 사이) · 앞에 식 = ` AND ` 연결 · 숫자 = `= 0` 선택 ·
    /// 연결어 뒤(`AND `)엔 AND를 덧붙이지 않음 · 설정 끔 = 이름만(낱말에 붙으면 공백).
    #[test]
    fn insert_column_template_and_connector() {
        let mut c = CondBar::new();
        c.set_rect(Rect::new(0, 0, 400, 26), 1.0);
        c.insert_column("NAME", false);
        assert_eq!(c.text(), "NAME = ''");
        assert_eq!(c.tb.caret(), 8, "캐럿 = 따옴표 사이");
        assert!(c.is_focused());
        // 캐럿이 따옴표 사이에 있어도 숫자 열 놓기는 **글 끝**에 AND로 잇고 0을 선택(협업 V1 bin27 C).
        let mut inv = Invalidations::default();
        c.insert_column("QTY", true);
        assert_eq!(c.text(), "NAME = '' AND QTY = 0");
        assert_eq!(c.tb.selection(), Some((20, 21)), "{}", c.text());
        // 연결어로 끝나면 AND를 덧붙이지 않는다.
        c.set_text("a = 1 AND ");
        let n = c.text().chars().count();
        c.tb.select_range(n, n, &mut inv);
        c.insert_column("B", true);
        assert_eq!(c.text(), "a = 1 AND B = 0");
        c.set_text("NOT (");
        let n = c.text().chars().count();
        c.tb.select_range(n, n, &mut inv);
        c.insert_column("B", false);
        assert_eq!(c.text(), "NOT (B = ''");
        // 설정 끔 = 이름만.
        c.set_drop_template(false);
        c.set_text("x =");
        let n = c.text().chars().count();
        c.tb.select_range(n, n, &mut inv);
        c.insert_column("COL", false);
        assert_eq!(c.text(), "x = COL");
    }

    /// 글자를 치면 완성 팝업(글자는 상자로 계속) · Tab = 넣기 · Enter = 검증 뒤 실행 요청 · 틀린 식 = 오류만 · Esc = 포커스 거둠.
    #[test]
    fn typing_completion_and_enter_runs() {
        let mut c = CondBar::new();
        c.set_columns(vec!["ITEM_CD".into(), "PROJECT_CD".into()]);
        c.set_dialect(Dialect::Oracle);
        c.set_rect(Rect::new(0, 0, 400, 26), 1.0);
        c.set_focused(true);
        let mut inv = Invalidations::default();
        for ch in "ite".chars() {
            assert!(c.on_event(&InputEvent::Char { c: ch, now_ms: 0 }, &mut inv));
        }
        assert_eq!(c.text(), "ite");
        assert!(c.menu_open(), "{}", c.dump());
        // 편집기와 같은 기준: 열릴 때 고른 항목 없음 → ↓로 고르고 Tab.
        let down = InputEvent::Key {
            key: Key::Down,
            shift: false,
            primary: false,
        };
        assert!(c.on_event(&down, &mut inv));
        assert!(c.on_event(&InputEvent::Char { c: '\t', now_ms: 0 }, &mut inv));
        assert_eq!(c.text(), "ITEM_CD");
        assert!(!c.menu_open());
        for ch in " = 1".chars() {
            c.on_event(&InputEvent::Char { c: ch, now_ms: 0 }, &mut inv);
        }
        let enter = InputEvent::Key {
            key: Key::Enter,
            shift: false,
            primary: false,
        };
        c.on_event(&enter, &mut inv);
        assert_eq!(c.take_run().as_deref(), Some("ITEM_CD = 1"));
        assert!(c.take_error().is_none());
        c.set_text("NO_SUCH = 1");
        c.set_focused(true);
        c.on_event(&enter, &mut inv);
        assert!(c.take_run().is_none(), "틀린 식은 실행 안 함");
        assert!(c.take_error().is_some());
        c.on_event(
            &InputEvent::Key {
                key: Key::Escape,
                shift: false,
                primary: false,
            },
            &mut inv,
        );
        assert!(!c.is_focused());
    }

    /// 편집기와 같은 완성 기준(사용자 10-07): 숫자 접두(`1`)엔 팝업 없음 · `min_chars` 전엔 없음 · 고른 항목 없이 Enter =
    /// 통과(팝업 닫고 실행 → 틀린 식이면 오류) · `key_passthrough` 끔 = 팝업만 닫힘(실행 없음).
    #[test]
    fn completion_follows_editor_rules() {
        let mut c = CondBar::new();
        c.set_columns(vec!["ITEM_CD".into(), "ITEM_NM".into()]);
        c.set_dialect(Dialect::Oracle);
        c.set_rect(Rect::new(0, 0, 400, 32), 1.0);
        c.set_focused(true);
        let mut inv = Invalidations::default();
        let ch = |ch: char| InputEvent::Char { c: ch, now_ms: 0 };
        c.on_event(&ch('1'), &mut inv);
        assert!(!c.menu_open(), "숫자 접두 = 팝업 없음 {}", c.dump());
        c.set_text("");
        c.on_event(&ch('i'), &mut inv);
        assert!(!c.menu_open(), "min_chars 2 전 = 없음");
        c.on_event(&ch('t'), &mut inv);
        assert!(c.menu_open(), "{}", c.dump());
        let enter = InputEvent::Key {
            key: Key::Enter,
            shift: false,
            primary: false,
        };
        c.on_event(&enter, &mut inv);
        assert!(!c.menu_open());
        assert!(
            c.take_run().is_none() && c.take_error().is_some(),
            "통과 = 실행 시도(틀린 식 = 오류)"
        );
        // 끄면 팝업만 닫힌다.
        c.set_intel_cfg(false, 1);
        c.set_text("");
        c.on_event(&ch('i'), &mut inv);
        assert!(c.menu_open(), "min_chars 1");
        c.on_event(&enter, &mut inv);
        assert!(!c.menu_open());
        assert!(
            c.take_run().is_none() && c.take_error().is_none(),
            "삼킴 = 실행 없음"
        );
    }
}
