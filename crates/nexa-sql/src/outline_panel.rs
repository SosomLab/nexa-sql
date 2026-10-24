//! **아웃라인 패널**(활동 막대 · docs/76 §20 · 사용자 09-23 "현재 문서 기반 Outline" · ★ 10-24 T-422 D-357 "너무 많이 보임") —
//! 활성 탭의 심볼을 **트리**로 보인다. 원칙 = "선언된 이름으로 이동하는 지도": 기본으로 보이는 것은 **심볼**(CREATE 대상 ·
//! 루틴 · CTE · DEFINE/VARIABLE · 커서 · 라벨 · 선언 변수)이고 **문장**(SELECT · DML · DDL · 명령/블록/기타)은 필터 틀의 토글 4개
//! (DataGrip Structure의 Show DDL/DML/Select/Other)로 켤 때만 보인다 · 토글 값 = 설정 `outline.show_*`(창마다 아님).
//! - 트리 = `Symbol.parent`(nsql-script · 평면 Vec + 부모 색인) · 종류로 숨은 조상은 건너뛰어 자식이 한 단 올라온다 · 접기 =
//!   기본 펼침 깊이 `outline.expand_depth`(1 = 뿌리 펼침 · 그 안의 선언은 접힘) + 사용자 토글(이름·종류 열쇠 · 본문이 바뀌어도 유지) ·
//!   머리 오른쪽 [모두 펼침] [모두 접음] · 키 ←/→ · 셰브론 클릭. 필터 중에는 일치 행과 그 조상을 접힘 무시하고 보인다.
//! - 커서 추종(`outline.follow_caret`) = 캐럿이 든 심볼(글자 색인 이진 탐색 1회 · 캐럿이 바뀐 틱만)을 선택·드러냄 · 패널에 포커스가
//!   있는 동안은 추종하지 않는다(키 이동과 겹치지 않게) · 조상이 접혀 있으면 펼친다.
//! - 아이콘 = 오브젝트 탐색기 것 재사용(`exp_icons` · 새 아이콘 0) · 줄 번호 · 오른쪽 종류/타입 · 머리 개수 = 보이는 수/전체.
//! - 클릭/Enter = 그 자리로 · 필터 틀(`FilterBar` · 한글 자모) · 타입어헤드. 데이터는 호스트가 `set_symbols`로 준다(탭별 캐시 =
//!   `intel::Intel::outline_for` · 세대 열쇠).

use std::collections::HashMap;
use std::rc::Rc;

use crate::exp_icons::{self, IconKind};
use crate::filterbar::{FilterBar, FilterEvent, GAP_Y};
use crate::findbar::BtnKind;
use crate::toolicons;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::{IconImage, Theme};
use nexa_ctl::{InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox};
use nsql_i18n::{t, tf, Msg};
use nsql_script::outline::{Outline, SymKind};
use nsql_settings::Settings;

/// 아웃라인 표시 설정(`outline.*` · docs/94 §6-1 · 호스트가 `outline_cfg`로 읽어 `set_cfg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutlineCfg {
    /// 문장 행 토글 4(DataGrip Show …) — 기본 전부 끔.
    pub show_select: bool,
    pub show_dml: bool,
    pub show_ddl: bool,
    /// 명령(EXEC·PRINT·SET…) · 익명 블록 · 그 밖 SQL = "기타".
    pub show_other: bool,
    /// 선언 변수(PL/SQL 선언부 · T-SQL `DECLARE @v`) — 기본 켬.
    pub show_declared: bool,
    /// 기본 펼침 깊이(0 = 전부 접힘 · 1 = 뿌리만 펼침 · …).
    pub expand_depth: u8,
    pub follow_caret: bool,
}

impl Default for OutlineCfg {
    fn default() -> Self {
        OutlineCfg {
            show_select: false,
            show_dml: false,
            show_ddl: false,
            show_other: false,
            show_declared: true,
            expand_depth: 1,
            follow_caret: true,
        }
    }
}

/// 설정 레지스트리 → 표시 설정(기동 · `outline.*` 변경 때).
pub(crate) fn outline_cfg(s: &Settings) -> OutlineCfg {
    OutlineCfg {
        show_select: s.flag("outline.show_select"),
        show_dml: s.flag("outline.show_dml"),
        show_ddl: s.flag("outline.show_ddl"),
        show_other: s.flag("outline.show_other"),
        show_declared: s.flag("outline.show_declared"),
        expand_depth: s.int("outline.expand_depth").clamp(0, 8) as u8,
        follow_caret: s.flag("outline.follow_caret"),
    }
}

/// 토글 종류 ↔ 설정 키(패널 토글 클릭 = 호스트가 그 키를 뒤집는다).
pub(crate) fn toggle_key(k: BtnKind) -> Option<&'static str> {
    Some(match k {
        BtnKind::OlSelect => "outline.show_select",
        BtnKind::OlDml => "outline.show_dml",
        BtnKind::OlDdl => "outline.show_ddl",
        BtnKind::OlOther => "outline.show_other",
        _ => return None,
    })
}

/// 종류 토글 판정(순수 · D-357 결정 1): 문장은 세부 종류의 토글 · 선언 변수는 `show_declared` · 그 밖 심볼은 늘 보인다.
pub(crate) fn kind_visible(cfg: &OutlineCfg, kind: SymKind, detail: &str) -> bool {
    match kind {
        SymKind::Statement => match detail {
            "select" => cfg.show_select,
            "dml" => cfg.show_dml,
            "ddl" => cfg.show_ddl,
            _ => cfg.show_other,
        },
        SymKind::Declared => cfg.show_declared,
        _ => true,
    }
}

/// 보이는 트리의 노드 — 종류로 숨은 조상은 건너뛴다(그 자식은 한 단 올라온다).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VisNode {
    /// `Outline::symbols` 색인.
    pub idx: usize,
    /// 유효 깊이(보이는 조상 수).
    pub depth: u8,
    /// 유효 부모(보이는 가장 가까운 조상 · 심볼 색인).
    pub parent: Option<usize>,
}

/// 종류 토글을 적용한 보이는 노드 목록(문서 순서 · 부모가 자식보다 먼저 · Goto Symbol과 패널 공용).
pub(crate) fn visible_tree(cfg: &OutlineCfg, ol: &Outline) -> Vec<VisNode> {
    let n = ol.symbols.len();
    let mut vis_parent: Vec<Option<usize>> = vec![None; n];
    let mut depth: Vec<u8> = vec![0; n];
    let mut shown: Vec<bool> = vec![false; n];
    let mut out = Vec::with_capacity(n);
    for (i, s) in ol.symbols.iter().enumerate() {
        let vp = match s.parent {
            Some(p) if p < i => {
                if shown[p] {
                    Some(p)
                } else {
                    vis_parent[p]
                }
            }
            _ => None,
        };
        vis_parent[i] = vp;
        depth[i] = vp.map_or(0, |p| depth[p].saturating_add(1));
        shown[i] = kind_visible(cfg, s.kind, &s.detail);
        if shown[i] {
            out.push(VisNode {
                idx: i,
                depth: depth[i],
                parent: vp,
            });
        }
    }
    out
}

/// 접기 적용(필터 없음): 부모가 보이고 펼쳐져 있을 때만 자식이 보인다 → 보이는 심볼 색인(문서 순서).
fn fold_rows(vis: &[VisNode], expanded: &dyn Fn(usize, u8) -> bool) -> Vec<usize> {
    let mut shown: HashMap<usize, bool> = HashMap::with_capacity(vis.len());
    let mut depth_of: HashMap<usize, u8> = HashMap::with_capacity(vis.len());
    let mut out = Vec::with_capacity(vis.len());
    for v in vis {
        let ok = match v.parent {
            None => true,
            Some(p) => shown.get(&p).copied().unwrap_or(false) && expanded(p, depth_of[&p]),
        };
        shown.insert(v.idx, ok);
        depth_of.insert(v.idx, v.depth);
        if ok {
            out.push(v.idx);
        }
    }
    out
}

/// 심볼 종류 → 탐색기 아이콘(새 아이콘 0 · 문장 행은 없음).
fn icon_kind(kind: SymKind, detail: &str) -> Option<IconKind> {
    Some(match kind {
        SymKind::Statement => return None,
        SymKind::Object => match detail {
            "view" => IconKind::View,
            "index" => IconKind::Index,
            "sequence" => IconKind::Sequence,
            "synonym" => IconKind::Synonym,
            "schema" | "database" => IconKind::Schema,
            _ => IconKind::Table,
        },
        SymKind::Procedure => IconKind::Procedure,
        SymKind::Function => IconKind::Function,
        SymKind::Package => IconKind::Package,
        SymKind::PackageBody => IconKind::PackageBody,
        SymKind::Trigger => IconKind::Trigger,
        SymKind::Type => IconKind::Type,
        SymKind::Cte => IconKind::View,
        SymKind::Cursor => IconKind::Queue,
        SymKind::Label => IconKind::Property,
        SymKind::Define | SymKind::Variable | SymKind::Declared => IconKind::Column,
    })
}

#[derive(Clone, Debug)]
struct Row {
    name: String,
    detail: String,
    kind: SymKind,
    line: usize,
    byte: usize,
    /// 글자 색인(캐럿 비교용 · `set_symbols`가 본문 한 번 걷어 채움).
    char_idx: usize,
}

/// 접기 상태 열쇠 — 심볼 색인은 편집마다 바뀌므로 이름·종류로 기억한다.
type FoldKey = (String, SymKind);

pub(crate) struct OutlinePanel {
    visible: bool,
    bounds: Rect,
    scale: f32,
    filter: FilterBar,
    filter_text: String,
    cfg: OutlineCfg,
    /// 심볼 전부(문서 순서).
    all: Vec<Row>,
    /// 종류 토글을 적용한 트리(`visible_tree`).
    vis: Vec<VisNode>,
    /// 심볼 색인 → 유효 깊이 · 유효 부모 · 보이는 자식 유무(종류 토글 기준).
    eff_depth: Vec<u8>,
    eff_parent: Vec<Option<usize>>,
    has_child: Vec<bool>,
    kind_shown: Vec<bool>,
    /// 글자 색인 순 심볼 색인(캐럿 추종 이진 탐색).
    by_char: Vec<usize>,
    /// 원래 부모(nsql-script) — 설정만 바뀌었을 때 종류 트리를 다시 만드는 재료.
    orig_parent: Vec<Option<usize>>,
    /// 사용자가 바꾼 접기(없으면 깊이 규칙).
    fold: HashMap<FoldKey, bool>,
    /// 지금 보이는 행 = 심볼 색인.
    rows: Vec<usize>,
    /// 지금 보이는 심볼의 열쇠(탭 id · 본문 세대) — 호스트가 바뀌었을 때만 다시 준다.
    key: Option<(u64, u64)>,
    scroll_y: i32,
    bars: ScrollBars,
    sel: Option<usize>,
    hover: Option<usize>,
    list_rect: Rect,
    header_rect: Rect,
    /// 머리 오른쪽 버튼 [모두 펼침] [모두 접음].
    hdr_btn: [Rect; 2],
    hdr_hover: Option<usize>,
    row_h: i32,
    focused: bool,
    clamp_w: i32,
    open: Option<usize>,
    /// 토글 클릭 → 호스트가 설정을 뒤집는다(1회성).
    pending_toggle: Option<BtnKind>,
    /// 마지막으로 받은 캐럿(글자 색인) — 같으면 아무 일도 없다.
    caret: Option<usize>,
    icon_cache: HashMap<(IconKind, i32), Rc<IconImage>>,
    /// ★ 타입어헤드(사용자 09-28 "아웃라인에도") — nexa-ctl 부품 · 설정 `explorer.typeahead*` 공유.
    typeahead: nexa_ctl::TypeAhead,
    ta_cfg: crate::explorer::TypeAheadCfg,
    now_hint: u64,
}

const ROW_H: f32 = 22.0;
const INPUT_H: f32 = 25.0;
const PAD: f32 = 8.0;
const INDENT: f32 = 14.0;
/// 셰브론 칸 · 아이콘 칸(논리 px).
const CHEV_W: f32 = 16.0;
const ICON_W: f32 = 18.0;
const ICON_PX: f32 = 16.0;

impl OutlinePanel {
    /// 필터 상자 팝업(편집 메뉴 · 이력)이 열려 있는가 — 우클릭 메뉴 배타 규칙(10-13).
    pub(crate) fn menu_open(&self) -> bool {
        self.filter.popup_open()
    }
    pub(crate) fn menu_bounds(&self) -> nexa_ctl::geom::Rect {
        self.filter.popup_bounds()
    }
    pub(crate) fn close_menu(&mut self) {
        self.filter.close_popups();
    }

    /// 언어 전환 뒤 재라벨(플레이스홀더).
    pub(crate) fn relabel(&mut self) {
        self.filter.set_placeholder(t(Msg::PhOutlineFilter));
    }

    pub(crate) fn new() -> Self {
        // 틀 오른쪽 토글 4 = 문장 종류(Select · DML · DDL · 기타) — 기본 끔 · 값은 설정(호스트가 `set_cfg`로 맞춘다).
        let side: [(BtnKind, fn() -> nexa_ctl::controls::ctxmenu::MenuIcon); 4] = [
            (BtnKind::OlSelect, toolicons::mi_ol_select),
            (BtnKind::OlDml, toolicons::mi_ol_dml),
            (BtnKind::OlDdl, toolicons::mi_ol_ddl),
            (BtnKind::OlOther, toolicons::mi_ol_other),
        ];
        OutlinePanel {
            visible: false,
            bounds: Rect::default(),
            scale: 1.0,
            filter: FilterBar::new(t(Msg::PhOutlineFilter), &side),
            filter_text: String::new(),
            cfg: OutlineCfg::default(),
            all: Vec::new(),
            vis: Vec::new(),
            eff_depth: Vec::new(),
            eff_parent: Vec::new(),
            has_child: Vec::new(),
            kind_shown: Vec::new(),
            by_char: Vec::new(),
            orig_parent: Vec::new(),
            fold: HashMap::new(),
            rows: Vec::new(),
            typeahead: nexa_ctl::TypeAhead::default(),
            ta_cfg: crate::explorer::TypeAheadCfg::default(),
            now_hint: 0,
            key: None,
            scroll_y: 0,
            bars: ScrollBars::new(),
            sel: None,
            hover: None,
            list_rect: Rect::default(),
            header_rect: Rect::default(),
            hdr_btn: [Rect::default(), Rect::default()],
            hdr_hover: None,
            row_h: 22,
            focused: false,
            clamp_w: i32::MAX / 2,
            open: None,
            pending_toggle: None,
            caret: None,
            icon_cache: HashMap::new(),
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    /// 끌어 놓기 대상 = 아웃라인 필터 상자(10-12 · 보일 때만).
    pub(crate) fn drop_box_at(&self, p: Point) -> Option<Rect> {
        self.visible.then(|| self.filter.drop_box_at(p)).flatten()
    }

    pub(crate) fn drop_insert(&mut self, p: Point, text: &str) -> bool {
        self.visible && self.filter.drop_insert(p, text)
    }

    /// 필터 검색어 이력 잇기(전역 · `filter.outline` · 사용자 09-23).
    pub(crate) fn set_history(&mut self, h: crate::search_history::SharedHistory) {
        self.filter.set_history(h, "filter.outline");
    }

    /// 팝업 층(필터 편집 메뉴 · 이력 드롭다운 · 툴팁) — 창의 맨 마지막에.
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.visible {
            self.filter.paint_popup(dc, th);
        }
    }

    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
        if !on {
            self.set_focused(false);
        }
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        if !on {
            self.filter.set_focused(false);
        }
    }

    pub(crate) fn focus_filter(&mut self) {
        self.filter.set_focused(true);
    }

    /// 글 입력 상자(필터)가 포커스인가 — 호스트의 IME 허용 판정(아니면 패널 = 타입어헤드 대상 · IME 끊음 · 09-28).
    pub(crate) fn wants_ime(&self) -> bool {
        self.filter.is_focused()
    }

    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.filter.is_focused() {
            Some(self.filter.tb_mut())
        } else {
            None
        }
    }

    pub(crate) fn bounds(&self) -> Rect {
        if self.visible {
            self.bounds
        } else {
            Rect::default()
        }
    }

    pub(crate) fn set_clamp_width(&mut self, w: i32) {
        self.clamp_w = w;
        self.filter.set_clamp_width(w);
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        let px = |v: f32| (v * scale).round() as i32;
        let pad = px(PAD);
        let ih = px(INPUT_H);
        self.row_h = px(ROW_H);
        self.header_rect = Rect::new(b.x, b.y + px(4.0), b.w, self.row_h);
        // 머리 오른쪽 버튼 둘(모두 펼침 · 모두 접음) — 한 행 높이의 정사각형.
        let bw = self.row_h;
        let hr = self.header_rect;
        self.hdr_btn = [
            Rect::new(hr.right() - pad - bw * 2 - px(2.0), hr.y, bw, bw),
            Rect::new(hr.right() - pad - bw, hr.y, bw, bw),
        ];
        let y1 = self.header_rect.bottom() + px(GAP_Y);
        self.filter.set_bounds(
            Rect::new(b.x + pad, y1, (b.w - pad * 2).max(px(80.0)), ih),
            scale,
        );
        let list_top = y1 + ih + px(GAP_Y);
        // 오른쪽 테두리 한 줄(`b.right() - 1`)은 목록 밖 — 머리·선택 행 배경이 스플리터 선을 덮지 않게(사용자 10-13 · 패널 공통).
        self.list_rect = Rect::new(
            b.x,
            list_top,
            (b.w - 1).max(0),
            (b.bottom() - list_top).max(0),
        );
        self.clamp_scroll();
    }

    /// 지금 보이는 심볼의 열쇠(탭 · 세대) — 호스트가 비교해 바뀐 문서만 다시 준다.
    pub(crate) fn key(&self) -> Option<(u64, u64)> {
        self.key
    }

    /// 표시 설정(기동 · `outline.*` 변경) — 토글 표시를 맞추고 다시 거른다.
    pub(crate) fn set_cfg(&mut self, cfg: OutlineCfg) {
        if self.cfg == cfg {
            return;
        }
        self.cfg = cfg;
        self.filter.set_checked(BtnKind::OlSelect, cfg.show_select);
        self.filter.set_checked(BtnKind::OlDml, cfg.show_dml);
        self.filter.set_checked(BtnKind::OlDdl, cfg.show_ddl);
        self.filter.set_checked(BtnKind::OlOther, cfg.show_other);
        self.retree();
    }

    pub(crate) fn cfg(&self) -> OutlineCfg {
        self.cfg
    }

    /// 심볼 전체 교체(활성 탭이 바뀌거나 본문이 바뀌었을 때) — `text` = 그 본문(바이트 → 글자 색인 한 번 걷기).
    pub(crate) fn set_symbols(&mut self, key: (u64, u64), ol: &Outline, text: &str) {
        let tab_changed = self.key.is_some_and(|k| k.0 != key.0);
        self.key = Some(key);
        if tab_changed {
            // 다른 문서 = 접기 기억도 다른 것(이름이 우연히 같아도 섞지 않는다).
            self.fold.clear();
            self.caret = None;
        }
        self.all = ol
            .symbols
            .iter()
            .map(|s| Row {
                name: s.name.clone(),
                detail: s.detail.clone(),
                kind: s.kind,
                line: s.line,
                byte: s.byte,
                char_idx: 0,
            })
            .collect();
        self.orig_parent = ol.symbols.iter().map(|s| s.parent).collect();
        // 바이트 → 글자 색인(심볼을 바이트 순으로 세워 본문을 한 번만 걷는다).
        let mut order: Vec<usize> = (0..self.all.len()).collect();
        order.sort_by_key(|&i| self.all[i].byte);
        let mut chars = text.char_indices();
        let mut cur = chars.next();
        let mut ci = 0usize;
        for &i in &order {
            let b = self.all[i].byte;
            while let Some((bi, _)) = cur {
                if bi >= b {
                    break;
                }
                ci += 1;
                cur = chars.next();
            }
            self.all[i].char_idx = ci;
        }
        self.by_char = order;
        // 종류 트리는 `retree`가 만든다(설정·심볼 어느 쪽이 바뀌어도 같은 길).
        self.vis = visible_tree(&self.cfg, ol);
        self.retree_from_vis();
    }

    /// 설정만 바뀐 경우 — 심볼은 그대로, 종류 트리만 다시(`Outline`을 다시 받지 않게 행에서 재구성).
    fn retree(&mut self) {
        let ol = Outline {
            symbols: self
                .all
                .iter()
                .enumerate()
                .map(|(i, r)| nsql_script::outline::Symbol {
                    kind: r.kind,
                    name: r.name.clone(),
                    detail: r.detail.clone(),
                    line: r.line,
                    byte: r.byte,
                    depth: 0,
                    parent: self.orig_parent.get(i).copied().flatten(),
                })
                .collect(),
        };
        self.vis = visible_tree(&self.cfg, &ol);
        self.retree_from_vis();
    }

    fn retree_from_vis(&mut self) {
        let n = self.all.len();
        self.eff_depth = vec![0; n];
        self.eff_parent = vec![None; n];
        self.has_child = vec![false; n];
        self.kind_shown = vec![false; n];
        for v in &self.vis {
            self.eff_depth[v.idx] = v.depth;
            self.eff_parent[v.idx] = v.parent;
            self.kind_shown[v.idx] = true;
            if let Some(p) = v.parent {
                self.has_child[p] = true;
            }
        }
        self.rebuild();
    }

    fn fold_key(&self, idx: usize) -> FoldKey {
        (self.all[idx].name.clone(), self.all[idx].kind)
    }

    /// 이 노드가 펼쳐져 있는가(사용자 토글 → 없으면 깊이 규칙).
    fn expanded(&self, idx: usize) -> bool {
        self.fold
            .get(&self.fold_key(idx))
            .copied()
            .unwrap_or(self.eff_depth[idx] < self.cfg.expand_depth)
    }

    fn rebuild(&mut self) {
        let filtering = !self.filter.is_empty();
        if filtering {
            // 일치 행 + 그 조상(접힘 무시).
            let mut keep = vec![false; self.all.len()];
            for v in &self.vis {
                let r = &self.all[v.idx];
                if self
                    .filter
                    .matches(&format!("{} {} {}", r.name, r.detail, r.kind.label()))
                {
                    keep[v.idx] = true;
                    let mut p = v.parent;
                    while let Some(a) = p {
                        if keep[a] {
                            break;
                        }
                        keep[a] = true;
                        p = self.eff_parent[a];
                    }
                }
            }
            self.rows = self
                .vis
                .iter()
                .map(|v| v.idx)
                .filter(|&i| keep[i])
                .collect();
        } else {
            let rows = fold_rows(&self.vis, &|idx, _d| self.expanded(idx));
            self.rows = rows;
        }
        if self.sel.is_some_and(|s| s >= self.rows.len()) {
            self.sel = None;
        }
        self.clamp_scroll();
    }

    fn content_h(&self) -> i32 {
        self.rows.len() as i32 * self.row_h
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h() - self.list_rect.h).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max);
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.list_rect.contains(p) || self.row_h <= 0 {
            return None;
        }
        let r = ((p.y - self.list_rect.y + self.scroll_y) / self.row_h) as usize;
        (r < self.rows.len()).then_some(r)
    }

    fn reveal(&mut self, r: usize) {
        let top = r as i32 * self.row_h;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if top + self.row_h > self.scroll_y + self.list_rect.h {
            self.scroll_y = top + self.row_h - self.list_rect.h;
        }
        self.clamp_scroll();
    }

    /// 열기 요청(바이트 오프셋 · 1회성).
    pub(crate) fn take_open(&mut self) -> Option<usize> {
        self.open.take()
    }

    /// 토글 클릭(1회성) — 호스트가 [`toggle_key`]의 설정을 뒤집고 `set_cfg`로 돌려준다.
    pub(crate) fn take_toggle(&mut self) -> Option<BtnKind> {
        self.pending_toggle.take()
    }

    /// 셰브론 x 범위(행의 유효 깊이 기준).
    fn chevron_x(&self, idx: usize) -> (i32, i32) {
        let px = |v: f32| (v * self.scale).round() as i32;
        let x0 = self.list_rect.x + px(PAD) + self.eff_depth[idx] as i32 * px(INDENT);
        (x0, x0 + px(CHEV_W))
    }

    fn set_fold(&mut self, idx: usize, on: bool) {
        let k = self.fold_key(idx);
        self.fold.insert(k, on);
    }

    fn toggle_fold(&mut self, idx: usize) {
        let on = !self.expanded(idx);
        self.set_fold(idx, on);
        self.rebuild();
    }

    /// 모두 펼침/접음(보이는 자식이 있는 노드 전부).
    fn fold_all(&mut self, expand: bool) {
        for i in 0..self.all.len() {
            if self.has_child[i] {
                self.set_fold(i, expand);
            }
        }
        self.rebuild();
    }

    /// ★ 커서 추종(`outline.follow_caret`) — 호스트가 틱마다 캐럿(글자 색인)을 준다 · 바뀐 틱만 일한다(이진 탐색 1회) ·
    ///   패널에 포커스가 있거나 필터 중이면 선택을 건드리지 않는다. 다시 그려야 하면 `true`.
    pub(crate) fn set_caret(&mut self, caret: usize) -> bool {
        if self.caret == Some(caret) {
            return false;
        }
        self.caret = Some(caret);
        if !self.cfg.follow_caret || self.focused || !self.filter.is_empty() || self.all.is_empty()
        {
            return false;
        }
        // 글자 색인 ≤ 캐럿인 마지막 심볼.
        let k = self
            .by_char
            .partition_point(|&i| self.all[i].char_idx <= caret);
        let Some(&i0) = k.checked_sub(1).and_then(|k| self.by_char.get(k)) else {
            return false;
        };
        let mut idx = i0;
        // 종류로 숨었으면 보이는 조상으로.
        while !self.kind_shown[idx] {
            match self.eff_parent[idx] {
                Some(p) => idx = p,
                None => return false,
            }
        }
        // 접힌 조상은 펼친다.
        let mut changed = false;
        let mut p = self.eff_parent[idx];
        while let Some(a) = p {
            if !self.expanded(a) {
                self.set_fold(a, true);
                changed = true;
            }
            p = self.eff_parent[a];
        }
        if changed {
            self.rebuild();
        }
        let Some(r) = self.rows.iter().position(|&i| i == idx) else {
            return changed;
        };
        if self.sel != Some(r) {
            self.sel = Some(r);
            self.reveal(r);
            return true;
        }
        changed
    }

    fn filter_feed(&mut self, ev: &InputEvent) -> FilterEvent {
        let mut inv = Invalidations::default();
        let evt = self.filter.on_event(ev, &mut inv);
        let now = self.filter.display_text();
        if evt == FilterEvent::Changed || now != self.filter_text {
            self.filter_text = now;
            self.rebuild();
        }
        if let FilterEvent::Side(k) = evt {
            // 토글은 설정이 원천 — 표시는 호스트가 `set_cfg`로 되돌려 준다(눌린 그대로 두면 설정과 어긋날 수 있다).
            self.pending_toggle = Some(k);
        }
        if evt == FilterEvent::LeaveDown {
            // 이력 끝에서 ↓/Tab = 심볼 목록 첫 행으로(사용자 09-23).
            self.filter.set_focused(false);
            if !self.rows.is_empty() {
                self.sel = Some(0);
                self.reveal(0);
            }
        }
        evt
    }

    /// IME 조합·확정 뒤(호스트) — 조합 중 글자까지 바로 거른다.
    pub(crate) fn query_changed(&mut self) {
        if !self.filter.is_focused() {
            return;
        }
        self.filter.refresh();
        let now = self.filter.display_text();
        if now != self.filter_text {
            self.filter_text = now;
            self.rebuild();
        }
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.now_hint = now_ms;
        self.filter.tick(now_ms) | self.bars.tick(now_ms) | self.typeahead.tick(now_ms)
    }

    /// 타입어헤드 설정(호스트 · 객체 탐색기와 같은 한 벌).
    pub(crate) fn set_typeahead(&mut self, cfg: crate::explorer::TypeAheadCfg) {
        self.ta_cfg = cfg;
        self.typeahead.set_timeout(cfg.timeout_ms);
        if !cfg.enabled {
            self.typeahead.clear();
        }
    }

    fn row_label(&self, r: usize) -> String {
        self.rows
            .get(r)
            .map(|&i| self.all[i].name.clone())
            .unwrap_or_default()
    }

    /// 타입어헤드 글자(객체 탐색기와 같은 규칙: 접두 확장 = 지금 행부터 · 새 접두 = 다음 행부터).
    fn typeahead_char(&mut self, c: char) -> bool {
        if !self.ta_cfg.enabled || !self.ta_cfg.filter.accepts(c) {
            return false;
        }
        let q = self.typeahead.push(c, self.now_hint);
        let n = self.rows.len();
        if n == 0 {
            return true;
        }
        let from = self
            .sel
            .map_or(0, |p| if q.include_caret { p } else { (p + 1) % n });
        if let Some(k) = nexa_ctl::typeahead::find_prefix(n, from, &q.prefix, |k| self.row_label(k))
        {
            self.sel = Some(k);
            self.reveal(k);
        }
        true
    }

    /// 타입어헤드 활성 중 ↑/↓ = 접두 매치 안에서 순환.
    fn ta_step(&mut self, down: bool) -> bool {
        let ta = self.typeahead.composing();
        let n = self.rows.len();
        if ta.is_empty() || n == 0 {
            return false;
        }
        self.typeahead.touch(self.now_hint);
        let from = self
            .sel
            .map_or(0, |p| if down { (p + 1) % n } else { (p + n - 1) % n });
        let hit = if down {
            nexa_ctl::typeahead::find_prefix(n, from, &ta, |k| self.row_label(k))
        } else {
            nexa_ctl::typeahead::find_prefix_rev(n, from, &ta, |k| self.row_label(k))
        };
        if let Some(k) = hit {
            self.sel = Some(k);
            self.reveal(k);
        }
        true
    }

    /// ←: 펼친 노드면 접기 · 아니면 부모 행으로. →: 접힌 노드면 펼치기 · 아니면 첫 자식(다음 행)으로.
    fn fold_key_nav(&mut self, right: bool) -> bool {
        let Some(r) = self.sel else {
            return false;
        };
        let Some(&idx) = self.rows.get(r) else {
            return false;
        };
        if right {
            if self.has_child[idx] && !self.expanded(idx) {
                self.set_fold(idx, true);
                self.rebuild();
            } else if self.has_child[idx] && r + 1 < self.rows.len() {
                self.sel = Some(r + 1);
                self.reveal(r + 1);
            }
        } else if self.has_child[idx] && self.expanded(idx) {
            self.set_fold(idx, false);
            self.rebuild();
        } else if let Some(p) = self.eff_parent[idx] {
            if let Some(pr) = self.rows.iter().position(|&i| i == p) {
                self.sel = Some(pr);
                self.reveal(pr);
            }
        }
        true
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        // ★ 필터 팝업(이력 드롭다운) 열림 중 마우스는 틀이 먼저(사용자 10-07 · 공용 규칙 `popup_takes/keeps`).
        // ★ 상자 팝업(편집 메뉴·이력)이 떠 있으면 키는 **그 상자(메뉴)가 먼저**(팝업 최상위 · 사용자 10-13 "모든 패널에서 다른 컨트롤이 먼저 수신").
        if self.filter.popup_open()
            && matches!(ev, InputEvent::Key { .. } | InputEvent::Char { .. })
        {
            let _ = self.filter_feed(ev);
            return true;
        }
        if self.filter.popup_takes(ev) {
            let _ = self.filter_feed(ev);
            if self.filter.popup_keeps(ev) {
                return true;
            }
        }
        if matches!(
            ev,
            InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. }
        ) {
            // 토글(Side)·글 변경은 틀이 알려 준다 — 머리 버튼 호버는 이어서 본다.
            let evt = self.filter_feed(ev);
            if evt != FilterEvent::None {
                return true;
            }
        }
        // 필터 상자 우클릭 = 편집 메뉴(틀이 연다 · 10-13).
        if let InputEvent::RightDown { x, y } = *ev {
            if self.filter.bounds().contains(Point { x, y }) {
                let _ = self.filter_feed(ev);
                return true;
            }
        }
        let (_, ny, consumed) = self.bars.on_event(
            ev,
            self.list_rect,
            self.list_rect.w,
            self.content_h().max(self.list_rect.h),
            0,
            self.scroll_y,
            self.scale,
        );
        if ny != self.scroll_y {
            self.scroll_y = ny;
            self.clamp_scroll();
        }
        if consumed {
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                let h = self.row_at(p);
                let hh = self.hdr_btn.iter().position(|r| r.contains(p));
                let changed = h != self.hover || hh != self.hdr_hover;
                self.hover = h;
                self.hdr_hover = hh;
                changed
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if let Some(b) = self.hdr_btn.iter().position(|r| r.contains(p)) {
                    self.fold_all(b == 0);
                    return true;
                }
                let in_f = self.filter.bounds().contains(p);
                self.filter.set_focused(in_f);
                if in_f {
                    self.filter_feed(ev);
                    return true;
                }
                if let Some(r) = self.row_at(p) {
                    let idx = self.rows[r];
                    let (cx0, cx1) = self.chevron_x(idx);
                    if self.has_child[idx] && (cx0..cx1).contains(&x) && self.filter.is_empty() {
                        self.sel = Some(r);
                        self.toggle_fold(idx);
                        return true;
                    }
                    self.sel = Some(r);
                    self.open = Some(self.all[idx].byte);
                } else if self.list_rect.contains(p) {
                    self.sel = None;
                }
                true
            }
            InputEvent::Wheel { delta } => {
                self.scroll_y -= delta / 120 * self.row_h * 3;
                self.clamp_scroll();
                true
            }
            InputEvent::Char { .. }
            | InputEvent::SelectAll
            | InputEvent::Undo
            | InputEvent::Redo
                if self.filter.is_focused() =>
            {
                self.filter_feed(ev);
                true
            }
            // ★ 글자 = 타입어헤드(필터 포커스가 아닐 때 · 09-28) · Backspace = 접두 한 글자 지우기.
            InputEvent::Char { c, .. } => {
                if c == '\u{8}' {
                    if self.typeahead.is_active() {
                        if let Some(q) = self.typeahead.backspace(self.now_hint) {
                            let n = self.rows.len();
                            let from = self.sel.unwrap_or(0);
                            if let Some(k) =
                                nexa_ctl::typeahead::find_prefix(n, from, &q.prefix, |k| {
                                    self.row_label(k)
                                })
                            {
                                self.sel = Some(k);
                                self.reveal(k);
                            }
                        }
                        return true;
                    }
                    return false;
                }
                if c == '\t' || c == '\n' || c == '\r' {
                    return false;
                }
                self.typeahead_char(c)
            }
            InputEvent::Key { key, .. } => {
                let n = self.rows.len();
                if !self.filter.is_focused() {
                    if matches!(key, CtlKey::Down | CtlKey::Up) && self.ta_step(key == CtlKey::Down)
                    {
                        return true;
                    }
                    if matches!(key, CtlKey::Escape) && self.typeahead.is_active() {
                        self.typeahead.clear();
                        return true;
                    }
                    if matches!(key, CtlKey::Left | CtlKey::Right) && self.filter.is_empty() {
                        return self.fold_key_nav(key == CtlKey::Right);
                    }
                }
                // 필터에 포커스면 이동 키는 이력(드롭다운/Flat)이 먼저 — 손대지 않으면(None) 종전대로 목록 이동.
                if self.filter.is_focused()
                    && matches!(
                        key,
                        CtlKey::Down
                            | CtlKey::Up
                            | CtlKey::Home
                            | CtlKey::End
                            | CtlKey::PageUp
                            | CtlKey::PageDown
                    )
                {
                    match self.filter_feed(ev) {
                        FilterEvent::None | FilterEvent::Side(_) => {}
                        _ => return true,
                    }
                }
                match key {
                    CtlKey::Down if n > 0 => {
                        let r = self.sel.map_or(0, |s| (s + 1).min(n - 1));
                        self.sel = Some(r);
                        self.reveal(r);
                        true
                    }
                    CtlKey::Up if n > 0 => {
                        let r = self.sel.map_or(0, |s| s.saturating_sub(1));
                        self.sel = Some(r);
                        self.reveal(r);
                        true
                    }
                    CtlKey::Enter => {
                        if let Some(r) = self
                            .sel
                            .or((n > 0 && self.filter.is_focused()).then_some(0))
                        {
                            self.open = Some(self.all[self.rows[r]].byte);
                        }
                        true
                    }
                    CtlKey::Escape => {
                        if self.filter.is_focused() && !self.filter_text.is_empty() {
                            self.filter.set_text("");
                            self.filter_text.clear();
                            self.rebuild();
                            return true;
                        }
                        false
                    }
                    _ => {
                        if self.filter.is_focused() {
                            let _ = self.filter_feed(ev);
                            return true;
                        }
                        false
                    }
                }
            }
            _ => false,
        }
    }

    fn icon_image(&mut self, kind: IconKind, size: i32) -> Rc<IconImage> {
        self.icon_cache
            .entry((kind, size))
            .or_insert_with(|| {
                let base = exp_icons::image(kind, kind.color());
                Rc::new(base.resized(size.max(1) as u32, size.max(1) as u32))
            })
            .clone()
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let scale = self.scale;
        let px = move |v: f32| (v * scale).round() as i32;
        let b = self.bounds;
        let pad = px(PAD);
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        dc.select_font(FontSlot::Base, true);
        let hr = self.header_rect;
        let hy = dc.text_center_y(hr.y, hr.h);
        let title = format!(
            "{} ({}/{})",
            t(Msg::MnOutlinePanel),
            self.rows.len(),
            self.all.len()
        );
        let title_clip = Rect::new(hr.x, hr.y, (self.hdr_btn[0].x - hr.x - pad).max(0), hr.h);
        dc.text(hr.x + pad, hy, title_clip, &title, th.text);
        dc.select_font(FontSlot::Base, false);
        // 머리 버튼 [모두 펼침] [모두 접음] — 셰브론 둘(펼침 = ⌄ · 접음 = ›) · 호버 배경.
        let cw = px(10.0);
        for (i, r) in self.hdr_btn.iter().enumerate() {
            if self.hdr_hover == Some(i) {
                dc.fill_rect_alpha(*r, th.text, 0.08);
            }
            let chev = Rect::new(r.x + (r.w - cw) / 2, r.y + (r.h - cw) / 2, cw, cw);
            let color = if self.hdr_hover == Some(i) {
                th.text
            } else {
                th.text_dim
            };
            nexa_ctl::controls::draw_chevron_90_in(dc, chev, color, i == 0, Some(*r));
        }
        self.filter.paint(dc, th, true);
        let lr = self.list_rect;
        let rh = self.row_h;
        if self.rows.is_empty() {
            let msg = if self.all.is_empty() {
                t(Msg::OutlineEmpty).to_string()
            } else if !self.filter.is_empty() {
                tf(Msg::ProjNoMatch, &[])
            } else {
                t(Msg::OutlineAllHidden).to_string()
            };
            let ty = dc.text_center_y(lr.y, rh);
            dc.text(lr.x + pad, ty, lr, &msg, th.text_dim);
            self.bars.paint(
                dc,
                th,
                lr,
                lr.w,
                self.content_h().max(lr.h),
                0,
                self.scroll_y,
                self.scale,
            );
            return;
        }
        let first = (self.scroll_y / rh) as usize;
        let mut y = lr.y - self.scroll_y % rh;
        let chev_w = px(CHEV_W);
        let icon_w = px(ICON_W);
        let icon_px = px(ICON_PX);
        let cw = px(10.0);
        let filtering = !self.filter.is_empty();
        for r in first..self.rows.len() {
            if y >= lr.bottom() {
                break;
            }
            let idx = self.rows[r];
            let row_rect = Rect::new(lr.x, y, lr.w, rh);
            if self.sel == Some(r) {
                dc.fill_rect(
                    row_rect,
                    if self.focused {
                        th.sel_bg
                    } else {
                        th.sel_bg_inactive
                    },
                );
            } else if self.hover == Some(r) {
                dc.fill_rect_alpha(row_rect, th.text, 0.06);
            }
            let ty = dc.text_center_y(y, rh);
            let (cx0, _) = self.chevron_x(idx);
            // 셰브론(보이는 자식이 있을 때 · 필터 중엔 전부 펼친 모양) — 탐색기와 같은 부품.
            if self.has_child[idx] {
                let expanded = filtering || self.expanded(idx);
                let chev = Rect::new(cx0 + (chev_w - cw) / 2, y + (rh - cw) / 2, cw, cw);
                let color = if expanded || self.hover == Some(r) {
                    th.text
                } else {
                    th.text_dim
                };
                nexa_ctl::controls::draw_chevron_90_in(dc, chev, color, expanded, Some(row_rect));
            }
            let mut tx = cx0 + chev_w;
            let (kind, detail_s) = (self.all[idx].kind, self.all[idx].detail.clone());
            if let Some(ik) = icon_kind(kind, &detail_s) {
                let img = self.icon_image(ik, icon_px);
                dc.image(tx, y + (rh - icon_px) / 2, &img, row_rect);
            }
            tx += icon_w;
            let row = &self.all[idx];
            // 줄 번호(흐린 작은 글) → 이름(문장은 흐리게 · 심볼은 본문색 · 서브프로그램은 굵게) → 종류/타입(오른쪽 · 흐림).
            dc.select_font(FontSlot::Status, false);
            let ln = row.line.to_string();
            let lw = dc.text_width(&ln);
            dc.text(tx, ty, row_rect, &ln, th.text_dim);
            tx += lw + px(6.0);
            let detail = if row.detail.is_empty() {
                row.kind.label().to_string()
            } else {
                row.detail.clone()
            };
            let dw = dc.text_width(&detail);
            let bold = matches!(
                row.kind,
                SymKind::Procedure
                    | SymKind::Function
                    | SymKind::Package
                    | SymKind::PackageBody
                    | SymKind::Trigger
            );
            dc.select_font(FontSlot::Base, bold);
            let fg = if row.kind == SymKind::Statement {
                th.text_dim
            } else if bold {
                th.accent
            } else {
                th.text
            };
            let clip = Rect::new(tx, y, (lr.right() - tx - dw - pad * 2).max(0), rh);
            let shown = nexa_ctl::draw::ellipsize_middle(dc, &row.name, clip.w);
            dc.text(tx, ty, clip, &shown, fg);
            dc.select_font(FontSlot::Status, false);
            dc.text(lr.right() - pad - dw, ty, row_rect, &detail, th.text_dim);
            dc.select_font(FontSlot::Base, false);
            y += rh;
        }
        // 타입어헤드 HUD(입력 중 접두 · 패널 영역 기준 3×3 위치 · 객체 탐색기와 같은 부품).
        let ta_text = self.typeahead.composing();
        if !ta_text.is_empty() {
            nexa_ctl::typeahead::paint_hud(
                dc,
                self.bounds,
                self.scale,
                self.ta_cfg.pos,
                &ta_text,
                th,
            );
        }
        self.bars.paint(
            dc,
            th,
            lr,
            lr.w,
            self.content_h().max(lr.h),
            0,
            self.scroll_y,
            self.scale,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_script::outline::Symbol;

    fn sym(kind: SymKind, name: &str, detail: &str, parent: Option<usize>, depth: u8) -> Symbol {
        Symbol {
            kind,
            name: name.to_string(),
            detail: detail.to_string(),
            line: 1,
            byte: 0,
            depth,
            parent,
        }
    }

    /// D-357 결정 1 — 종류 토글 판정(MC/DC): 토글 하나가 그 종류의 답만 뒤집는다.
    #[test]
    fn kind_visible_mcdc() {
        let base = OutlineCfg::default();
        assert!(!kind_visible(&base, SymKind::Statement, "select"));
        assert!(!kind_visible(&base, SymKind::Statement, "dml"));
        assert!(!kind_visible(&base, SymKind::Statement, "ddl"));
        assert!(!kind_visible(&base, SymKind::Statement, "command"));
        assert!(!kind_visible(&base, SymKind::Statement, "block"));
        assert!(kind_visible(&base, SymKind::Declared, "NUMBER"));
        assert!(kind_visible(&base, SymKind::Object, "table"));
        assert!(kind_visible(&base, SymKind::Cte, "cte"));
        let sel = OutlineCfg {
            show_select: true,
            ..base
        };
        assert!(kind_visible(&sel, SymKind::Statement, "select"));
        assert!(!kind_visible(&sel, SymKind::Statement, "dml"));
        let dml = OutlineCfg {
            show_dml: true,
            ..base
        };
        assert!(kind_visible(&dml, SymKind::Statement, "dml"));
        assert!(!kind_visible(&dml, SymKind::Statement, "select"));
        let ddl = OutlineCfg {
            show_ddl: true,
            ..base
        };
        assert!(kind_visible(&ddl, SymKind::Statement, "ddl"));
        assert!(!kind_visible(&ddl, SymKind::Statement, "block"));
        let other = OutlineCfg {
            show_other: true,
            ..base
        };
        assert!(kind_visible(&other, SymKind::Statement, "command"));
        assert!(kind_visible(&other, SymKind::Statement, "block"));
        assert!(kind_visible(&other, SymKind::Statement, "sql"));
        assert!(!kind_visible(&other, SymKind::Statement, "select"));
        let nodecl = OutlineCfg {
            show_declared: false,
            ..base
        };
        assert!(!kind_visible(&nodecl, SymKind::Declared, "NUMBER"));
        assert!(kind_visible(&nodecl, SymKind::Define, ""));
    }

    /// 숨은 조상(문장 행)은 건너뛰어 자식(CTE)이 뿌리로 올라온다 · 토글을 켜면 원래 깊이.
    #[test]
    fn visible_tree_skips_hidden_ancestors() {
        let ol = Outline {
            symbols: vec![
                sym(SymKind::Statement, "WITH a AS …", "select", None, 0),
                sym(SymKind::Cte, "a", "cte", Some(0), 1),
                sym(SymKind::PackageBody, "pkg", "package body", None, 0),
                sym(SymKind::Procedure, "p", "procedure", Some(2), 1),
                sym(SymKind::Declared, "v", "NUMBER", Some(3), 2),
            ],
        };
        let v = visible_tree(&OutlineCfg::default(), &ol);
        let got: Vec<(usize, u8, Option<usize>)> =
            v.iter().map(|n| (n.idx, n.depth, n.parent)).collect();
        assert_eq!(
            got,
            vec![(1, 0, None), (2, 0, None), (3, 1, Some(2)), (4, 2, Some(3))]
        );
        let on = OutlineCfg {
            show_select: true,
            ..OutlineCfg::default()
        };
        let v2 = visible_tree(&on, &ol);
        assert_eq!(
            (v2[0].idx, v2[1].idx, v2[1].depth, v2[1].parent),
            (0, 1, 1, Some(0))
        );
        // 선언 끔 = v 빠짐 · p는 그대로.
        let nd = OutlineCfg {
            show_declared: false,
            ..OutlineCfg::default()
        };
        assert_eq!(visible_tree(&nd, &ol).len(), 3);
    }

    /// 접기: 기본 펼침 깊이 1 = 뿌리 펼침 · 그 안(깊이 1)의 자식은 접힘 · 펼친 노드의 자식만 보인다.
    #[test]
    fn fold_rows_respects_expand_depth_and_overrides() {
        let vis = vec![
            VisNode {
                idx: 2,
                depth: 0,
                parent: None,
            },
            VisNode {
                idx: 3,
                depth: 1,
                parent: Some(2),
            },
            VisNode {
                idx: 4,
                depth: 2,
                parent: Some(3),
            },
            VisNode {
                idx: 5,
                depth: 0,
                parent: None,
            },
        ];
        let depth1 = |_i: usize, d: u8| d < 1;
        assert_eq!(fold_rows(&vis, &depth1), vec![2, 3, 5]);
        let depth0 = |_i: usize, _d: u8| false;
        assert_eq!(fold_rows(&vis, &depth0), vec![2, 5]);
        let all = |_i: usize, _d: u8| true;
        assert_eq!(fold_rows(&vis, &all), vec![2, 3, 4, 5]);
        // 사용자 토글 = 3만 펼침(뿌리 2는 접힘) → 2의 자식이 안 보이므로 3·4도 안 보인다.
        let only3 = |i: usize, _d: u8| i == 3;
        assert_eq!(fold_rows(&vis, &only3), vec![2, 5]);
    }
}
