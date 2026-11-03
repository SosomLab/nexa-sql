//! ★ **Import Data 창 2차**(docs/127 §2-2 · 11-04 · T-8 ⑥ · 1차 = 09-26 T-236 B-3): 탐색기 표/스키마 우클릭 ▸ Import Data… → 파일 창 → 이 창.
//! **한 창 · 세 구역** = A 원료(형식 · 시트 · 헤더 · 구분자 · 인코딩 · 다듬기 · NULL 글 — 바꾸면 호스트가 스레드로 다시 살펴본다 `Reinspect`) ·
//! B 미리보기(결과 그리드 재사용 · 앞 N행 · 헤더 = `이름 · 추론 형` · D-453) · C 대상(기존 표 / 새 표 라디오 · 표 이름 · **열 매핑 표**(포함 ✓ ·
//! 원료 열 · 추론 형 · 대상 열 콤보 또는 새 표 형 콤보 · 표본) · 적재 옵션(배치 · 커밋 간격 · 경로 · 빈 값 NULL · 오류 시 멈춤/건너뜀 D-452) ·
//! 시작/취소/닫기 · 진행·결과). 시작 → 호스트가 세션 워커에 `Runner::import_file`(새 표면 CREATE 먼저) · 취소 = 배치 경계.
//!
//! 창 골격은 1차 그대로(winit 창 + `Presenter` · 포커스 ≤ 1 · 마우스는 커서 아래 컨트롤에만 · 콤보 팝업이 열려 있으면 그 콤보가 먼저 · Esc = 닫기/취소).

use nexa_ctl::controls::{ComboItem, RadioGroup, RadioOption};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::{FontSet, RasterCtx};
use nexa_ctl::theme::{FontPrefs, Theme};
use nexa_ctl::{
    Button, Checkbox, Combo, ComboControl, Control, InputEvent, Invalidations, TextBox, Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_core::{Dialect, ResultSet, Value};
use nsql_i18n::{t, tf, Msg};
use nsql_run::bulk::{
    BulkMode, CreateCol, CreateSpec, ImportSpec, Inspect, OnError, SourceFormat, SourceOpts,
};
use std::path::PathBuf;
use std::rc::Rc;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) enum ImportAction {
    None,
    Paint,
    /// 시작(사양은 창의 옵션에서).
    Start(Box<ImportSpec>),
    /// 진행 중 취소.
    Cancel,
    Close,
    /// 원료 옵션이 바뀌었다 — 호스트가 `source_opts()`로 다시 살펴본다(스레드).
    Reinspect,
    /// 새 표 DDL 보기(읽기 전용 글 창).
    ShowDdl(String),
}

const MODES: [BulkMode; 4] = [
    BulkMode::Auto,
    BulkMode::Driver,
    BulkMode::MultiRow,
    BulkMode::Single,
];

/// 호스트가 채우는 열기 인자.
pub(crate) struct ImportOpen {
    /// 기존 표 표기(인용된 `schema.table`) · `None` = 새 표.
    pub existing: Option<String>,
    /// 새 표를 둘 스키마(표기용 · 없으면 접속 기본).
    pub schema: Option<String>,
    /// 새 표 기본 이름(파일 줄기 정리값).
    pub new_name: String,
    pub dialect: Dialect,
    pub path: PathBuf,
    pub defaults: Defaults,
}

/// 설정 기본값(`bulk.*` · `import.*`).
#[derive(Clone, Debug)]
pub(crate) struct Defaults {
    pub batch: usize,
    pub commit: usize,
    pub mode: BulkMode,
    pub encoding: String,
    pub on_error: OnError,
    pub text_margin_pct: u32,
    pub json_max_mb: u32,
}

/// 매핑 표 한 줄 = 원료 열 하나.
struct MapRow {
    on: Checkbox,
    src: String,
    kind: String,
    nullable: bool,
    sample: String,
    /// 기존 표 모드: 대상 열(값 `""` = 건너뜀).
    target: Combo,
    /// 새 표 모드: 열 이름(접힌 식별자) · 형 콤보.
    new_name: String,
    ty: Combo,
}

pub(crate) struct ImportWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    path: PathBuf,
    dialect: Dialect,
    schema: Option<String>,
    existing: Option<String>,
    defaults: Defaults,
    // ── A 원료
    cb_format: Combo,
    cb_sheet: Combo,
    sheets: Vec<String>,
    cb_header: Checkbox,
    cb_delim: Combo,
    cb_enc: Combo,
    cb_trim: Checkbox,
    tb_null: TextBox,
    /// 콤보를 프로그램이 맞추는 중(변경 알림 무시).
    syncing: bool,
    // ── B 미리보기
    grid: crate::grid::Grid,
    inspect: Option<Inspect>,
    inspect_err: Option<String>,
    inspecting: bool,
    // ── C 대상
    rg_target: RadioGroup,
    tb_table: TextBox,
    btn_ddl: Button,
    rows: Vec<MapRow>,
    /// 기존 표 열(이름 · 형) — 호스트가 메타에서 채운다(`None` = 아직).
    table_cols: Option<Vec<(String, String)>>,
    map_scroll: i32,
    map_area: Rect,
    tb_batch: TextBox,
    tb_commit: TextBox,
    btn_mode: Button,
    mode_label: String,
    mode_idx: usize,
    cb_empty_null: Checkbox,
    rg_on_error: RadioGroup,
    btn_start: Button,
    btn_cancel: Button,
    btn_close: Button,
    running: bool,
    progress: Option<(u64, f64)>,
    result: Option<(String, bool)>,
    last_report: String,
}

const FORMATS: [(&str, Msg); 7] = [
    ("auto", Msg::ImpFmtAuto),
    ("csv", Msg::ExpFmtCsv),
    ("tsv", Msg::ExpFmtTsv),
    ("json", Msg::ExpFmtJson),
    ("jsonl", Msg::ExpFmtJsonl),
    ("xlsx", Msg::ExpFmtXlsx),
    ("parquet", Msg::ExpFmtParquet),
];
const DELIMS: [(&str, &str); 5] = [
    ("auto", ""),
    (",", ","),
    (";", ";"),
    ("tab", "⇥ tab"),
    ("|", "|"),
];
const ENCODINGS: [&str; 8] = [
    "auto",
    "utf-8",
    "euc-kr",
    "utf-16le",
    "utf-16be",
    "shift_jis",
    "gb18030",
    "windows-1252",
];

const OFF: Rect = Rect {
    x: -1000,
    y: -1000,
    w: 1,
    h: 1,
};

fn combo_items<'a>(items: impl Iterator<Item = (&'a str, String)>) -> Vec<ComboItem> {
    items.map(|(v, l)| ComboItem::new(v, l)).collect()
}

impl ImportWin {
    pub(crate) fn new() -> Self {
        let fmt_items = combo_items(FORMATS.iter().map(|(v, m)| (*v, t(*m).to_string())));
        let delim_items = combo_items(DELIMS.iter().map(|(v, l)| {
            (
                *v,
                if l.is_empty() {
                    t(Msg::ImpAuto).to_string()
                } else {
                    (*l).to_string()
                },
            )
        }));
        let enc_items = combo_items(ENCODINGS.iter().map(|v| {
            (
                *v,
                if *v == "auto" {
                    t(Msg::ImpAuto).to_string()
                } else {
                    (*v).to_string()
                },
            )
        }));
        let rg_target = RadioGroup::new(
            vec![
                RadioOption::new("existing", t(Msg::ImpExisting)),
                RadioOption::new("new", t(Msg::ImpNewTable)),
            ],
            0,
        );
        let rg_on_error = RadioGroup::new(
            vec![
                RadioOption::new("stop", t(Msg::ImpErrStop)),
                RadioOption::new("skip", t(Msg::ImpErrSkip)),
            ],
            0,
        );
        let mut grid = crate::grid::Grid::default();
        grid.set_result(ResultSet::default());
        ImportWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            path: PathBuf::new(),
            dialect: Dialect::Sqlite,
            schema: None,
            existing: None,
            defaults: Defaults {
                batch: 1000,
                commit: 10_000,
                mode: BulkMode::Auto,
                encoding: "auto".into(),
                on_error: OnError::Stop,
                text_margin_pct: 50,
                json_max_mb: 256,
            },
            cb_format: Combo::new(fmt_items, 0),
            cb_sheet: Combo::new(Vec::new(), 0),
            sheets: Vec::new(),
            cb_header: Checkbox::new(t(Msg::ImpHeader), true),
            cb_delim: Combo::new(delim_items, 0),
            cb_enc: Combo::new(enc_items, 0),
            cb_trim: Checkbox::new(t(Msg::ImpTrim), false),
            tb_null: TextBox::new(""),
            syncing: false,
            grid,
            inspect: None,
            inspect_err: None,
            inspecting: false,
            rg_target,
            tb_table: TextBox::new(""),
            btn_ddl: Button::new(t(Msg::ImpShowDdl)),
            rows: Vec::new(),
            table_cols: None,
            map_scroll: 0,
            map_area: OFF,
            tb_batch: TextBox::new(""),
            tb_commit: TextBox::new(""),
            btn_mode: Button::new(""),
            mode_label: String::new(),
            mode_idx: 0,
            cb_empty_null: Checkbox::new(t(Msg::ImpEmptyNull), true),
            rg_on_error,
            btn_start: Button::new(t(Msg::ImpBtnStart)),
            btn_cancel: Button::new(t(Msg::ImpBtnCancel)),
            btn_close: Button::new(t(Msg::ImpBtnClose)),
            running: false,
            progress: None,
            result: None,
            last_report: String::new(),
        }
    }

    /// 열기 — 호스트는 곧바로 `source_opts()`로 살펴보기를 시작하고(`set_inspect`) · 기존 표면 열 목록을 채운다(`set_table_columns`).
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        owner: Option<&Window>,
        o: ImportOpen,
    ) {
        self.path = o.path;
        self.dialect = o.dialect;
        self.schema = o.schema;
        self.existing = o.existing;
        self.defaults = o.defaults;
        self.syncing = true;
        self.cb_format.select_value("auto");
        self.cb_sheet = Combo::new(Vec::new(), 0);
        self.sheets.clear();
        self.cb_header.set_checked(true);
        self.cb_delim.select_value("auto");
        let enc = if self.defaults.encoding.eq_ignore_ascii_case("auto") {
            "auto"
        } else {
            nsql_io::read::canonical_encoding(&self.defaults.encoding)
        };
        self.cb_enc.select_value(enc);
        self.cb_trim.set_checked(false);
        self.tb_null.set_text("");
        self.syncing = false;
        self.inspect = None;
        self.inspect_err = None;
        self.inspecting = true;
        self.grid.set_result(ResultSet::default());
        self.rows.clear();
        self.table_cols = None;
        self.map_scroll = 0;
        match &self.existing {
            Some(tbl) => {
                self.rg_target.select_value("existing");
                self.tb_table.set_text(tbl);
            }
            None => {
                self.rg_target.select_value("new");
                self.tb_table.set_text(&o.new_name);
            }
        }
        let _ = self.rg_target.take_changed();
        self.tb_batch.set_text(&self.defaults.batch.to_string());
        self.tb_commit.set_text(&self.defaults.commit.to_string());
        self.mode_idx = MODES
            .iter()
            .position(|m| *m == self.defaults.mode)
            .unwrap_or(0);
        self.sync_mode_label();
        self.cb_empty_null.set_checked(true);
        self.rg_on_error.select_value(self.defaults.on_error.name());
        let _ = self.rg_on_error.take_changed();
        self.running = false;
        self.progress = None;
        self.result = None;
        self.last_report.clear();
        self.sync_buttons();
        if let Some(w) = &self.window {
            w.set_title(&self.title());
            crate::winfocus::focus(w);
            self.redraw();
            return;
        }
        let size = winit::dpi::LogicalSize::new(960.0, 700.0);
        let attrs = Window::default_attributes()
            .with_title(self.title())
            .with_theme(theme)
            .with_resizable(true)
            .with_inner_size(size)
            .with_min_inner_size(winit::dpi::LogicalSize::new(720.0, 520.0));
        let mut attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        attrs = crate::wingeom::centered_over(attrs, owner, size.width, 4);
        let Ok(win) = el.create_window(crate::winfocus::attrs_headless(attrs)) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        win.set_ime_allowed(crate::input::system_ime());
        crate::winfocus::focus(&win);
        self.window = Some(win);
        self.redraw();
    }

    fn title(&self) -> String {
        let what = self
            .existing
            .clone()
            .unwrap_or_else(|| self.tb_table.text());
        format!("Nexa SQL - {} - {what}", t(Msg::WinImport))
    }

    fn is_new(&self) -> bool {
        self.rg_target.selected_value() == Some("new")
    }

    /// 지금 A 구역이 말하는 원료 옵션(호스트의 살펴보기 · 시작 둘 다 이것).
    pub(crate) fn source_opts(&self) -> SourceOpts {
        let delim = match self.cb_delim.value().as_str() {
            "auto" | "" => None,
            "tab" => Some(b'\t'),
            s => s.bytes().next(),
        };
        let sheet = {
            let v = self.cb_sheet.value();
            (!v.is_empty() && !self.sheets.is_empty()).then_some(v)
        };
        SourceOpts {
            format: SourceFormat::parse(&self.cb_format.value()).unwrap_or_default(),
            delim,
            header: self.cb_header.is_checked(),
            encoding: self.cb_enc.value(),
            sheet,
            trim: self.cb_trim.is_checked(),
            null_text: self.tb_null.text(),
            json_max_mb: self.defaults.json_max_mb,
        }
    }

    /// 살펴보기 시작(호스트) — 결과 전까지 "살펴보는 중…".
    pub(crate) fn set_inspecting(&mut self) {
        self.inspecting = true;
        self.sync_buttons();
        self.redraw();
    }

    /// 살펴본 결과(스레드) — 미리보기 그리드 · 매핑 표 다시 만들기(사용자가 고른 것은 원료 열 이름으로 보존) · 감지값으로 콤보 맞춤.
    pub(crate) fn set_inspect(&mut self, r: Result<Inspect, String>) {
        self.inspecting = false;
        match r {
            Err(m) => {
                self.inspect_err = Some(m);
                self.inspect = None;
                self.grid.set_result(ResultSet::default());
                self.rows.clear();
            }
            Ok(insp) => {
                self.inspect_err = None;
                // 미리보기 = 글 값 그대로 · 헤더 = 이름 · 추론 형.
                let mut rs = insp.preview.clone();
                for (i, c) in rs.columns.iter_mut().enumerate() {
                    if let Some(k) = insp.kinds.get(i) {
                        c.type_name = k.kind.label();
                        c.name = format!("{} · {}", c.name, k.kind.label());
                    }
                }
                self.grid.set_result(rs);
                // 감지값 → 콤보(알림 없이).
                self.syncing = true;
                if let Some(d) = insp.detected.delim {
                    if self.cb_delim.value() == "auto" {
                        let v = match d {
                            b'\t' => "tab".to_string(),
                            b => (b as char).to_string(),
                        };
                        self.cb_delim.select_value(&v);
                    }
                }
                if let Some(e) = insp.detected.encoding {
                    if self.cb_enc.value() == "auto" {
                        self.cb_enc.select_value(e);
                    }
                }
                if !insp.detected.sheets.is_empty() {
                    self.sheets = insp.detected.sheets.clone();
                    let items = combo_items(self.sheets.iter().map(|s| (s.as_str(), s.clone())));
                    let idx = insp
                        .detected
                        .sheet
                        .as_ref()
                        .and_then(|s| self.sheets.iter().position(|x| x == s))
                        .unwrap_or(0);
                    self.cb_sheet = Combo::new(items, idx);
                }
                let fmt = SourceFormat::parse(&self.cb_format.value()).unwrap_or_default();
                if fmt == SourceFormat::Auto {
                    self.cb_format
                        .select_value(SourceFormat::Auto.resolve(&self.path).name());
                }
                self.syncing = false;
                self.rebuild_rows(&insp);
                self.inspect = Some(insp);
            }
        }
        self.sync_buttons();
        self.redraw();
    }

    /// 기존 표의 열(이름 · 형) — 대상 콤보 후보 · 자동 짝(이름 → 위치).
    pub(crate) fn set_table_columns(&mut self, cols: Vec<(String, String)>) {
        self.table_cols = Some(cols);
        if let Some(insp) = self.inspect.clone() {
            self.rebuild_rows(&insp);
        }
        self.redraw();
    }

    fn norm(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }

    /// 매핑 표 다시 만들기 — 종전 줄의 사용자 선택(포함 · 대상 · 형)은 원료 열 이름으로 이어 받는다.
    fn rebuild_rows(&mut self, insp: &Inspect) {
        let prev: Vec<(String, bool, String, String)> = self
            .rows
            .iter()
            .map(|r| {
                (
                    r.src.clone(),
                    r.on.is_checked(),
                    r.target.value(),
                    r.ty.value(),
                )
            })
            .collect();
        let create =
            nsql_run::bulk::suggest_create(self.dialect, insp, self.defaults.text_margin_pct);
        let choices = nsql_io::infer::type_choices(self.dialect);
        let table_cols = self.table_cols.clone().unwrap_or_default();
        let mut used: Vec<String> = Vec::new();
        let mut rows = Vec::new();
        for (i, src) in insp.columns.iter().enumerate() {
            let k = insp.kinds.get(i);
            let kind = k.map_or_else(|| "?".to_string(), |k| k.kind.label());
            let nullable = k.is_none_or(|k| k.nullable);
            let sample: String = insp
                .preview
                .rows
                .iter()
                .take(3)
                .filter_map(|r| match r.get(i) {
                    Some(Value::Str(s)) => Some(s.chars().take(16).collect::<String>()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(", ");
            let old = prev.iter().find(|(s, ..)| s == src);
            // 대상 열 콤보: 건너뜀 + 표 열 · 자동 짝 = 이름(정규화) → 위치.
            let mut items = vec![ComboItem::new("", t(Msg::ImpSkipCol))];
            for (n, ty) in &table_cols {
                items.push(ComboItem::new(n.clone(), format!("{n}  ({ty})")));
            }
            let mut target = Combo::new(items, 0);
            let pick = match old {
                Some((_, _, tv, _)) if !tv.is_empty() || table_cols.is_empty() => tv.clone(),
                _ => {
                    let by_name = table_cols
                        .iter()
                        .find(|(n, _)| Self::norm(n) == Self::norm(src))
                        .map(|(n, _)| n.clone());
                    by_name
                        .or_else(|| {
                            table_cols
                                .get(i)
                                .filter(|(n, _)| !used.iter().any(|u| u == n))
                                .map(|(n, _)| n.clone())
                        })
                        .unwrap_or_default()
                }
            };
            if !pick.is_empty() {
                used.push(pick.clone());
            }
            target.select_value(&pick);
            // 새 표: 제안 이름 · 형(제안값 맨 위 + 방언 후보).
            let col = create.cols.get(i).cloned().unwrap_or(CreateCol {
                name: src.clone(),
                ty: "TEXT".into(),
                nullable,
            });
            let mut ty_items = vec![ComboItem::new(col.ty.clone(), col.ty.clone())];
            for c in &choices {
                if !c.eq_ignore_ascii_case(&col.ty) {
                    ty_items.push(ComboItem::new(*c, *c));
                }
            }
            if let Some((_, _, _, old_ty)) = old {
                if !old_ty.is_empty() && !ty_items.iter().any(|it| it.value == *old_ty) {
                    ty_items.insert(0, ComboItem::new(old_ty.clone(), old_ty.clone()));
                }
            }
            let mut ty = Combo::new(ty_items, 0);
            if let Some((_, _, _, old_ty)) = old {
                if !old_ty.is_empty() {
                    ty.select_value(old_ty);
                }
            }
            let on = old.is_none_or(|(_, on, ..)| *on);
            rows.push(MapRow {
                on: Checkbox::new("", on),
                src: src.clone(),
                kind,
                nullable,
                sample,
                target,
                new_name: col.name,
                ty,
            });
        }
        self.rows = rows;
    }

    fn sync_mode_label(&mut self) {
        let m = MODES[self.mode_idx];
        let label = match m {
            BulkMode::Auto => t(Msg::OptBulkAuto),
            BulkMode::Driver => t(Msg::OptBulkDriver),
            BulkMode::MultiRow => t(Msg::OptBulkMultiRow),
            BulkMode::Single => t(Msg::OptBulkSingle),
        };
        self.mode_label = format!("{}: {label}", t(Msg::ImpMode));
        self.btn_mode.set_label(self.mode_label.clone());
    }

    fn sync_buttons(&mut self) {
        let ready = !self.running && self.inspect.is_some() && !self.inspecting;
        self.btn_start.set_enabled(ready);
        self.btn_cancel.set_enabled(self.running);
        self.btn_close.set_enabled(!self.running);
        self.btn_ddl
            .set_enabled(self.is_new() && self.inspect.is_some());
    }

    /// 새 표 사양(포함 열만) · 원료 이름 ≠ 만든 이름이면 매핑.
    fn create_spec(&self) -> (CreateSpec, Vec<(String, String)>) {
        let mut cols = Vec::new();
        let mut map = Vec::new();
        for r in &self.rows {
            if !r.on.is_checked() {
                continue;
            }
            let ty = r.ty.value();
            cols.push(CreateCol {
                name: r.new_name.clone(),
                ty: if ty.is_empty() { "TEXT".into() } else { ty },
                nullable: r.nullable,
            });
            if !r.new_name.eq_ignore_ascii_case(&r.src) {
                map.push((r.src.clone(), r.new_name.clone()));
            }
        }
        (CreateSpec { cols }, map)
    }

    /// 새 표 표기 = 스키마 + 접힌 이름(인용).
    fn new_table_name(&self) -> String {
        let raw = self.tb_table.text().trim().to_string();
        let name = nsql_run::bulk::fold_ident(self.dialect, &nsql_io::infer::column_ident(&raw, 0));
        nsql_catalog::qualified(self.dialect, self.schema.as_deref().unwrap_or(""), &name)
    }

    /// 새 표 DDL 전문.
    fn ddl(&self) -> String {
        let (c, _) = self.create_spec();
        format!(
            "{};",
            nsql_run::bulk::create_table_sql_for(self.dialect, &self.new_table_name(), &c)
        )
    }

    /// 지금 옵션으로 사양을 만든다.
    fn spec(&self) -> ImportSpec {
        let on_error =
            OnError::parse(self.rg_on_error.selected_value().unwrap_or("stop")).unwrap_or_default();
        let mut skip: Vec<String> = self
            .rows
            .iter()
            .filter(|r| !r.on.is_checked())
            .map(|r| r.src.clone())
            .collect();
        let (table, create, map) = if self.is_new() {
            let (c, map) = self.create_spec();
            (self.new_table_name(), Some(c), map)
        } else {
            let mut map = Vec::new();
            for r in self.rows.iter().filter(|r| r.on.is_checked()) {
                let tv = r.target.value();
                if tv.is_empty() && self.table_cols.is_some() {
                    skip.push(r.src.clone());
                } else if !tv.is_empty() && !tv.eq_ignore_ascii_case(&r.src) {
                    map.push((r.src.clone(), tv));
                }
            }
            (
                self.existing
                    .clone()
                    .unwrap_or_else(|| self.tb_table.text().trim().to_string()),
                None,
                map,
            )
        };
        ImportSpec {
            table,
            source: self.source_opts(),
            cols: None,
            map,
            skip,
            create,
            on_error,
            batch_rows: self.tb_batch.text().trim().parse().unwrap_or(1000),
            commit_every: self.tb_commit.text().trim().parse().unwrap_or(10_000),
            mode: MODES[self.mode_idx],
            empty_null: self.cb_empty_null.is_checked(),
            opts: nsql_core::BulkOpts::default(),
            cancel: None,
        }
    }

    /// 호스트: 시작됨(워커로 보냈다).
    pub(crate) fn set_running(&mut self) {
        self.running = true;
        self.progress = Some((0, 0.0));
        self.result = None;
        self.sync_buttons();
        self.redraw();
    }

    pub(crate) fn set_progress(&mut self, rows: u64, secs: f64) {
        self.progress = Some((rows, secs));
        self.redraw();
    }

    /// 호스트: 끝(요약 또는 오류) — `report` = 덤프용 한 줄.
    pub(crate) fn set_result(&mut self, text: String, bad: bool, report: String) {
        self.running = false;
        self.result = Some((text, bad));
        self.last_report = report;
        self.sync_buttons();
        self.redraw();
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    /// 자체 시험 덤프(`import.dump:`): 상태 · 원료 옵션 · 감지 · 열 매핑 · 진행 · 결과.
    pub(crate) fn dump(&self) -> String {
        let so = self.source_opts();
        let mut out = format!(
            "file={} mode={} table={} format={} header={} delim={:?} enc={} sheet={:?} trim={} null_text={:?} inspecting={} running={} progress={:?} result={:?} report={}\n",
            self.path.display(),
            if self.is_new() { "new" } else { "existing" },
            if self.is_new() {
                self.new_table_name()
            } else {
                self.existing.clone().unwrap_or_default()
            },
            so.format.name(),
            so.header,
            so.delim.map(|d| d as char),
            so.encoding,
            so.sheet,
            so.trim,
            so.null_text,
            self.inspecting,
            self.running,
            self.progress,
            self.result
                .as_ref()
                .map(|(s, b)| format!("{}:{s}", if *b { "err" } else { "ok" })),
            self.last_report
        );
        if let Some(i) = &self.inspect {
            out.push_str(&format!(
                "inspect sampled={} truncated={} total={:?} enc={:?} delim={:?} sheet={:?} sheets={:?} preview_rows={}\n",
                i.sampled,
                i.truncated,
                i.total_hint,
                i.detected.encoding,
                i.detected.delim.map(|d| d as char),
                i.detected.sheet,
                i.detected.sheets,
                i.preview.rows.len()
            ));
        }
        if let Some(e) = &self.inspect_err {
            out.push_str(&format!("inspect_err={e}\n"));
        }
        out.push_str(&format!(
            "table_cols={:?}\n",
            self.table_cols.as_ref().map(Vec::len)
        ));
        for r in &self.rows {
            out.push_str(&format!(
                "col {} kind={} nullable={} on={} target={} new={} ty={}\n",
                r.src,
                r.kind,
                r.nullable,
                r.on.is_checked(),
                r.target.value(),
                r.new_name,
                r.ty.value()
            ));
        }
        out
    }

    /// 자체 시험(`import.set:<키>=<값>`): 창 옵션 하나를 바꾼다 — 원료 키면 참(호스트가 다시 살펴본다).
    pub(crate) fn set_option(&mut self, key: &str, val: &str) -> bool {
        let mut reinspect = false;
        let dialect = self.dialect;
        match key {
            "header" => {
                self.cb_header.set_checked(val != "off");
                reinspect = true;
            }
            "delim" => {
                self.cb_delim.select_value(val);
                reinspect = true;
            }
            "encoding" => {
                self.cb_enc.select_value(val);
                reinspect = true;
            }
            "sheet" => {
                self.cb_sheet.select_value(val);
                reinspect = true;
            }
            "format" => {
                self.cb_format.select_value(val);
                reinspect = true;
            }
            "trim" => {
                self.cb_trim.set_checked(val != "off");
                reinspect = true;
            }
            "null_text" => {
                self.tb_null.set_text(val);
                reinspect = true;
            }
            "target" => {
                self.rg_target.select_value(val);
                let _ = self.rg_target.take_changed();
                if val == "existing" && self.existing.is_none() {
                    self.existing = Some(self.tb_table.text().trim().to_string());
                }
            }
            "table" => {
                self.tb_table.set_text(val);
                if !self.is_new() {
                    self.existing = Some(val.to_string());
                }
            }
            "on_error" => {
                self.rg_on_error.select_value(val);
                let _ = self.rg_on_error.take_changed();
            }
            "batch" => self.tb_batch.set_text(val),
            "commit" => self.tb_commit.set_text(val),
            "empty_null" => self.cb_empty_null.set_checked(val != "off"),
            "skip" => {
                if let Some(r) = self.rows.iter_mut().find(|r| r.src == val) {
                    r.on.set_checked(false);
                }
            }
            "include" => {
                if let Some(r) = self.rows.iter_mut().find(|r| r.src == val) {
                    r.on.set_checked(true);
                }
            }
            "type" | "map" => {
                if let Some((src, v)) = val.split_once('=') {
                    if let Some(r) = self.rows.iter_mut().find(|r| r.src == src) {
                        if key == "type" {
                            let mut items = vec![ComboItem::new(v, v)];
                            items.extend(
                                nsql_io::infer::type_choices(dialect)
                                    .iter()
                                    .filter(|c| !c.eq_ignore_ascii_case(v))
                                    .map(|c| ComboItem::new(*c, *c)),
                            );
                            r.ty = Combo::new(items, 0);
                        } else {
                            r.target.select_value(v);
                        }
                    }
                }
            }
            _ => {}
        }
        self.sync_buttons();
        self.redraw();
        reinspect
    }

    /// 자체 시험: 시작 요청(버튼과 같은 길).
    pub(crate) fn start_spec(&self) -> ImportSpec {
        self.spec()
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        self.running = false;
        for b in [
            &mut self.btn_start,
            &mut self.btn_cancel,
            &mut self.btn_close,
            &mut self.btn_mode,
            &mut self.btn_ddl,
        ] {
            b.clear_transient();
        }
        for tb in self.text_boxes() {
            tb.set_focused(false);
        }
        self.rows.clear();
        self.inspect = None;
        self.grid.set_result(ResultSet::default());
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn text_boxes(&mut self) -> [&mut TextBox; 4] {
        [
            &mut self.tb_table,
            &mut self.tb_batch,
            &mut self.tb_commit,
            &mut self.tb_null,
        ]
    }

    /// 포커스 링은 하나: 텍스트박스 넷 중 하나만.
    fn focused_box(&mut self) -> Option<&mut TextBox> {
        self.text_boxes().into_iter().find(|tb| tb.is_focused())
    }

    fn own_focus(&mut self, p: Point) {
        let mut boxes = self.text_boxes();
        let hit = boxes.iter().position(|b| b.bounds().contains(p));
        for (i, tb) in boxes.iter_mut().enumerate() {
            tb.set_focused(hit == Some(i));
        }
    }

    /// 열린 콤보(팝업)가 있으면 그것 — 사건을 먼저 받는다.
    fn open_combo(&mut self) -> Option<&mut Combo> {
        if self.cb_format.is_open() {
            return Some(&mut self.cb_format);
        }
        if self.cb_sheet.is_open() {
            return Some(&mut self.cb_sheet);
        }
        if self.cb_delim.is_open() {
            return Some(&mut self.cb_delim);
        }
        if self.cb_enc.is_open() {
            return Some(&mut self.cb_enc);
        }
        self.rows.iter_mut().find_map(|r| {
            if r.target.is_open() {
                Some(&mut r.target)
            } else if r.ty.is_open() {
                Some(&mut r.ty)
            } else {
                None
            }
        })
    }

    fn all_combos(&mut self) -> Vec<&mut Combo> {
        let mut v: Vec<&mut Combo> = vec![
            &mut self.cb_format,
            &mut self.cb_sheet,
            &mut self.cb_delim,
            &mut self.cb_enc,
        ];
        for r in &mut self.rows {
            v.push(&mut r.target);
            v.push(&mut r.ty);
        }
        v
    }

    /// 콤보 변경 알림 수거 — 원료 콤보면 다시 살펴보기.
    fn take_source_changes(&mut self) -> bool {
        let mut changed = false;
        for c in [
            &mut self.cb_format,
            &mut self.cb_sheet,
            &mut self.cb_delim,
            &mut self.cb_enc,
        ] {
            if c.take_changed().is_some() {
                changed = true;
            }
        }
        for r in &mut self.rows {
            let _ = r.target.take_changed();
            let _ = r.ty.take_changed();
        }
        changed && !self.syncing
    }

    fn buttons(&mut self) -> [&mut Button; 5] {
        [
            &mut self.btn_start,
            &mut self.btn_cancel,
            &mut self.btn_close,
            &mut self.btn_mode,
            &mut self.btn_ddl,
        ]
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> ImportAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => {
                return if self.running {
                    ImportAction::Cancel
                } else {
                    ImportAction::Close
                };
            }
            WindowEvent::RedrawRequested => return ImportAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                for b in self.buttons() {
                    b.clear_transient();
                }
                self.redraw();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                if let Some(c) = self.open_combo() {
                    c.on_event(&mv, &mut inv);
                    self.redraw();
                    return ImportAction::None;
                }
                for b in self.buttons() {
                    b.on_event(&mv, &mut inv);
                }
                for cb in [
                    &mut self.cb_header,
                    &mut self.cb_trim,
                    &mut self.cb_empty_null,
                ] {
                    cb.on_event(&mv, &mut inv);
                }
                for tb in self.text_boxes() {
                    tb.on_event(&mv, &mut inv);
                }
                for c in self.all_combos() {
                    c.on_event(&mv, &mut inv);
                }
                for r in &mut self.rows {
                    r.on.on_event(&mv, &mut inv);
                }
                self.rg_target.on_event(&mv, &mut inv);
                self.rg_on_error.on_event(&mv, &mut inv);
                if self.grid.bounds.contains(Point { x, y }) {
                    self.grid.on_event(&mv, self.scale);
                }
                if !inv.is_empty() {
                    self.redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                let e = crate::input::wheel_event(delta, self.shift);
                if let Some(c) = self.open_combo() {
                    c.on_event(&e, &mut inv);
                } else if self.grid.bounds.contains(p) {
                    self.grid.on_event(&e, self.scale);
                } else if self.map_area.contains(p) {
                    if let InputEvent::Wheel { delta } = e {
                        let max =
                            ((self.rows.len() as i32) * self.row_h() - self.map_area.h).max(0);
                        self.map_scroll = (self.map_scroll - delta).clamp(0, max);
                    }
                }
                self.redraw();
            }
            WindowEvent::MouseInput { state, button, .. } if *button == MouseButton::Left => {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                let e = match state {
                    ElementState::Pressed => InputEvent::MouseDown {
                        x,
                        y,
                        shift: self.shift,
                        primary: self.primary,
                    },
                    ElementState::Released => InputEvent::MouseUp { x, y },
                };
                let up = matches!(e, InputEvent::MouseUp { .. });
                // 열린 콤보 = 모달: 그 콤보에만(팝업 밖 클릭은 콤보가 닫는다).
                if let Some(c) = self.open_combo() {
                    c.on_event(&e, &mut inv);
                    let reinspect = self.take_source_changes();
                    self.redraw();
                    return if reinspect {
                        ImportAction::Reinspect
                    } else {
                        ImportAction::None
                    };
                }
                if !up {
                    self.own_focus(p);
                }
                // 마우스 라우팅 규칙: 누름은 커서 아래 컨트롤에만 · 뗌은 전부.
                for tb in self.text_boxes() {
                    if up || tb.bounds().contains(p) {
                        tb.on_event(&e, &mut inv);
                    }
                }
                for c in self.all_combos() {
                    if up || c.bounds().contains(p) {
                        c.on_event(&e, &mut inv);
                    }
                }
                let mut source_changed = false;
                for cb in [&mut self.cb_header, &mut self.cb_trim] {
                    if up || cb.bounds().contains(p) {
                        cb.on_event(&e, &mut inv);
                        cb.set_focused(false);
                        if cb.take_toggled().is_some() {
                            source_changed = true;
                        }
                    }
                }
                if up || self.cb_empty_null.bounds().contains(p) {
                    self.cb_empty_null.on_event(&e, &mut inv);
                    self.cb_empty_null.set_focused(false);
                    let _ = self.cb_empty_null.take_toggled();
                }
                if self.map_area.contains(p) || up {
                    for r in &mut self.rows {
                        if up || r.on.bounds().contains(p) {
                            r.on.on_event(&e, &mut inv);
                            r.on.set_focused(false);
                            let _ = r.on.take_toggled();
                        }
                    }
                }
                for rg in [&mut self.rg_target, &mut self.rg_on_error] {
                    if up || rg.bounds().contains(p) {
                        rg.on_event(&e, &mut inv);
                    }
                }
                let _ = self.rg_on_error.take_changed();
                if self.rg_target.take_changed().is_some() {
                    self.sync_buttons();
                }
                if self.grid.bounds.contains(p) || up {
                    self.grid.on_event(&e, self.scale);
                }
                for b in self.buttons() {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    b.set_focused(false);
                }
                self.redraw();
                if self.btn_mode.take_clicked() {
                    self.mode_idx = (self.mode_idx + 1) % MODES.len();
                    self.sync_mode_label();
                    return ImportAction::None;
                }
                if self.btn_ddl.take_clicked() && self.is_new() {
                    return ImportAction::ShowDdl(self.ddl());
                }
                if self.btn_start.take_clicked() && !self.running && self.inspect.is_some() {
                    return ImportAction::Start(Box::new(self.spec()));
                }
                if self.btn_cancel.take_clicked() && self.running {
                    return ImportAction::Cancel;
                }
                if self.btn_close.take_clicked() && !self.running {
                    return ImportAction::Close;
                }
                if source_changed || self.take_source_changes() {
                    return ImportAction::Reinspect;
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if *button == MouseButton::Right && *state == ElementState::Pressed =>
            {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                for tb in self.text_boxes() {
                    if tb.bounds().contains(p) {
                        tb.on_event(&InputEvent::RightDown { x, y }, &mut inv);
                    }
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if let Some(tb) = self.focused_box() {
                    tb.set_preedit("", &mut inv);
                    for c in text.chars().filter(|c| !c.is_control()) {
                        tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                    }
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                if let Some(tb) = self.focused_box() {
                    tb.set_preedit(text, &mut inv);
                }
                self.redraw();
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                if matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) {
                    if let Some(c) = self.open_combo() {
                        c.on_event(
                            &InputEvent::Key {
                                key: nexa_ctl::Key::Escape,
                                shift: false,
                                primary: false,
                            },
                            &mut inv,
                        );
                        self.redraw();
                        return ImportAction::None;
                    }
                    return if self.running {
                        ImportAction::Cancel
                    } else {
                        ImportAction::Close
                    };
                }
                if let Key::Character(_) = kev.logical_key.as_ref() {
                    if self.primary && crate::input::shortcut_letter(kev) == Some('a') {
                        if let Some(tb) = self.focused_box() {
                            tb.on_event(&InputEvent::SelectAll, &mut inv);
                        }
                        self.redraw();
                        return ImportAction::None;
                    }
                }
                if let Some(e) = crate::input::text_key_event(
                    kev,
                    self.shift,
                    self.primary,
                    crate::input::TextKeys::Multi,
                ) {
                    let null_focus = self.tb_null.is_focused();
                    let enter = matches!(
                        e,
                        InputEvent::Key {
                            key: nexa_ctl::Key::Enter,
                            ..
                        }
                    );
                    if let Some(tb) = self.focused_box() {
                        tb.on_event(&e, &mut inv);
                    }
                    self.redraw();
                    // NULL 글 상자는 Enter로 적용(살펴보기 다시).
                    if null_focus && enter {
                        return ImportAction::Reinspect;
                    }
                }
            }
            _ => {}
        }
        ImportAction::None
    }

    fn row_h(&self) -> i32 {
        (26.0 * self.scale).round() as i32
    }

    pub(crate) fn paint(
        &mut self,
        font: &Font,
        mono_font: &Font,
        th: &Theme,
        ui_px: f32,
        mono_px: f32,
    ) {
        let Some(win) = self.window.clone() else {
            return;
        };
        let Some(mut surface) = self.surface.take() else {
            return;
        };
        let size = win.inner_size();
        let Some(mut buf) = surface.frame(size) else {
            self.surface = Some(surface);
            return;
        };
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let px = |v: f32| (v * s).round() as i32;
        let inv = &mut Invalidations::default();
        let is_new = self.is_new();
        let has_sheets = !self.sheets.is_empty();
        let fmt = SourceFormat::parse(&self.cb_format.value()).unwrap_or_default();
        let fmt_eff = if fmt == SourceFormat::Auto {
            SourceFormat::Auto.resolve(&self.path)
        } else {
            fmt
        };
        let delim_opts = matches!(fmt_eff, SourceFormat::Csv | SourceFormat::Tsv);
        let text_opts = fmt_eff.is_text();
        let grid_rect;
        let body;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base_status(ui_px);
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.panel_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            let pad = px(12.0);
            let gap = px(8.0);
            let clip = Rect::new(0, 0, wi, hi);
            let row_h = th_txt + px(12.0);
            let mut y = pad;
            // ── A1: 파일 · 형식 · 시트
            let fmt_w = px(190.0);
            let sheet_w = if has_sheets { px(150.0) } else { 0 };
            let fmt_label = t(Msg::ImpFormat);
            let sheet_label = t(Msg::ImpSheet);
            let right_w = dc.text_width(fmt_label)
                + gap
                + fmt_w
                + if has_sheets {
                    gap * 2 + dc.text_width(sheet_label) + gap + sheet_w
                } else {
                    0
                };
            let file_text = format!("{}: {}", t(Msg::ImpFile), self.path.display());
            let file_w = (wi - pad * 2 - right_w - gap * 2).max(px(80.0));
            let file_text = nexa_ctl::draw::ellipsize_middle(&mut dc, &file_text, file_w);
            dc.text(pad, y + (row_h - th_txt) / 2, clip, &file_text, th.text_dim);
            let mut x = wi - pad - right_w;
            dc.text(x, y + (row_h - th_txt) / 2, clip, fmt_label, th.text);
            x += dc.text_width(fmt_label) + gap;
            self.cb_format.set_scale(s);
            self.cb_format
                .set_bounds(Rect::new(x, y, fmt_w, row_h), inv);
            self.cb_format.set_viewport_bottom(hi);
            x += fmt_w + gap * 2;
            if has_sheets {
                dc.text(x, y + (row_h - th_txt) / 2, clip, sheet_label, th.text);
                x += dc.text_width(sheet_label) + gap;
                self.cb_sheet.set_scale(s);
                self.cb_sheet
                    .set_bounds(Rect::new(x, y, sheet_w, row_h), inv);
                self.cb_sheet.set_viewport_bottom(hi);
            } else {
                self.cb_sheet.set_bounds(OFF, inv);
            }
            y += row_h + gap;
            // ── A2: 헤더 · 구분자 · 인코딩 · 다듬기 · NULL 글
            let mut x = pad;
            self.cb_header.set_scale(s);
            let cbw = px(24.0) + dc.text_width(t(Msg::ImpHeader)) + px(12.0);
            self.cb_header.set_bounds(Rect::new(x, y, cbw, row_h), inv);
            self.cb_header.set_enabled(matches!(
                fmt_eff,
                SourceFormat::Csv | SourceFormat::Tsv | SourceFormat::Xlsx
            ));
            self.cb_header.paint(&mut dc, th);
            x += cbw + gap * 2;
            if delim_opts {
                let l = t(Msg::ImpDelim);
                dc.text(x, y + (row_h - th_txt) / 2, clip, l, th.text);
                x += dc.text_width(l) + gap;
                self.cb_delim.set_scale(s);
                self.cb_delim
                    .set_bounds(Rect::new(x, y, px(96.0), row_h), inv);
                self.cb_delim.set_viewport_bottom(hi);
                x += px(96.0) + gap * 2;
            } else {
                self.cb_delim.set_bounds(OFF, inv);
            }
            if text_opts {
                let l = t(Msg::ImpEncoding);
                dc.text(x, y + (row_h - th_txt) / 2, clip, l, th.text);
                x += dc.text_width(l) + gap;
                self.cb_enc.set_scale(s);
                self.cb_enc
                    .set_bounds(Rect::new(x, y, px(130.0), row_h), inv);
                self.cb_enc.set_viewport_bottom(hi);
                x += px(130.0) + gap * 2;
            } else {
                self.cb_enc.set_bounds(OFF, inv);
            }
            self.cb_trim.set_scale(s);
            let tw = px(24.0) + dc.text_width(t(Msg::ImpTrim)) + px(12.0);
            self.cb_trim.set_bounds(Rect::new(x, y, tw, row_h), inv);
            self.cb_trim.paint(&mut dc, th);
            x += tw + gap * 2;
            let l = t(Msg::ImpNullText);
            dc.text(x, y + (row_h - th_txt) / 2, clip, l, th.text);
            x += dc.text_width(l) + gap;
            self.tb_null.set_scale(s);
            let nw = px(90.0).min((wi - pad - x).max(px(40.0)));
            self.tb_null.set_bounds(Rect::new(x, y, nw, row_h), inv);
            self.tb_null.paint(&mut dc, th);
            y += row_h + gap;
            // ── 아래부터 위로: 버튼 행 · 상태 줄 · 옵션 행 · 대상 행 → 남는 높이를 미리보기 : 매핑 = 2 : 3.
            let btn_h = th_txt + px(14.0);
            let btn_y = hi - pad - btn_h;
            let status_y = btn_y - gap - th_txt;
            let opts_y = status_y - gap - row_h;
            let target_y = opts_y - gap - row_h;
            let map_bottom = target_y - gap;
            let avail = (map_bottom - y - (th_txt + px(4.0)) * 2 - gap).max(px(120.0));
            let grid_h = (avail * 2 / 5).max(px(60.0));
            // ── B 미리보기 제목 + 그리드
            let title = match (&self.inspect, &self.inspect_err, self.inspecting) {
                (_, _, true) => t(Msg::ImpInspecting).to_string(),
                (_, Some(e), _) => tf(Msg::ImpInspectErr, &[e]),
                (Some(i), _, _) => tf(
                    Msg::ImpPreviewTitle,
                    &[
                        &i.preview.rows.len().to_string(),
                        &format!("{}{}", i.sampled, if i.truncated { "+" } else { "" }),
                        &i.total_hint.map_or("?".to_string(), |n| n.to_string()),
                    ],
                ),
                _ => String::new(),
            };
            let title = nexa_ctl::draw::ellipsize_middle(&mut dc, &title, wi - pad * 2);
            dc.text(
                pad,
                y,
                clip,
                &title,
                if self.inspect_err.is_some() {
                    th.danger
                } else {
                    th.text_dim
                },
            );
            y += th_txt + px(4.0);
            grid_rect = Rect::new(pad, y, wi - pad * 2, grid_h);
            y += grid_h + gap;
            // ── 매핑 제목 + 표(스크롤)
            let map_title = if is_new {
                t(Msg::ImpMapTitleNew)
            } else {
                t(Msg::ImpMapTitle)
            };
            dc.text(pad, y, clip, map_title, th.text_dim);
            y += th_txt + px(4.0);
            let area = Rect::new(pad, y, wi - pad * 2, (map_bottom - y).max(px(40.0)));
            self.map_area = area;
            dc.fill_rect(area, th.field_bg);
            dc.stroke_round_rect(area, 0, th.border, 1.0);
            let rh = self.row_h();
            let col_on = px(28.0);
            let col_src = ((area.w - col_on) * 22 / 100).max(px(80.0));
            let col_kind = px(110.0);
            let col_sel = ((area.w - col_on - col_src - col_kind) * 55 / 100).max(px(120.0));
            // 머리줄
            dc.fill_rect(Rect::new(area.x, area.y, area.w, rh), th.panel_bg);
            let heads = [
                (area.x + col_on, t(Msg::ImpMapCol)),
                (area.x + col_on + col_src, t(Msg::ImpMapKind)),
                (
                    area.x + col_on + col_src + col_kind,
                    if is_new {
                        t(Msg::ImpMapType)
                    } else {
                        t(Msg::ImpMapTarget)
                    },
                ),
                (
                    area.x + col_on + col_src + col_kind + col_sel,
                    t(Msg::ImpMapSample),
                ),
            ];
            for (x, l) in heads {
                dc.text(
                    x + px(6.0),
                    area.y + (rh - th_txt) / 2,
                    area,
                    l,
                    th.text_dim,
                );
            }
            body = Rect::new(area.x, area.y + rh, area.w, area.h - rh);
            let max_scroll = ((self.rows.len() as i32) * rh - body.h).max(0);
            self.map_scroll = self.map_scroll.clamp(0, max_scroll);
            let mut ry = body.y - self.map_scroll;
            for r in &mut self.rows {
                let visible = ry + rh > body.y && ry < body.bottom();
                let rowrect = Rect::new(body.x, ry, body.w, rh).intersection(&body);
                if visible {
                    r.on.set_scale(s);
                    r.on.set_bounds(Rect::new(body.x + px(4.0), ry, col_on, rh), inv);
                    r.on.paint(&mut dc, th);
                    let dim = !r.on.is_checked();
                    let tc = if dim { th.text_dim } else { th.text };
                    let src = nexa_ctl::draw::ellipsize_middle(&mut dc, &r.src, col_src - px(10.0));
                    dc.text(
                        body.x + col_on + px(6.0),
                        ry + (rh - th_txt) / 2,
                        rowrect,
                        &src,
                        tc,
                    );
                    let kind = format!("{}{}", r.kind, if r.nullable { "" } else { " ·NN" });
                    dc.text(
                        body.x + col_on + col_src + px(6.0),
                        ry + (rh - th_txt) / 2,
                        rowrect,
                        &kind,
                        tc,
                    );
                    let sel_x = body.x + col_on + col_src + col_kind + px(4.0);
                    let sel = Rect::new(sel_x, ry + px(2.0), col_sel - px(8.0), rh - px(4.0));
                    let (combo, other) = if is_new {
                        (&mut r.ty, &mut r.target)
                    } else {
                        (&mut r.target, &mut r.ty)
                    };
                    combo.set_scale(s);
                    combo.set_bounds(sel, inv);
                    combo.set_viewport_bottom(hi);
                    combo.set_enabled(!dim);
                    other.set_bounds(OFF, inv);
                    let sx = body.x + col_on + col_src + col_kind + col_sel + px(6.0);
                    let sample = nexa_ctl::draw::ellipsize_middle(
                        &mut dc,
                        &r.sample,
                        body.right() - sx - px(6.0),
                    );
                    dc.text(sx, ry + (rh - th_txt) / 2, rowrect, &sample, th.text_dim);
                } else {
                    r.on.set_bounds(OFF, inv);
                    r.target.set_bounds(OFF, inv);
                    r.ty.set_bounds(OFF, inv);
                }
                ry += rh;
            }
            if self.rows.is_empty() {
                let hint = if self.inspecting {
                    t(Msg::ImpInspecting)
                } else {
                    t(Msg::ImpNoColumns)
                };
                dc.text(body.x + px(8.0), body.y + px(8.0), body, hint, th.text_dim);
            }
            // ── 대상 행
            let ty = target_y;
            self.rg_target.set_scale(s);
            self.rg_target.set_horizontal(None);
            let rw = self.rg_target.preferred_width().max(px(220.0));
            self.rg_target
                .set_bounds(Rect::new(pad, ty, rw, row_h), inv);
            self.rg_target.paint(&mut dc, th);
            let mut x = pad + rw + gap * 2;
            let l = t(Msg::ImpTableName);
            dc.text(x, ty + (row_h - th_txt) / 2, clip, l, th.text);
            x += dc.text_width(l) + gap;
            self.tb_table.set_scale(s);
            self.tb_table.set_read_only(!is_new);
            let tb_w = px(280.0).min(wi - pad - x - px(120.0)).max(px(100.0));
            self.tb_table.set_bounds(Rect::new(x, ty, tb_w, row_h), inv);
            self.tb_table.paint(&mut dc, th);
            x += tb_w + gap;
            self.btn_ddl.set_scale(s);
            let dw = dc.text_width(t(Msg::ImpShowDdl)) + px(28.0);
            self.btn_ddl.set_bounds(Rect::new(x, ty, dw, row_h), inv);
            self.btn_ddl.set_enabled(is_new && self.inspect.is_some());
            self.btn_ddl.paint(&mut dc, th);
            // ── 옵션 행
            let oy = opts_y;
            let mut x = pad;
            for (label, tb) in [
                (t(Msg::ImpBatch), &mut self.tb_batch),
                (t(Msg::ImpCommit), &mut self.tb_commit),
            ] {
                dc.text(x, oy + (row_h - th_txt) / 2, clip, label, th.text);
                x += dc.text_width(label) + gap;
                tb.set_scale(s);
                tb.set_bounds(Rect::new(x, oy, px(80.0), row_h), inv);
                tb.paint(&mut dc, th);
                x += px(80.0) + gap * 2;
            }
            self.btn_mode.set_scale(s);
            let mw = dc.text_width(&self.mode_label) + px(28.0);
            self.btn_mode.set_bounds(Rect::new(x, oy, mw, row_h), inv);
            self.btn_mode.paint(&mut dc, th);
            x += mw + gap * 2;
            self.cb_empty_null.set_scale(s);
            let ew = px(24.0) + dc.text_width(t(Msg::ImpEmptyNull)) + px(12.0);
            self.cb_empty_null
                .set_bounds(Rect::new(x, oy, ew, row_h), inv);
            self.cb_empty_null.paint(&mut dc, th);
            x += ew + gap * 2;
            let l = t(Msg::ImpOnError);
            dc.text(x, oy + (row_h - th_txt) / 2, clip, l, th.text);
            x += dc.text_width(l) + gap;
            self.rg_on_error.set_scale(s);
            self.rg_on_error.set_horizontal(None);
            let ow = self
                .rg_on_error
                .preferred_width()
                .max(px(200.0))
                .min((wi - pad - x).max(px(100.0)));
            self.rg_on_error
                .set_bounds(Rect::new(x, oy, ow, row_h), inv);
            self.rg_on_error.paint(&mut dc, th);
            // ── 상태 줄 + 버튼
            let line = match (&self.result, &self.progress) {
                (Some((text, bad)), _) => (text.clone(), if *bad { th.danger } else { th.ok }),
                (None, Some((rows, secs))) => (
                    tf(Msg::ImpRunning, &[&rows.to_string(), &format!("{secs:.1}")]),
                    th.accent,
                ),
                _ => (t(Msg::ImpHint2).to_string(), th.text_dim),
            };
            let line_text = nexa_ctl::draw::ellipsize_middle(&mut dc, &line.0, wi - pad * 2);
            dc.text(pad, status_y, clip, &line_text, line.1);
            let mut bx = pad;
            for (b, label) in [
                (&mut self.btn_start, Msg::ImpBtnStart),
                (&mut self.btn_cancel, Msg::ImpBtnCancel),
                (&mut self.btn_close, Msg::ImpBtnClose),
            ] {
                b.set_scale(s);
                let bw = (dc.text_width(t(label)) + px(28.0)).max(px(84.0));
                b.set_bounds(Rect::new(bx, btn_y, bw, btn_h), inv);
                b.paint(&mut dc, th);
                bx += bw + gap;
            }
            // 콤보(팝업이 위에 오도록 마지막) — 보이는 것만.
            self.cb_format.paint(&mut dc, th);
            if has_sheets {
                self.cb_sheet.paint(&mut dc, th);
            }
            if delim_opts {
                self.cb_delim.paint(&mut dc, th);
            }
            if text_opts {
                self.cb_enc.paint(&mut dc, th);
            }
            for r in &self.rows {
                let c = if is_new { &r.ty } else { &r.target };
                if c.bounds().y > 0 && c.bounds().intersects(&body) {
                    c.paint(&mut dc, th);
                }
            }
        }
        // 미리보기 그리드(고정폭 얼굴 · 결과 그리드와 같은 크기).
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base(mono_px);
            let fonts = FontSet {
                mono: Some(mono_font),
                ..FontSet::single(mono_font)
            };
            let mut dc = RasterCtx::with_font_set(&mut gfx, fonts, s).with_fonts(prefs);
            self.grid.set_bounds(grid_rect);
            self.grid.paint(&mut dc, th, s);
        }
        // 열린 콤보 팝업은 맨 위에 다시.
        if let Some(c) = self.open_combo() {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base_status(ui_px);
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            c.paint(&mut dc, th);
        }
        for tb in [
            &self.tb_table,
            &self.tb_batch,
            &self.tb_commit,
            &self.tb_null,
        ] {
            if tb.popup_open() {
                let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
                let prefs = FontPrefs::with_base(ui_px);
                let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
                tb.paint_popup(&mut dc, th);
            }
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
