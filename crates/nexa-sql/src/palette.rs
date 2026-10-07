//! 명령 팔레트(사용자 09-14 · Sublime `Ctrl+⇧P`) — 입력 한 줄 + 퍼지 필터 목록.
//!
//! 명령 = 메뉴 항목 전부(`File: New` …) + `Set Syntax: <이름>`(레지스트리) — 호스트가 [`Palette::set_commands`]로 준다.
//! 키: ↑↓ 이동 · Enter 실행 · Esc 닫기 · 바깥 클릭 닫기 · 행 클릭 실행.

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};

pub(crate) enum PaletteAction {
    None,
    Pick(String),
    /// 프롬프트 모드의 입력 확정(`open_prompt`의 id · 입력 글자).
    Prompt {
        id: String,
        text: String,
    },
    Close,
}

pub(crate) struct Palette {
    open: bool,
    input: TextBox,
    /// 자체 목록(명령 팔레트 · Goto의 생성 모드 `%`/`?`/`#`) — (id, 표시 라벨) + 소문자 캐시.
    cmds: Src,
    /// 지금 거르는 원천(Goto 모드는 접두로 바뀐다 · **복제 없이** 참조만 · 사용자 10-07 "Ctrl+P 멈춤").
    active: Active,
    /// 필터 결과 — cmds index (점수순).
    matches: Vec<usize>,
    sel: usize,
    /// 보이는 첫 행(전체 결과 기준) — 결과가 `MAX_ROWS`보다 많으면 ↑/↓·휠로 굴린다(사용자 09-19 마우스 보완).
    top: usize,
    /// 마지막으로 본 마우스 위치 — 같은 자리의 MouseMove(키보드 이동 직후 재발행)로 선택이 되돌아가지 않게.
    last_mouse: (i32, i32),
    bounds: Rect,
    row_h: i32,
    scale: f32,
    last_query: String,
    /// `:` 로 시작하는 질의 = 줄 이동 모드(Goto Anything · T-96) — 숫자가 있으면 그 줄.
    goto: Option<Option<usize>>,
    /// 프롬프트 모드(탭 이름 바꾸기 등 · 09-17): 목록 없이 글자 입력만 · Enter = [`PaletteAction::Prompt`].
    prompt: Option<String>,
    /// 프롬프트 모드의 안내 글(입력란 아래 한 줄 — 입력란에 이미 글이 있어 자리표시자가 보이지 않는다).
    hint: String,
    /// 프롬프트가 **붙을 대상**(이름을 바꾸는 탭의 rect · 사용자 09-21 "요청한 탭 근처에 · 수정 중인 대상을 식별할 수 있게").
    /// 있으면 상자가 그 바로 아래(자리가 없으면 위)에 붙고 대상에 강조 테두리를 그린다 · 없으면 종전처럼 창 위 가운데.
    anchor: Option<Rect>,
    /// 창 크기(물리 px) — 붙는 상자를 창 안에 두는 데 쓴다.
    win: (i32, i32),
    /// ★ Goto Anything 원천(사용자 10-07 · VS Code식 접두): 기본 = 파일 · `>` = 명령 · `@` = 현재 파일 심볼(`@:` = 종류별) ·
    /// `%` = 현재 파일 줄 · `?` = 도움말 · `#` = 프로젝트 심볼(다음 단계 · 안내만). `set_commands`로 연 단순 팔레트는 `goto_mode` = 거짓.
    goto_mode: bool,
    src_files: Src,
    src_cmds: Src,
    src_syms: Src,
    doc_lines: Vec<String>,
    help: Vec<(String, String)>,
    /// 파일 모드 `이름:줄`의 줄 — Pick id에 `#L<n>` 꼬리로 붙인다.
    tail_line: Option<usize>,
    /// 접두 모드의 안내(결과가 없을 때 한 줄 · `#` 등).
    mode_hint: String,
    /// 파일 목록 안내(호스트가 준다 · 상한·열거 중) · 상위 K에서 잘린 수(결과 끝 흐린 줄 "…외 N개").
    files_note: String,
    more_hidden: usize,
}

/// 원천 한 벌 — (id, 라벨) + **소문자 라벨 캐시**(키 입력마다 `to_lowercase` 할당을 안 한다 · 10k 파일에서 멈춤의 뿌리).
/// `with_id`면 라벨 뒤에 id도 붙여 두어 영어 낱말(`save`)이 한국어 라벨의 명령(`file.save`)에 맞는다(Goto `>`).
#[derive(Default, Clone, Debug)]
pub(crate) struct Src {
    items: Vec<(String, String)>,
    lower: Vec<String>,
}

impl Src {
    fn from_items(items: Vec<(String, String)>, with_id: bool) -> Self {
        let mut s = Src::default();
        s.extend(items, with_id);
        s
    }
    fn extend(&mut self, items: Vec<(String, String)>, with_id: bool) {
        self.lower.reserve(items.len());
        for (id, label) in &items {
            let mut l = label.to_lowercase();
            if with_id {
                l.push(' ');
                l.push_str(&id.to_lowercase());
            }
            self.lower.push(l);
        }
        self.items.extend(items);
    }
    fn len(&self) -> usize {
        self.items.len()
    }
}

/// 지금 거르는 원천.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Active {
    Own,
    Files,
    Cmds,
    Syms,
}

/// 퍼지 결과 상한 — 화면 12행에 넉넉히 · 그 이상은 부분 정렬(`select_nth`)로 끊는다(10k 전부 정렬 안 함).
const TOP_K: usize = 400;

/// 접두 풀이(순수 · 시험): 어느 원천을 어떤 질의로 볼지.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GotoMode {
    Files,
    Commands,
    Symbols,
    SymbolsByKind,
    WorkspaceSymbols,
    QuickSearch,
    Help,
    /// `:줄`(종전 줄 이동).
    Line,
}

/// `q` → (모드 · 남은 질의 · `이름:줄`의 줄).
pub(crate) fn parse_goto(q: &str) -> (GotoMode, String, Option<usize>) {
    let t = q.trim_start();
    if let Some(r) = t.strip_prefix(':') {
        return (GotoMode::Line, r.trim().to_string(), None);
    }
    if let Some(r) = t.strip_prefix('>') {
        return (GotoMode::Commands, r.trim().to_string(), None);
    }
    if let Some(r) = t.strip_prefix("@:") {
        return (GotoMode::SymbolsByKind, r.trim().to_string(), None);
    }
    if let Some(r) = t.strip_prefix('@') {
        return (GotoMode::Symbols, r.trim().to_string(), None);
    }
    if let Some(r) = t.strip_prefix('#') {
        return (GotoMode::WorkspaceSymbols, r.trim().to_string(), None);
    }
    if let Some(r) = t.strip_prefix('%') {
        return (GotoMode::QuickSearch, r.to_string(), None);
    }
    if t.starts_with('?') {
        return (GotoMode::Help, String::new(), None);
    }
    // `이름:줄` — 마지막 `:` 뒤가 숫자면 줄(이름이 비면 `:줄` 모드와 같다 = 위에서 처리됨).
    if let Some((name, n)) = t.rsplit_once(':') {
        if let Ok(n) = n.trim().parse::<usize>() {
            if n > 0 && !name.trim().is_empty() {
                return (GotoMode::Files, name.trim().to_string(), Some(n));
            }
        }
    }
    (GotoMode::Files, t.trim().to_string(), None)
}

/// 파일 모드 순위 가산 — 열린 탭 > 최근 > 프로젝트 파일(퍼지 점수에 더한다).
fn tier_bonus(id: &str) -> i32 {
    if id.starts_with("tab:") {
        3000
    } else if id.starts_with("file.recent:") {
        2000
    } else {
        0
    }
}

const MAX_ROWS: usize = 12;

impl Palette {
    pub(crate) fn new() -> Self {
        Palette {
            open: false,
            input: TextBox::new(t(Msg::PhPalette)).with_clearable(),
            cmds: Src::default(),
            active: Active::Own,
            matches: Vec::new(),
            sel: 0,
            top: 0,
            last_mouse: (i32::MIN, i32::MIN),
            bounds: Rect::new(0, 0, 0, 0),
            row_h: 26,
            scale: 1.0,
            last_query: String::new(),
            goto: None,
            prompt: None,
            hint: String::new(),
            anchor: None,
            win: (0, 0),
            goto_mode: false,
            src_files: Src::default(),
            src_cmds: Src::default(),
            src_syms: Src::default(),
            doc_lines: Vec::new(),
            help: Vec::new(),
            tail_line: None,
            mode_hint: String::new(),
            files_note: String::new(),
            more_hidden: 0,
        }
    }

    /// 파일 목록 안내 글(빈 글 = 없음) — 파일 모드 결과 끝에 흐리게.
    pub(crate) fn set_files_note(&mut self, note: &str) {
        self.files_note = note.to_string();
    }

    /// ★ Goto Anything로 열기(사용자 10-07) — 원천 전부를 받고 접두 모드를 켠다.
    pub(crate) fn open_goto(
        &mut self,
        files: Vec<(String, String)>,
        cmds: Vec<(String, String)>,
        syms: Vec<(String, String)>,
        doc_lines: Vec<String>,
        help: Vec<(String, String)>,
        prefill: &str,
    ) {
        self.src_files = Src::from_items(files, false);
        self.src_cmds = Src::from_items(cmds, true);
        self.src_syms = Src::from_items(syms, false);
        self.doc_lines = doc_lines;
        self.help = help;
        self.goto_mode = true;
        self.open_with(prefill, t(Msg::PhGoto));
    }

    /// 자체 시험 덤프: `mode=goto|cmd query=… rows=N sel=i` + 보이는 행 라벨.
    pub(crate) fn dump(&self) -> String {
        let mut out = format!(
            "mode={} query={} rows={} sel={} top={}",
            if self.goto_mode { "goto" } else { "cmd" },
            self.input.text(),
            self.matches.len(),
            self.sel,
            self.top
        );
        if let Some(g) = self.goto {
            out.push_str(&format!(" goto_line={g:?}"));
        }
        if let Some(n) = self.tail_line {
            out.push_str(&format!(" tail_line={n}"));
        }
        if !self.mode_hint.is_empty() {
            out.push_str(&format!(" hint={}", self.mode_hint));
        }
        if !self.files_note.is_empty() {
            out.push_str(&format!(" note={}", self.files_note));
        }
        if self.more_hidden > 0 {
            out.push_str(&format!(" more={}", self.more_hidden));
        }
        for &i in self.matches.iter().skip(self.top).take(MAX_ROWS) {
            out.push('\n');
            out.push_str(&self.items().items[i].0);
            out.push('\t');
            out.push_str(&self.items().items[i].1);
        }
        out
    }

    /// 파일 원천 교체(Goto 모드일 때만).
    pub(crate) fn set_files(&mut self, files: Vec<(String, String)>) {
        if self.goto_mode {
            self.src_files = Src::from_items(files, false);
            self.refilter(true);
        }
    }

    /// 파일 원천 **덧붙이기**(프로젝트 폴더 열거가 도착할 때 · 새 파일만 · 재생성 0) — 파일 모드면 다시 거른다.
    pub(crate) fn append_files(&mut self, files: Vec<(String, String)>) {
        if !self.goto_mode || files.is_empty() {
            return;
        }
        self.src_files.extend(files, false);
        if self.active == Active::Files {
            self.refilter(true);
        }
    }

    pub(crate) fn goto_mode(&self) -> bool {
        self.open && self.goto_mode
    }

    /// 입력란 글을 바꾼다(도움말 항목 선택 = 접두 넣기 · 캐럿 끝).
    pub(crate) fn set_query(&mut self, q: &str) {
        let mut inv = Invalidations::default();
        self.input.set_text(q);
        let n = q.chars().count();
        self.input.select_range(n, n, &mut inv);
        self.refilter(true);
    }

    /// 프롬프트 모드로 열기 — `id`는 확정 때 그대로 돌려준다 · `placeholder` 안내 · `initial` 초기 글자(전체 선택).
    /// [`Self::open_prompt`] + **대상 옆에 붙이기** — `anchor` = 이름을 바꾸는 탭의 rect.
    pub(crate) fn open_prompt_at(
        &mut self,
        id: &str,
        placeholder: &str,
        initial: &str,
        anchor: Option<Rect>,
    ) {
        self.anchor = anchor;
        self.open_prompt(id, placeholder, initial);
    }

    /// 창 크기(붙는 상자의 경계).
    pub(crate) fn set_window(&mut self, w: i32, h: i32) {
        self.win = (w, h);
    }

    pub(crate) fn open_prompt(&mut self, id: &str, placeholder: &str, initial: &str) {
        self.open = true;
        self.prompt = Some(id.to_string());
        self.hint = placeholder.to_string();
        self.input = TextBox::new(placeholder).with_text(initial);
        self.input.set_scale(self.scale);
        self.input.set_focused(true);
        let mut inv = Invalidations::default();
        self.input.on_event(&InputEvent::SelectAll, &mut inv);
        self.layout(&mut inv);
        self.matches.clear();
        self.sel = 0;
    }

    pub(crate) fn set_commands(&mut self, cmds: Vec<(String, String)>) {
        self.goto_mode = false;
        self.cmds = Src::from_items(cmds, false);
        self.active = Active::Own;
        self.refilter(true);
    }

    /// 지금 원천.
    fn items(&self) -> &Src {
        match self.active {
            Active::Own => &self.cmds,
            Active::Files => &self.src_files,
            Active::Cmds => &self.src_cmds,
            Active::Syms => &self.src_syms,
        }
    }

    /// IME 조합 중 글자(preedit)를 입력 상자에 보인다 — 팔레트는 `Focus` 변형이 아니라 호스트가 IME 사건을 여기로 넘긴다(09-27 한글 버그).
    pub(crate) fn set_preedit(&mut self, text: &str, inv: &mut Invalidations) {
        self.input.set_preedit(text, inv);
    }

    /// 지금 검색어(시험).
    #[cfg(test)]
    pub(crate) fn query(&self) -> String {
        self.input.text()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn open(&mut self, prefill: &str) {
        self.open_with(prefill, t(Msg::PhPalette));
    }

    /// 열기(자리 표시 글 지정 · Goto 모드는 접두 안내).
    fn open_with(&mut self, prefill: &str, placeholder: &str) {
        self.open = true;
        // × 지우기 = 검색 입력란 공통(명령 검색 · 글이 있을 때만 · 사용자 09-23).
        self.input = TextBox::new(placeholder)
            .with_clearable()
            .with_text(prefill);
        self.input.set_scale(self.scale);
        self.input.set_focused(true);
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
        self.last_query.clear();
        self.refilter(true);
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.prompt = None;
        self.anchor = None;
        self.input.set_focused(false);
    }

    /// 창 폭 · 상단 y(메뉴/툴바 아래) · 스케일.
    pub(crate) fn set_bounds(&mut self, win_w: i32, top: i32, scale: f32) {
        self.scale = scale;
        self.row_h = (26.0 * scale) as i32;
        let w = ((560.0 * scale) as i32)
            .min(win_w - (32.0 * scale) as i32)
            .max(200);
        let h = self.row_h * (MAX_ROWS as i32 + 1) + (16.0 * scale) as i32;
        self.bounds = Rect::new((win_w - w) / 2, top + (6.0 * scale) as i32, w, h);
        self.input.set_scale(scale);
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
    }

    /// ★ 지금 모드의 **실제 상자**(사용자 09-21 "이름 바꾸기는 간단한 형태로 — 불필요한 하단이 함께 표시된다"): 프롬프트(이름
    /// 바꾸기 · 저장소 추가)는 목록이 없으므로 **입력란 + 안내 한 줄**만 · 폭도 좁게. 명령 팔레트·줄 이동은 종전 크기.
    fn frame(&self) -> Rect {
        let b = self.bounds;
        if self.prompt.is_none() {
            return b;
        }
        let pad = (6.0 * self.scale) as i32;
        let w = ((380.0 * self.scale) as i32).min(b.w);
        let h = pad + self.row_h + pad / 2 + self.row_h * 3 / 4 + pad;
        match self.anchor {
            // 대상 탭 **바로 아래**(왼쪽 끝을 맞춘다) — 자리가 없으면(결과 탭이 창 아래쪽) **바로 위** · 그래도 안 되면 창 안으로
            // 밀어 넣는다(팝업 배치 규칙 · nexa-ctl `place_popup`과 같은 순서를 세로축에 · 가로는 밀어 넣기).
            Some(a) if self.win.0 > 0 && self.win.1 > 0 => {
                let gap = (3.0 * self.scale) as i32;
                let below = a.bottom() + gap;
                let y = if below + h <= self.win.1 {
                    below
                } else if a.y - gap - h >= 0 {
                    a.y - gap - h
                } else {
                    (self.win.1 - h).max(0)
                };
                let host = Rect::new(0, 0, self.win.0, self.win.1);
                nexa_ctl::geom::nudge_into(Rect::new(a.x, y, w, h), host)
            }
            _ => Rect::new(b.x + (b.w - w) / 2, b.y, w, h),
        }
    }

    fn layout(&mut self, inv: &mut Invalidations) {
        let b = self.frame();
        let p = (6.0 * self.scale) as i32;
        self.input
            .set_bounds(Rect::new(b.x + p, b.y + p, b.w - p * 2, self.row_h), inv);
    }

    fn rows_rect(&self) -> Rect {
        let b = self.bounds;
        let p = (6.0 * self.scale) as i32;
        let top = b.y + p + self.row_h + p / 2;
        Rect::new(b.x + p, top, b.w - p * 2, self.row_h * MAX_ROWS as i32)
    }

    /// 파일 모드 안내 글(읽는 중 · 상한 · …외 N) — 입력란 바로 아래 **결과 위**에 한 줄(사용자 10-07 "결과가 많아도 보이게").
    fn note_text(&self) -> String {
        if !(self.goto_mode && self.active == Active::Files) {
            return String::new();
        }
        let mut note = self.files_note.clone();
        if self.more_hidden > 0 {
            if !note.is_empty() {
                note.push_str(" · ");
            }
            note.push_str(&tf(Msg::PalMoreHidden, &[&self.more_hidden.to_string()]));
        }
        note
    }

    /// 결과 행 수(안내 줄이 있으면 한 줄 적게) · 결과 첫 행 y 오프셋.
    fn rows_visible(&self) -> usize {
        if self.note_text().is_empty() {
            MAX_ROWS
        } else {
            MAX_ROWS - 1
        }
    }
    fn rows_offset(&self) -> i32 {
        if self.note_text().is_empty() {
            0
        } else {
            self.row_h
        }
    }

    /// 점 아래 결과 행(전체 결과 기준 인덱스 · 목록 밖/빈 행 = None).
    fn row_at(&self, p: Point) -> Option<usize> {
        let rr = self.rows_rect();
        if self.goto.is_some() || self.prompt.is_some() || !rr.contains(p) {
            return None;
        }
        let y = p.y - rr.y - self.rows_offset();
        if y < 0 {
            return None;
        }
        let row = (y / self.row_h.max(1)) as usize;
        let i = self.top + row;
        (row < self.rows_visible() && i < self.matches.len()).then_some(i)
    }

    /// 선택 행이 보이도록 굴린다.
    fn reveal_sel(&mut self) {
        let vis = self.rows_visible();
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + vis {
            self.top = self.sel + 1 - vis;
        }
    }

    fn refilter(&mut self, force: bool) {
        if self.prompt.is_some() {
            self.matches.clear();
            return;
        }
        let q = self.input.text();
        if !force && q == self.last_query {
            return;
        }
        self.last_query = q.clone();
        // `:123` = 줄 이동(항목 필터 대신 안내 한 줄).
        if let Some(rest) = q.trim().strip_prefix(':') {
            self.goto = Some(rest.trim().parse::<usize>().ok().filter(|n| *n > 0));
            self.matches.clear();
            self.sel = 0;
            self.top = 0;
            return;
        }
        self.goto = None;
        self.tail_line = None;
        self.mode_hint.clear();
        self.more_hidden = 0;
        // ★ Goto 모드 = 접두로 원천을 고른다(사용자 10-07 · VS Code식) — 원천은 **참조**(복제 0 · 사용자 10-07 "멈춤").
        let mut files_mode = false;
        let mut by_kind = false;
        let q = if self.goto_mode {
            let (mode, rest, line) = parse_goto(&q);
            self.tail_line = line;
            match mode {
                GotoMode::Files | GotoMode::Line => {
                    files_mode = true;
                    self.active = Active::Files;
                }
                GotoMode::Commands => self.active = Active::Cmds,
                GotoMode::Symbols => self.active = Active::Syms,
                GotoMode::SymbolsByKind => {
                    self.active = Active::Syms;
                    by_kind = true;
                }
                GotoMode::WorkspaceSymbols => {
                    self.cmds = Src::default();
                    self.active = Active::Own;
                    self.mode_hint = t(Msg::PalHintWorkspaceSym).to_string();
                }
                GotoMode::QuickSearch => {
                    let needle = rest.trim().to_lowercase();
                    let items: Vec<(String, String)> = self
                        .doc_lines
                        .iter()
                        .enumerate()
                        .filter(|(_, l)| needle.is_empty() || l.to_lowercase().contains(&needle))
                        .take(300)
                        .map(|(i, l)| {
                            (
                                format!("goto.line:{}", i + 1),
                                format!("{}: {}", i + 1, l.trim()),
                            )
                        })
                        .collect();
                    // 내용 검색은 포함 여부가 곧 결과 — 퍼지 점수는 생략(순서 = 줄 순).
                    self.cmds = Src::from_items(items, false);
                    self.active = Active::Own;
                    self.matches = (0..self.cmds.len()).collect();
                    self.sel = 0;
                    self.top = 0;
                    return;
                }
                GotoMode::Help => {
                    self.cmds = Src::from_items(self.help.clone(), false);
                    self.active = Active::Own;
                    self.matches = (0..self.cmds.len()).collect();
                    self.sel = 0;
                    self.top = 0;
                    return;
                }
            }
            rest
        } else {
            q
        };
        // 띄어쓰기 = 낱말 토큰(각각 퍼지 · AND).
        let tokens: Vec<Vec<char>> = q
            .to_lowercase()
            .split_whitespace()
            .map(|t| t.chars().collect())
            .collect();
        let mut hidden = 0usize;
        let src = self.items();
        // 빈 질의 = 점수 없이 원천 순서 그대로(열린 탭 → 최근 → 프로젝트 = 이미 그 순서 · 10k 정렬 안 함).
        let mut matches: Vec<usize> = if tokens.is_empty() {
            (0..src.len()).collect()
        } else {
            let mut scored: Vec<(i32, usize)> = src
                .lower
                .iter()
                .enumerate()
                .filter_map(|(i, l)| {
                    // 파일 모드 라벨 = `이름  -  경로` → 이름 경계(바이트) · 그 안 일치 우선.
                    let name_end = if files_mode { l.find("  -  ") } else { None };
                    query_score(&tokens, l, name_end).map(|s| {
                        (
                            s + if files_mode {
                                tier_bonus(&src.items[i].0)
                            } else {
                                0
                            },
                            i,
                        )
                    })
                })
                .collect();
            // 상위 K만 — 넘치면 부분 정렬로 K개를 뽑은 뒤 그 안에서만 정렬(10k 전부 정렬 안 함).
            if scored.len() > TOP_K {
                hidden = scored.len() - TOP_K;
                scored.select_nth_unstable_by(TOP_K, |a, b| b.0.cmp(&a.0));
                scored.truncate(TOP_K);
            }
            scored.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| src.items[a.1].1.cmp(&src.items[b.1].1))
            });
            scored.into_iter().map(|(_, i)| i).collect()
        };
        if by_kind {
            // 종류별 = 라벨의 `—  종류` 뒤를 1차 키로(안정 정렬 · 원래 순서 보존).
            matches.sort_by_cached_key(|&i| {
                src.items[i]
                    .1
                    .split("  -  ")
                    .nth(1)
                    .unwrap_or("")
                    .trim()
                    .to_string()
            });
        }
        self.matches = matches;
        self.more_hidden = hidden;
        self.sel = 0;
        self.top = 0;
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> PaletteAction {
        if !self.open {
            return PaletteAction::None;
        }
        // ★ 입력란의 우클릭 편집 메뉴가 떠 있으면 모든 사건은 입력란(메뉴)에게 — 메뉴가 상자 밖으로 펼쳐져도 팔레트가 닫히지 않는다
        //   (사용자 09-29 "우클릭 표시가 밑으로 숨고 선택도 안 됨").
        if self.input.popup_open() {
            let outside = match *ev {
                InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y } => {
                    let p = Point { x, y };
                    !self.frame().contains(p) && !self.input.popup_bounds().contains(p)
                }
                _ => false,
            };
            if outside {
                // 상자도 메뉴도 아닌 곳 = 메뉴를 닫고 바깥 클릭 규칙(취소)으로 흘린다.
                self.input.close_menu();
            } else {
                self.input.on_event(ev, inv);
                self.apply_edit_ctx(inv);
                return PaletteAction::None;
            }
        }
        match *ev {
            InputEvent::Key {
                key: Key::Escape, ..
            } => return PaletteAction::Close,
            InputEvent::Key {
                key: Key::Enter, ..
            } => {
                if let Some(id) = self.prompt.clone() {
                    let text = self.input.text().trim().to_string();
                    return if text.is_empty() {
                        PaletteAction::Close
                    } else {
                        PaletteAction::Prompt { id, text }
                    };
                }
                if let Some(g) = self.goto {
                    return match g {
                        Some(n) => PaletteAction::Pick(format!("goto.line:{n}")),
                        None => PaletteAction::None,
                    };
                }
                return match self.matches.get(self.sel) {
                    Some(&i) => {
                        let id = self.items().items[i].0.clone();
                        // `이름:줄` = 파일 항목이면 열고 그 줄로(`#L` 꼬리 · 호스트 `goto_pick`).
                        match self.tail_line {
                            Some(n) if id.starts_with("tab:") || id.starts_with("file.") => {
                                PaletteAction::Pick(format!("{id}#L{n}"))
                            }
                            _ => PaletteAction::Pick(id),
                        }
                    }
                    None => PaletteAction::Close,
                };
            }
            InputEvent::Key { key: Key::Up, .. } => {
                self.sel = self.sel.saturating_sub(1);
                self.reveal_sel();
                return PaletteAction::None;
            }
            InputEvent::Key { key: Key::Down, .. } => {
                if self.sel + 1 < self.matches.len() {
                    self.sel += 1;
                }
                self.reveal_sel();
                return PaletteAction::None;
            }
            InputEvent::Key {
                key: Key::PageUp, ..
            } => {
                self.sel = self.sel.saturating_sub(MAX_ROWS);
                self.reveal_sel();
                return PaletteAction::None;
            }
            InputEvent::Key {
                key: Key::PageDown, ..
            } => {
                self.sel = (self.sel + MAX_ROWS).min(self.matches.len().saturating_sub(1));
                self.reveal_sel();
                return PaletteAction::None;
            }
            // ★ 마우스(사용자 09-19): 목록 위에서 움직이면 그 행이 선택(키보드 선택과 같은 강조 하나) · 휠 = 목록 굴리기 ·
            //   클릭 = 실행. 같은 자리의 MouseMove는 무시한다(키보드로 옮긴 선택이 커서 아래 행으로 튀지 않게).
            InputEvent::MouseMove { x, y } => {
                // 입력란이 먼저(드래그 선택 · hover) — 목록 hover는 그 다음(사용자 09-29 "드래그가 안 된다").
                self.input.on_event(ev, inv);
                if (x, y) != self.last_mouse {
                    self.last_mouse = (x, y);
                    if let Some(i) = self.row_at(Point { x, y }) {
                        self.sel = i;
                    }
                }
                return PaletteAction::None;
            }
            InputEvent::Wheel { .. } if self.prompt.is_some() => {
                // 프롬프트 = 목록이 없다 → 휠은 입력란(가로 스크롤).
                self.input.on_event(ev, inv);
                return PaletteAction::None;
            }
            InputEvent::Wheel { delta } => {
                let rows = if delta > 0 { -3isize } else { 3 };
                let max_top = self.matches.len().saturating_sub(MAX_ROWS);
                self.top = (self.top as isize + rows).clamp(0, max_top as isize) as usize;
                // 선택은 보이는 범위 안으로(커서가 목록 위에 있으면 그 행).
                let under = self.row_at(Point {
                    x: self.last_mouse.0,
                    y: self.last_mouse.1,
                });
                self.sel = under.unwrap_or_else(|| {
                    self.sel
                        .clamp(self.top, (self.top + MAX_ROWS).saturating_sub(1))
                        .min(self.matches.len().saturating_sub(1))
                });
                return PaletteAction::None;
            }
            // 바깥 **우클릭**도 취소(사용자 09-21 — 좌클릭만 닫히고 우클릭은 상자가 남았다). 상자 안의 우클릭은 입력란의 편집 메뉴로.
            InputEvent::RightDown { x, y } if !self.frame().contains(Point { x, y }) => {
                return PaletteAction::Close;
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if !self.frame().contains(p) {
                    return PaletteAction::Close;
                }
                if self.rows_rect().contains(p) {
                    if let Some(i) = self.row_at(p) {
                        return PaletteAction::Pick(self.items().items[self.matches[i]].0.clone());
                    }
                    return PaletteAction::None;
                }
            }
            _ => {}
        }
        self.input.on_event(ev, inv);
        self.apply_edit_ctx(inv);
        self.refilter(false);
        PaletteAction::None
    }

    /// 입력란 편집 메뉴의 복사/잘라내기/붙여넣기(클립보드는 호스트 몫 · 설정 창 검색란과 같은 규칙).
    fn apply_edit_ctx(&mut self, inv: &mut Invalidations) {
        let Some(act) = self.input.take_edit_ctx() else {
            return;
        };
        self.clip(act, inv);
    }

    /// 클립보드 동작(편집 메뉴 · Ctrl+C/X/V 키맵이 팔레트가 열려 있을 때 여기로 — 사용자 09-29 "Ctrl 단축키가 그리드로 간다").
    pub(crate) fn clip(&mut self, act: nexa_ctl::controls::EditCtxAction, inv: &mut Invalidations) {
        use nexa_ctl::controls::EditCtxAction;
        match act {
            EditCtxAction::Copy => {
                if let Some(text) = self.input.copy_selection() {
                    let _ = crate::clipboard::write_text(&text);
                }
            }
            EditCtxAction::Cut => {
                if let Some(text) = self.input.cut_selection(inv) {
                    let _ = crate::clipboard::write_text(&text);
                }
            }
            EditCtxAction::Paste => {
                if let Some(text) = crate::clipboard::read_text() {
                    self.input.paste(&text, inv);
                }
            }
            EditCtxAction::Custom(_) => {}
        }
        self.refilter(false);
    }

    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.open {
            return;
        }
        self.paint_body(dc, th);
        // 입력란 편집 메뉴 = 맨 위 층(힌트·목록 위 · 프롬프트 모드도 같다 — 사용자 09-29 "그리기가 잘못 적용").
        self.input.paint_popup(dc, th);
    }

    fn paint_body(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        let b = self.frame();
        // 그림자 느낌의 테두리 2겹
        dc.fill_rect(Rect::new(b.x - 1, b.y - 1, b.w + 2, b.h + 2), th.border);
        dc.fill_rect(b, th.chrome_bg);
        self.input.paint(dc, th);
        if let (Some(a), true) = (self.anchor, self.prompt.is_some()) {
            // ★ 수정 중인 대상 강조: 대상 탭에 **강조색 테두리**(2px) + 상자의 그쪽 변도 같은 색 — 어느 탭의 이름을 바꾸는지
            //   테두리 색이 이어 준다(탭이 여럿이어도 헷갈리지 않는다).
            let w2 = (2.0 * self.scale).round().max(2.0) as i32;
            for r in [
                Rect::new(a.x, a.y, a.w, w2),
                Rect::new(a.x, a.bottom() - w2, a.w, w2),
                Rect::new(a.x, a.y, w2, a.h),
                Rect::new(a.right() - w2, a.y, w2, a.h),
            ] {
                dc.fill_rect(r, th.accent);
            }
            let edge_y = if b.y >= a.bottom() {
                b.y - 1
            } else {
                b.bottom() - w2 + 1
            };
            dc.fill_rect(Rect::new(b.x - 1, edge_y, b.w + 2, w2), th.accent);
        }
        if self.prompt.is_some() {
            // 간단한 입력 상자: 입력란 아래 안내 한 줄(Enter 적용 · Esc 취소)만 — 목록 영역은 그리지 않는다.
            let pad = (6.0 * self.scale) as i32;
            let th_px = dc.text_height();
            let y = b.y + pad + self.row_h + pad / 2;
            dc.text(
                b.x + pad + (4.0 * self.scale) as i32,
                y + (self.row_h * 3 / 4 - th_px) / 2,
                b,
                &self.hint,
                th.text_dim,
            );
            return;
        }
        let rr = self.rows_rect();
        let pad = (8.0 * self.scale) as i32;
        let th_px = dc.text_height();
        if let Some(g) = self.goto {
            let (text, color) = match g {
                Some(n) => (tf(Msg::PalGotoLine, &[&n.to_string()]), th.text),
                None => (t(Msg::PalGotoLineHint).to_string(), th.text_dim),
            };
            let r = Rect::new(rr.x, rr.y, rr.w, self.row_h);
            if g.is_some() {
                dc.fill_rect(r, th.sel_bg);
            }
            // 긴 항목(경로 · 최근 프로젝트)은 가운데 … — Alt = 전체(사용자 09-22).
            let shown = nexa_ctl::draw::ellipsize_middle(dc, &text, r.w - pad * 2);
            dc.text(r.x + pad, rr.y + (self.row_h - th_px) / 2, r, &shown, color);
            return;
        }
        if self.matches.is_empty() {
            let msg = if self.mode_hint.is_empty() {
                t(Msg::PalNoMatch).to_string()
            } else {
                self.mode_hint.clone()
            };
            dc.text(
                rr.x + pad,
                rr.y + (self.row_h - th_px) / 2,
                rr,
                &msg,
                th.text_dim,
            );
            return;
        }
        // ★ 안내 줄(읽는 중 · 상한 · …외 N개)은 **결과 위**(입력란 바로 아래) — 결과가 12행을 채워도 보인다(사용자 10-07).
        let note = self.note_text();
        let off = self.rows_offset();
        if !note.is_empty() {
            let r = Rect::new(rr.x, rr.y, rr.w, self.row_h);
            let shown = nexa_ctl::draw::ellipsize_middle(dc, &note, r.w - pad * 2);
            dc.text(
                r.x + pad,
                rr.y + (self.row_h - th_px) / 2,
                r,
                &shown,
                th.text_dim,
            );
        }
        for (row, &i) in self
            .matches
            .iter()
            .skip(self.top)
            .take(self.rows_visible())
            .enumerate()
        {
            let y = rr.y + off + row as i32 * self.row_h;
            let r = Rect::new(rr.x, y, rr.w, self.row_h);
            if self.top + row == self.sel {
                dc.fill_rect(r, th.sel_bg);
            }
            dc.text(
                r.x + pad,
                y + (self.row_h - th_px) / 2,
                r,
                &self.items().items[i].1,
                th.text,
            );
        }
    }
}

/// 퍼지 점수 — 대소문자 무관 부분열 매치. 연속 매치·단어 첫 글자 매치에 가산 · 없으면 None. 빈 질의 = 0.
#[cfg(test)]
pub(crate) fn fuzzy_score(query: &str, label: &str) -> Option<i32> {
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    fuzzy_score_lower(&q, &label.to_lowercase())
}

/// 퍼지 점수 코어 — 질의(소문자 글자들)·라벨(소문자) 모두 준비된 것으로 **할당 없이** 한 번 훑는다(키 입력마다 10k 라벨).
/// 가점 = 글자 +1 · 연속 +3 · **낱말 머리**(앞 글자가 영숫자 아님 = `_` `/` `.` `-` 공백 · 또는 첫 글자) +6 · 연속 부분 문자열 +40 ·
/// 감점 = 첫 일치~끝 일치 사이의 **빈 글자 수**(촘촘할수록 위 · 상한 30 · `cf35`가 `cflz0d…` 잡음보다 `cache_file_35`에 가게).
pub(crate) fn fuzzy_score_lower(q: &[char], lower: &str) -> Option<i32> {
    if q.is_empty() {
        return Some(0);
    }
    let qs: String = q.iter().collect();
    let mut score = if lower.contains(&qs) { 40 } else { 0 };
    let mut qi = 0usize;
    let mut prev_hit = false;
    let mut prev: Option<char> = None;
    let mut first: Option<usize> = None;
    let mut last = 0usize;
    for (i, c) in lower.chars().enumerate() {
        if qi < q.len() && c == q[qi] {
            score += 1;
            if prev_hit {
                score += 3;
            }
            if prev.is_none_or(|p| !p.is_alphanumeric()) {
                score += 6;
            }
            prev_hit = true;
            qi += 1;
            if first.is_none() {
                first = Some(i);
            }
            last = i;
        } else {
            prev_hit = false;
        }
        prev = Some(c);
    }
    if qi != q.len() {
        return None;
    }
    let span = last + 1 - first.unwrap_or(0);
    let gaps = (span.saturating_sub(q.len())).min(30) as i32;
    Some(score - gaps)
}

/// 질의 전체 점수 — **띄어쓰기 = 낱말 AND**(토큰마다 퍼지 · 하나라도 안 맞으면 없음 · 점수 합) · `name_end`가 있으면(파일 모드 라벨
/// `이름  -  경로`) **이름 안에서 맞으면 +30**(경로 글자만 맞는 것보다 위 · 사용자 10-07 `cache file 35` · `cf35`).
pub(crate) fn query_score(
    tokens: &[Vec<char>],
    lower: &str,
    name_end: Option<usize>,
) -> Option<i32> {
    let mut total = 0i32;
    for t in tokens {
        let s = match name_end {
            Some(end) => match fuzzy_score_lower(t, &lower[..end]) {
                Some(s) => s + 30,
                None => fuzzy_score_lower(t, lower)?,
            },
            None => fuzzy_score_lower(t, lower)?,
        };
        total += s;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 09-27(사용자 Linux 실기): 팔레트 열림 중 IME 확정 글자(`Ime::Commit`)가 편집기로 새고 팔레트엔 안 들어갔다 → 호스트가
    /// `Char` 사건으로 팔레트에 넘긴다. 여기서는 팔레트가 한글 글자를 검색어로 받고 그 글자로 거르는지 본다.
    #[test]
    fn hangul_chars_reach_the_query_and_filter() {
        let mut p = Palette::new();
        p.set_commands(vec![
            ("view.log".into(), "보기: 로그 창".into()),
            ("file.new".into(), "새 편집기".into()),
        ]);
        p.open("");
        let mut inv = Invalidations::default();
        for c in "로그".chars() {
            p.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
        }
        assert_eq!(p.query(), "로그");
        assert_eq!(p.matches.len(), 1, "'로그'로 거른 결과 = 로그 창 하나");
        assert_eq!(p.cmds.items[p.matches[0]].0, "view.log");
        p.set_preedit("ㅊ", &mut inv);
        assert_eq!(p.query(), "로그", "조합 중 글자는 본문이 아니다");
    }

    use super::{fuzzy_score, parse_goto, GotoMode};

    /// 점수 규칙(사용자 10-07): 띄어쓰기 = 낱말 AND · 이름 안 일치 우선 · 촘촘한 일치가 흩어진 일치보다 위 · 오타(빠진 글자)는 부분열로 허용.
    #[test]
    fn query_score_rules() {
        use super::query_score;
        let tok = |q: &str| -> Vec<Vec<char>> {
            q.to_lowercase()
                .split_whitespace()
                .map(|t| t.chars().collect())
                .collect()
        };
        let a = "cache_file_35.txt  -  downloads/many/cache_file_35.txt";
        let noise = "cflz3x5d.o  -  target/debug/deps/cflz3x5d.o";
        let ne = |l: &str| l.find("  -  ");
        assert!(
            query_score(&tok("cache file 35"), a, ne(a)).is_some(),
            "띄어쓰기 = AND"
        );
        assert!(
            query_score(&tok("chche file 35"), a, ne(a)).is_some(),
            "빠진 글자 허용"
        );
        assert!(
            query_score(&tok("cache zzz"), a, ne(a)).is_none(),
            "토큰 하나라도 안 맞으면 없음"
        );
        let s_a = query_score(&tok("cf35"), a, ne(a)).expect("a");
        let s_n = query_score(&tok("cf35"), noise, ne(noise)).expect("noise");
        assert!(
            s_a > s_n,
            "cf35: 머리글자·촘촘함 = cache_file_35 {s_a} > 잡음 {s_n}"
        );
        let s_name =
            query_score(&tok("many"), "x.txt  -  downloads/many/x.txt", Some(5)).expect("path");
        let s_in_name =
            query_score(&tok("many"), "many.txt  -  downloads/x/many.txt", Some(8)).expect("name");
        assert!(s_in_name > s_name, "이름 안 일치가 경로 일치보다 위");
    }

    /// 접두 풀이(사용자 10-07): `>` 명령 · `:` 줄 · `@`/`@:` 심볼 · `#` 프로젝트 심볼 · `%` 빠른 검색 · `?` 도움 · `이름:줄`.
    #[test]
    fn goto_prefixes() {
        assert_eq!(
            parse_goto("tabbar"),
            (GotoMode::Files, "tabbar".into(), None)
        );
        assert_eq!(
            parse_goto("app/out"),
            (GotoMode::Files, "app/out".into(), None)
        );
        assert_eq!(
            parse_goto("tabbar.rs:1172"),
            (GotoMode::Files, "tabbar.rs".into(), Some(1172))
        );
        assert_eq!(
            parse_goto(">save"),
            (GotoMode::Commands, "save".into(), None)
        );
        assert_eq!(parse_goto(":1172"), (GotoMode::Line, "1172".into(), None));
        assert_eq!(
            parse_goto("@sync"),
            (GotoMode::Symbols, "sync".into(), None)
        );
        assert_eq!(
            parse_goto("@:"),
            (GotoMode::SymbolsByKind, String::new(), None)
        );
        assert_eq!(
            parse_goto("#sync_bar"),
            (GotoMode::WorkspaceSymbols, "sync_bar".into(), None)
        );
        assert_eq!(
            parse_goto("%pinned"),
            (GotoMode::QuickSearch, "pinned".into(), None)
        );
        assert_eq!(parse_goto("?"), (GotoMode::Help, String::new(), None));
        assert_eq!(
            parse_goto("a:b"),
            (GotoMode::Files, "a:b".into(), None),
            "숫자 아니면 이름의 일부"
        );
    }

    #[test]
    fn subsequence_and_ranking() {
        assert!(fuzzy_score("syntax", "Set Syntax: SQL").is_some());
        assert!(fuzzy_score("xyz", "Set Syntax: SQL").is_none());
        let a = fuzzy_score("sql", "Set Syntax: SQL").unwrap_or(0);
        let b = fuzzy_score("sql", "Set Syntax: Plain Text").unwrap_or(-1);
        assert!(a > b);
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    /// 이름 바꾸기 상자(프롬프트 · 사용자 09-21): 대상 탭 바로 아래에 붙는다(아래에 자리가 없으면 위) · 상자는 입력란 + 안내 한 줄 ·
    /// **바깥 좌클릭도 우클릭도 취소** · 상자 안의 클릭은 취소가 아니다.
    #[test]
    fn prompt_is_anchored_compact_and_outside_clicks_cancel() {
        use super::{Palette, PaletteAction};
        use nexa_ctl::geom::Rect;
        use nexa_ctl::{InputEvent, Invalidations};
        let mut p = Palette::new();
        p.set_bounds(1000, 40, 1.0);
        p.set_window(1000, 700);
        let mut inv = Invalidations::default();
        let tab = Rect::new(120, 90, 110, 28);
        p.open_prompt_at("tab.rename:1", "hint", "Script_1", Some(tab));
        let f = p.frame();
        assert_eq!(
            (f.x, f.y),
            (tab.x, tab.bottom() + 3),
            "탭 바로 아래 · 왼쪽 끝 맞춤"
        );
        assert!(f.h < 26 * 4, "목록 영역이 없다: {f:?}");
        let inside = InputEvent::RightDown {
            x: f.x + 10,
            y: f.y + 10,
        };
        assert!(!matches!(
            p.on_event(&inside, &mut inv),
            PaletteAction::Close
        ));
        let outside = InputEvent::RightDown { x: 900, y: 600 };
        assert!(matches!(
            p.on_event(&outside, &mut inv),
            PaletteAction::Close
        ));
        let left = InputEvent::MouseDown {
            x: 900,
            y: 600,
            shift: false,
            primary: false,
        };
        assert!(matches!(p.on_event(&left, &mut inv), PaletteAction::Close));
        // 창 아래쪽의 탭(결과 탭) = 상자가 탭 위로.
        p.close();
        let low = Rect::new(80, 680, 90, 18);
        p.open_prompt_at("result.rename:7", "hint", "Result 1", Some(low));
        let f = p.frame();
        assert!(f.bottom() <= low.y, "아래에 자리가 없으면 위로: {f:?}");
        // 대상을 모르면 종전처럼 위 가운데.
        p.close();
        p.open_prompt_at("x", "hint", "", None);
        assert_eq!(p.frame().y, p.bounds.y);
    }

    /// 마우스(사용자 09-19): 목록 위에서 움직이면 그 행 선택 · 같은 자리 MouseMove는 무시 · 휠 = 굴리기 · 클릭 = 실행 ·
    /// ↓는 보이는 행 수를 넘어 끝까지(목록이 따라 굴러간다).
    #[test]
    fn mouse_hover_wheel_click_and_scrolling() {
        use super::{Palette, PaletteAction, MAX_ROWS};
        use nexa_ctl::{InputEvent, Invalidations, Key};
        let mut p = Palette::new();
        p.set_commands(
            (0..40)
                .map(|i| (format!("cmd.{i}"), format!("Command {i:02}")))
                .collect(),
        );
        p.set_bounds(1000, 40, 1.0);
        p.open("");
        let mut inv = Invalidations::default();
        let rr = p.rows_rect();
        let at = |row: i32| InputEvent::MouseMove {
            x: rr.x + 10,
            y: rr.y + row * p.row_h + 3,
        };
        let ev = at(3);
        p.on_event(&ev, &mut inv);
        assert_eq!(p.sel, 3, "hover = 선택");
        // 키보드로 옮긴 뒤 같은 자리 MouseMove가 와도 선택은 그대로.
        p.on_event(
            &InputEvent::Key {
                key: Key::Down,
                shift: false,
                primary: false,
            },
            &mut inv,
        );
        assert_eq!(p.sel, 4);
        p.on_event(&ev, &mut inv);
        assert_eq!(p.sel, 4, "같은 자리 = 무시");
        // 휠 아래 = 3행 굴림 · 선택은 커서 아래 행.
        p.on_event(&InputEvent::Wheel { delta: -120 }, &mut inv);
        assert_eq!(p.top, 3);
        assert_eq!(p.sel, 6, "커서 아래 행(3행째) = top 3 + 3");
        // ↓를 끝까지 — 보이는 행 수를 넘어 마지막 항목까지.
        for _ in 0..100 {
            p.on_event(
                &InputEvent::Key {
                    key: Key::Down,
                    shift: false,
                    primary: false,
                },
                &mut inv,
            );
        }
        assert_eq!(p.sel, 39);
        assert_eq!(p.top, 40 - MAX_ROWS);
        // 클릭 = 그 행 실행(굴린 위치 기준).
        let click = InputEvent::MouseDown {
            x: rr.x + 10,
            y: rr.y + 3,
            shift: false,
            primary: false,
        };
        assert!(matches!(
            p.on_event(&click, &mut inv),
            PaletteAction::Pick(id) if id == format!("cmd.{}", 40 - MAX_ROWS)
        ));
    }
}
