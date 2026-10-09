//! **코드 완성 호스트**(docs/76 · docs/47 §6 · 사용자 09-23) — 팝업 상태 · 트리거 · 후보 조립 · 확정 · MRU · 문서 아웃라인 캐시.
//!
//! 계산은 `nsql_script::intel`(문맥·랭킹 · 순수)과 `nsql_script::outline`(문서 심볼)에 있고, 메타는 탐색기가 채운 `MetaStore`
//! 스냅샷([`MetaView`])만 읽는다(서버 접속 0 · 26 §8). 팝업 = nexa-ctl `ContextMenu` 재사용(D-201 · 창 안 배치 규칙).
//! 호스트(main.rs)가 하는 것: 글자 입력 뒤 [`Intel::after_char`] → 필요하면 [`Intel::request`] · 키(↑↓ Enter Tab Esc)를 팝업에 먼저 ·
//! [`Intel::take_accept`]로 확정 글자를 편집기에 · 틱에서 [`Intel::due`] 디바운스.

use crate::exp_icons::{self, IconKind};
use nexa_ctl::controls::ctxmenu::{ContextMenu, CtxItem, MenuIcon};
use nexa_ctl::geom::{Point, Rect};
use nsql_catalog::ObjectKind;
use nsql_core::Dialect;
use nsql_run::meta::{ColState, Coverage, Interner, Snapshot, Sym};
use nsql_script::builtins;
use nsql_script::intel::{self, Cand, CandKind, Context, CtxKind, MatchMode};
use nsql_script::outline::{self, Outline, SymKind};
use nsql_settings::Settings;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 설정 묶음(`intel.*` · 47 §8-1).
#[derive(Clone, Debug)]
pub(crate) struct IntelCfg {
    pub enabled: bool,
    pub auto: bool,
    pub delay_ms: u64,
    pub triggers: Vec<char>,
    pub min_chars: usize,
    pub mode: MatchMode,
    pub recent: bool,
    pub max_items: usize,
    pub rows: usize,
    /// 팝업 폭 상한(논리 px · 넘치면 가운데 … + 가로 스크롤 · 09-23).
    pub popup_max_w: i32,
    /// 고른 항목이 없을 때 Enter/Tab = 팝업만 닫고 키는 편집기로(참 · 기본) / 키를 삼킨다(거짓 = 두 번 입력 · 09-24).
    pub key_passthrough: bool,
    /// 상세 카드(팝업 옆 · 같은 높이 · 09-24) · 배경 투명도 % · 글자 투명도 %.
    pub detail_card: bool,
    /// ★ 문법 참조 완성(.sqlg 절별 · docs/82) — 라이선스 게이트(T-36 · 호스트가 `check(GrammarCompletion)`으로 정한다 · 기본 참).
    pub grammar: bool,
    pub detail_bg_alpha: u8,
    pub detail_text_alpha: u8,
    /// 접두 없이 고른 컬럼에 alias를 붙여 넣기(Alt = 반대 · 09-24).
    pub qualify_columns: bool,
    /// FROM 자리에 함수·패키지·프로시저도(낮은 레이어 · 채움 요청 포함 · `intel.from_routines` · 09-24).
    pub routines: bool,
    /// 항목 왼쪽에 종류 아이콘(탐색기와 같은 도형 · `intel.icons` · 09-24).
    pub icons: bool,
    /// 상세 카드가 대상을 바꾸기 전 머물러야 하는 시간(ms · 0 = 즉시 · `intel.card_settle_ms` · 09-24 "빠른 스크롤 중엔 누적 · 마지막만").
    pub card_settle_ms: u64,
    /// `*` → 모든 컬럼 조각의 구분(사용자 09-24): `star_lines` = 줄바꿈 적용(다음 줄 들여쓰기 뒤 `, 이름` · 줄 앞 쉼표) · 아니면 한 줄
    /// (`intel.star_layout`) · `star_tab` = 쉼표 뒤 Tab(기본 Space · `intel.star_comma_space`).
    pub star_lines: bool,
    /// 쉼표 뒤 Tab/Space(`intel.star_comma_space` · `None` = 편집기 설정 따름 = 기본 · 사용자 09-29).
    pub star_tab: Option<bool>,
    /// 한 세트로 드는 후보 상한(`intel.max_total` · 페이지 로딩 76 §13 · 그 위는 잘림).
    pub max_total: usize,
    /// 문서 전체 훑기 상한(KB · `intel.max_doc_kb` · 09-24 §187): 넘으면 캐럿 앞뒤 창(`WINDOW_BYTES`)만 · 문서 낱말·아웃라인 끔.
    pub max_doc_kb: usize,
    pub keywords: bool,
    pub doc_words: bool,
    pub insert_case: String,
    pub show_types: bool,
    /// T-178: 내장 함수·시스템 패키지·사전 객체(정적 표) · 삽입 옵션 · 시그니처 도움 · 예산.
    pub functions: bool,
    pub insert_parens: bool,
    /// `intel.types` — 자료형 자리에 방언의 내장 자료형(사용자 10-09).
    pub types: bool,
    pub insert_alias: bool,
    /// alias 방식(`intel.alias_style` · true = A, B, C 순서 · false = 약어 · 사용자 09-29).
    pub alias_letters: bool,
    pub insert_space: bool,
    pub insert_columns: bool,
    /// `JOIN 테이블 ON` 뒤 외래 키 조인 조건 조각(`intel.join_fk` · T-178).
    pub join_fk: bool,
    pub signature_help: bool,
    /// 캐럿 위 시그니처 카드(`intel.signature_card` · T-178) — 상태줄 글은 그대로 두고 카드를 더한다.
    pub signature_card: bool,
    pub budget_ms: u64,
}

impl IntelCfg {
    pub(crate) fn from_settings(s: &Settings) -> IntelCfg {
        IntelCfg {
            enabled: s.flag("intel.enabled"),
            auto: s.flag("intel.auto_activation"),
            delay_ms: s.int("intel.delay_ms").clamp(0, 2000) as u64,
            triggers: s
                .get("intel.trigger_chars")
                .unwrap_or(".")
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect(),
            min_chars: s.int("intel.min_chars").clamp(1, 5) as usize,
            mode: MatchMode::parse(s.get("intel.match").unwrap_or("fuzzy")),
            recent: s.flag("intel.recent_boost"),
            max_items: s.int("intel.max_items").clamp(50, 5000) as usize,
            rows: s.int("intel.popup_rows").clamp(6, 30) as usize,
            popup_max_w: s.int("intel.popup_max_width").clamp(240, 2000) as i32,
            key_passthrough: s.flag("intel.key_passthrough"),
            detail_card: s.flag("intel.detail_card"),
            grammar: true,
            detail_bg_alpha: s.int("intel.detail_bg_alpha").clamp(0, 100) as u8,
            detail_text_alpha: s.int("intel.detail_text_alpha").clamp(0, 100) as u8,
            qualify_columns: s.flag("intel.qualify_columns"),
            routines: s.flag("intel.from_routines"),
            icons: s.flag("intel.icons"),
            card_settle_ms: s.int("intel.card_settle_ms").clamp(0, 2000) as u64,
            star_lines: s.get("intel.star_layout").unwrap_or("inline") == "lines",
            star_tab: match s.get("intel.star_comma_space").unwrap_or("editor") {
                "tab" => Some(true),
                "space" => Some(false),
                _ => None,
            },
            max_total: s.int("intel.max_total").clamp(500, 20_000) as usize,
            max_doc_kb: s.int("intel.max_doc_kb").clamp(64, 65_536) as usize,
            keywords: s.flag("intel.keywords"),
            doc_words: s.flag("intel.document_words"),
            insert_case: s.get("intel.insert_case").unwrap_or("default").to_string(),
            show_types: s.flag("intel.show_types"),
            functions: s.flag("intel.functions"),
            insert_parens: s.flag("intel.insert_parens"),
            types: s.flag("intel.types"),
            insert_alias: s.flag("intel.insert_alias"),
            alias_letters: s.get("intel.alias_style").unwrap_or("abbr") == "letters",
            insert_space: s.flag("intel.insert_space"),
            insert_columns: s.flag("intel.insert_columns"),
            join_fk: s.flag("intel.join_fk"),
            signature_help: s.flag("intel.signature_help"),
            signature_card: s.flag("intel.signature_card"),
            budget_ms: s.int("intel.budget_ms").clamp(5, 500) as u64,
        }
    }
}

/// 메타 읽기 창(탐색기 스냅샷 · 접속 없으면 None).
pub(crate) struct MetaView<'a> {
    pub names: &'a Interner,
    pub snap: Arc<Snapshot>,
}

/// 호스트에 되돌리는 요청 — 컬럼이 없어서 즉시 채워야 할 테이블(47 §4 "즉시 채움").
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NeedColumns {
    pub schema: Option<String>,
    pub table: String,
    /// 지금 팝업이 기다리는 대상(참) / FROM 절의 나머지 테이블 선적재(거짓 · 워커 큐에서 뒤로 · 09-23).
    pub urgent: bool,
}

/// 확정 결과(편집기에 적용할 것).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Accept {
    /// 바이트 구간(접두 자리).
    pub replace: Range<usize>,
    pub text: String,
    /// 넣은 뒤 캐럿을 끝에서 이만큼 앞으로(`NAME()` = 1 · 괄호 안).
    pub caret_back: usize,
}

/// 문서 캐시 항목(탭별 · 본문 세대 열쇠 · D-203).
struct DocCache {
    rev: u64,
    outline: Outline,
    words: Vec<String>,
}

pub(crate) struct Intel {
    pub(crate) menu: ContextMenu,
    cfg: IntelCfg,
    /// 활성 탭의 들여쓰기 단위(탭 = true · 호스트가 요청마다 넣음) — `star_tab`이 "편집기 설정"일 때.
    editor_tab: bool,
    /// 종류 아이콘 캐시(탐색기 도형을 메뉴 아이콘으로 · 종류당 한 번 래스터).
    icons: HashMap<IconKind, MenuIcon>,
    /// ★ 페이지 로딩(76 §13 · T-196): `cands` = 한 세트(전체 랭킹 ≤ `max_total`) · 팝업 항목은 `cands[..shown]`만 조립 · 끝에 닿으면
    ///   `extend`가 `intel.max_items`만큼 더. 조립에 필요한 것(종류 아이콘 · 접두)은 요청 때 적어 둔다(메타 빌림 없이 다시 조립).
    shown: usize,
    icon_kinds: Vec<Option<IconKind>>,
    prefix_now: String,
    text_w: i32,
    /// ★ 자기 감속(09-24 §187): 예산(`intel.budget_ms`) 초과가 **연속 2회**면 그 탭의 문서 낱말을 끈다(`words_off`) · 호스트에 한 번 알림.
    over_streak: u8,
    words_off: HashSet<u64>,
    degraded: Option<u64>,
    /// ★ 상세 카드 머무름(사용자 09-24 · hover 효과 규칙과 같은 사상 = 사건은 목표 **덮어쓰기만** · 틱이 머문 마지막 목표만 넘김):
    ///   `card_want` = 마지막 강조 행(index · 바뀐 시각) · `card_shown` = 카드가 보이는 행. 첫 대상은 즉시, 그 뒤는 `card_settle_ms` 머문 뒤.
    card_want: Option<(usize, Instant)>,
    card_shown: Option<usize>,
    cands: Vec<Cand>,
    ctx: Option<Context>,
    /// 팝업이 열린 탭 id(다른 탭이면 닫는다).
    tab: Option<u64>,
    mru: Vec<String>,
    /// 자동 활성 마감(디바운스).
    due: Option<Instant>,
    docs: HashMap<u64, DocCache>,
    accept: Option<Accept>,
    /// 마지막 요청이 남긴 즉시 채움 요청.
    needs: Vec<NeedColumns>,
    /// 마지막 요청이 남긴 객체 목록 채움 요청(스키마 이름 · `DICT_SCHEMA` = 사전 · 09-23).
    need_objects: Vec<String>,
    /// 종류를 지정한 버킷 채움 요청(사용자 정의 타입 · 식 자리 함수·패키지 · 10-09) — `request_objects`가 안 읽는 종류.
    need_kinds: Vec<(String, ObjectKind)>,
    /// 마지막 요청이 남긴 **테이블 상세**(제약) 채움 요청 — `JOIN … ON` 조건 조각이 외래 키를 기다린다(T-178).
    need_details: Vec<nsql_run::meta::ObjId>,
    /// 루틴 인자 채움 요청(T-317 · `EXEC proc |`).
    need_args: Vec<nsql_run::meta::ObjId>,
    /// "불러오는 중" 표시 여부(마지막 요청).
    pub(crate) loading: bool,
    /// 객체 목록(테이블 등)을 기다리는 중(표시 문구 선택).
    loading_objects: bool,
    /// 마지막 요청이 예산(`intel.budget_ms`)을 넘겼으면 (후보 수, ms) — 호스트가 로그 한 줄.
    over_budget: Option<(usize, u128)>,
    /// 마지막 요청의 방언(확정 때 시그니처 조회).
    dialect: Option<Dialect>,
    /// 마지막으로 팝업을 붙인 캐럿 자리(호스트가 이번에 좌표를 못 주면 열린 팝업은 이 자리에 머문다 · 09-23).
    last_anchor: Option<Point>,
}

const MRU_MAX: usize = 20;
/// 문서 단어 상한(큰 문서 보호).
const DOC_WORDS_MAX: usize = 5000;

impl Intel {
    pub(crate) fn new(cfg: IntelCfg) -> Self {
        Intel {
            menu: ContextMenu::new(),
            cfg,
            editor_tab: false,
            icons: HashMap::new(),
            shown: 0,
            icon_kinds: Vec::new(),
            prefix_now: String::new(),
            text_w: 0,
            over_streak: 0,
            words_off: HashSet::new(),
            degraded: None,
            card_want: None,
            card_shown: None,
            cands: Vec::new(),
            ctx: None,
            tab: None,
            mru: Vec::new(),
            due: None,
            docs: HashMap::new(),
            accept: None,
            needs: Vec::new(),
            need_objects: Vec::new(),
            need_kinds: Vec::new(),
            need_details: Vec::new(),
            need_args: Vec::new(),
            loading: false,
            loading_objects: false,
            over_budget: None,
            dialect: None,
            last_anchor: None,
        }
    }

    /// 활성 탭의 들여쓰기 단위(탭 = true) — `star_tab`이 "편집기 설정"일 때 쉼표 뒤 글자(호스트가 요청마다 넣는다).
    pub(crate) fn set_editor_tab(&mut self, tab: bool) {
        self.editor_tab = tab;
    }

    pub(crate) fn set_cfg(&mut self, cfg: IntelCfg) {
        if !cfg.enabled {
            self.close();
            self.docs.clear();
        }
        self.cfg = cfg;
    }

    pub(crate) fn cfg(&self) -> &IntelCfg {
        &self.cfg
    }

    /// 자체 시험용(기동 명령 `intel.dump:<파일>` · 10-01 ⑭): 지금 후보를 `종류|글|설명` 한 줄씩.
    pub(crate) fn dump_cands(&self) -> String {
        self.cands
            .iter()
            .map(|c| format!("{:?}|{}|{}\n", c.kind, c.text, c.detail))
            .collect()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.menu.is_open()
    }

    pub(crate) fn close(&mut self) {
        self.menu.close();
        self.cands.clear();
        self.ctx = None;
        self.tab = None;
        self.due = None;
        self.loading = false;
        self.card_want = None;
        self.card_shown = None;
        self.shown = 0;
        self.icon_kinds = Vec::new();
    }

    /// 탭 하나의 아웃라인(캐시 · 세대가 같으면 재계산 0 · 큰 파일 단계는 호출자가 거른다).
    pub(crate) fn outline_for(
        &mut self,
        tab: u64,
        rev: u64,
        text: &dyn Fn() -> String,
        dialect: Option<Dialect>,
    ) -> &Outline {
        let stale = self.docs.get(&tab).is_none_or(|d| d.rev != rev);
        if stale {
            let src = text();
            let ol = outline::outline(&src, dialect);
            let classes = nsql_script::lexer::classify(&src);
            let mut words: Vec<String> = Vec::new();
            for w in outline::words(&src, &classes) {
                if words.len() >= DOC_WORDS_MAX {
                    break;
                }
                let t = w.text;
                if t.len() < 2
                    || !t.as_bytes()[0].is_ascii_alphabetic()
                        && !matches!(t.as_bytes()[0], b'_' | b':' | b'&' | b'@')
                {
                    continue;
                }
                if !words.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                    words.push(t.to_string());
                }
            }
            self.docs.insert(
                tab,
                DocCache {
                    rev,
                    outline: ol,
                    words,
                },
            );
            // 캐시 상한 — 닫힌 탭의 항목이 남지 않게(탭 32개 넘으면 지금 탭만 남긴다).
            if self.docs.len() > 32 {
                self.docs.retain(|k, _| *k == tab);
            }
        }
        &self.docs.get(&tab).expect("just inserted").outline
    }

    /// 글자 하나가 편집기에 들어간 뒤 — 트리거 문자면 즉시(`true` = 지금 요청), 식별자 글자면 디바운스, 그 밖은 닫는다.
    pub(crate) fn after_char(&mut self, c: char, prefix_len: usize) -> bool {
        if !self.cfg.enabled {
            return false;
        }
        if self.cfg.triggers.contains(&c) {
            self.due = None;
            return self.cfg.auto;
        }
        let ident = c.is_alphanumeric() || c == '_' || c == '$' || c == '#';
        if !ident {
            self.close();
            return false;
        }
        if self.is_open() {
            // 열린 채 타이핑 = 바로 다시 거른다.
            return true;
        }
        if self.cfg.auto && prefix_len >= self.cfg.min_chars {
            if self.cfg.delay_ms == 0 {
                return true;
            }
            self.due = Some(Instant::now() + Duration::from_millis(self.cfg.delay_ms));
        }
        false
    }

    /// 디바운스 마감이 지났으면(틱) `true` — 호스트가 `request`를 부른다.
    pub(crate) fn due(&mut self, now: Instant) -> bool {
        match self.due {
            Some(t) if now >= t => {
                self.due = None;
                true
            }
            _ => false,
        }
    }

    /// 다음 깨울 시각(틱 스케줄러) — 디바운스 마감과 카드 머무름 마감 중 이른 것.
    pub(crate) fn next_wake(&self) -> Option<Instant> {
        match (self.due, self.card_wake()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// 강조 행이 바뀌었을 수 있는 사건 뒤(키·마우스·새 목록) — 목표만 덮어쓴다(비용 0 · 큐 없음).
    pub(crate) fn note_hover(&mut self, now: Instant) {
        if !self.is_open() {
            return;
        }
        let i = self.menu.hovered().unwrap_or(0);
        if self.card_want.is_none_or(|(w, _)| w != i) {
            self.card_want = Some((i, now));
        }
    }

    /// 머문 목표를 카드에 넘긴다(틱) — 넘겼으면 `true`(호스트 = 상세 선조회 + 다시 그리기). 첫 대상·`card_settle_ms` 0 = 즉시.
    pub(crate) fn card_tick(&mut self, now: Instant) -> bool {
        let Some((i, since)) = self.card_want else {
            return false;
        };
        if self.card_shown == Some(i) {
            return false;
        }
        let settle = Duration::from_millis(self.cfg.card_settle_ms);
        if self.card_shown.is_none() || now.saturating_duration_since(since) >= settle {
            self.card_shown = Some(i);
            return true;
        }
        false
    }

    /// 카드가 아직 안 넘긴 목표가 있으면 그 마감 시각.
    fn card_wake(&self) -> Option<Instant> {
        let (i, since) = self.card_want?;
        if self.card_shown == Some(i) || self.card_shown.is_none() {
            return None;
        }
        Some(since + Duration::from_millis(self.cfg.card_settle_ms))
    }

    /// 카드에 보이는 행(시험·진단).
    #[cfg(test)]
    pub(crate) fn card_index(&self) -> Option<usize> {
        self.card_shown
    }

    /// 후보 조립 + 팝업 열기. `tab` = 편집기 탭 id · `doc` = 본문 · `caret` = 바이트 오프셋 · `anchor` = 캐럿 아래 점 · `host` = 창.
    /// 반환 = 열렸는가. 즉시 채움 요청은 [`Intel::take_needs`]로.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn request(
        &mut self,
        tab: u64,
        rev: u64,
        doc: &str,
        caret: usize,
        dialect: Option<Dialect>,
        meta: Option<&MetaView<'_>>,
        anchor: Option<Point>,
        host: Rect,
        scale: f32,
        point_of: &dyn Fn(usize) -> Option<Point>,
    ) -> bool {
        if !self.cfg.enabled {
            return false;
        }
        let t0 = Instant::now();
        self.needs.clear();
        self.need_objects.clear();
        self.need_kinds.clear();
        self.need_details.clear();
        self.loading = false;
        self.loading_objects = false;
        // ★ 창 방식(09-24 §187 · 사용자 "파일 용량별 레벨"): 문서가 `intel.max_doc_kb`를 넘으면 캐럿 앞뒤 창만 문맥으로 읽고(전체
        //   `classify`/문장 분할 = 4 MB에 130 ms) 문서 낱말·아웃라인 캐시는 만들지 않는다. 구간 값은 절대 바이트로 되돌린다.
        let windowed = doc.len() > self.cfg.max_doc_kb * 1024;
        let (base, slice) = if windowed {
            let (a, b) = window_of(doc, caret);
            (a, &doc[a..b])
        } else {
            (0, doc)
        };
        let mut ctx = intel::context_at(slice, caret - base, dialect);
        if base > 0 {
            ctx.replace = ctx.replace.start + base..ctx.replace.end + base;
            ctx.statement = ctx.statement.start + base..ctx.statement.end + base;
        }
        let ctx = ctx;
        let prefix_start = ctx.replace.start;
        if std::env::var_os("NSQL_TRACE_META").is_some() {
            eprintln!(
                "[intel] ctx kind={:?} prefix={:?} aliases={}",
                ctx.kind,
                ctx.prefix,
                ctx.aliases.len()
            );
        }
        if ctx.kind == CtxKind::None {
            self.close();
            return false;
        }
        // 문서 캐시(심볼 · 단어) — 창 방식이거나 자기 감속으로 꺼진 탭은 비운다.
        let (symbols, doc_words): (Vec<(String, SymKind)>, Vec<String>) = if windowed {
            (Vec::new(), Vec::new())
        } else {
            self.outline_for(tab, rev, &|| doc.to_string(), dialect);
            let d = self.docs.get(&tab).expect("cache");
            let words = if self.words_off.contains(&tab) {
                Vec::new()
            } else {
                d.words.clone()
            };
            (d.outline.names(), words)
        };
        let mut cands: Vec<Cand> = Vec::new();
        let show_types = self.cfg.show_types;
        // 컬럼 후보 = `이름 : 타입`(사용자 09-24) · 키 표식은 오른쪽 · 태그 = (객체 id, 순번)으로 상세 카드가 찾는다.
        let push_col = |cands: &mut Vec<Cand>,
                        id: nsql_run::meta::ObjId,
                        c: &nsql_run::meta::ColEntry,
                        names: &Interner| {
            cands.push(Cand {
                text: names.get(c.name).to_string(),
                kind: CandKind::Column,
                detail: if show_types {
                    names.get(c.data_type).to_string()
                } else {
                    String::new()
                },
                source: 1,
                tag: tag_column(id, c.position),
                mark: if show_types {
                    key_marks(c.key, c.nullable)
                } else {
                    String::new()
                },
                // 둘러보기 순서 = 테이블 안 순번(사용자 09-24).
                order: u32::from(c.position),
                qualifier: String::new(),
                layer: 0,
            });
        };
        // ★ 패키지 멤버(09-24 ③): 메타에 그 패키지의 "컬럼"으로 든 멤버 → 함수·프로시저 후보(`NAME()`). FROM 사슬(`FROM sch.pkg.`)에서는
        //   프로시저 제외 · 테이블 함수(`TABLE FUNCTION →`) 층 1 · 그 밖 함수 층 2.
        let push_member = |cands: &mut Vec<Cand>,
                           id: nsql_run::meta::ObjId,
                           c: &nsql_run::meta::ColEntry,
                           names: &Interner,
                           from_chain: bool| {
            let ty = names.get(c.data_type);
            let table_fn = ty.starts_with("TABLE FUNCTION");
            if from_chain && ty.starts_with("PROCEDURE") {
                return;
            }
            cands.push(Cand {
                text: names.get(c.name).to_string(),
                kind: CandKind::Routine,
                detail: if show_types {
                    ty.to_string()
                } else {
                    String::new()
                },
                source: 1,
                tag: tag_column(id, c.position),
                mark: String::new(),
                order: u32::from(c.position),
                qualifier: String::new(),
                layer: if !from_chain {
                    0
                } else if table_fn {
                    1
                } else {
                    2
                },
            });
        };
        let functions = self.cfg.functions;
        match &ctx.kind {
            CtxKind::None => unreachable!(),
            CtxKind::Member { qualifier } => {
                // 1) alias/테이블 → 컬럼.
                let mut resolved = false;
                if let Some(a) = intel::resolve_alias(&ctx.aliases, qualifier) {
                    // CTE/서브쿼리 = 카탈로그 없음 → 아래 문서 단어로(열 이름이 문서 안에 있다).
                    resolved = !a.local;
                    if a.local {
                    } else if let Some(m) = meta {
                        match m.snap.lookup(m.names, a.schema.as_deref(), &a.table) {
                            // ★ 데이터베이스·스키마 같은 "컬럼 없는" 객체가 이름만 같을 때(`FROM BISCM_MS.` · 10-01 ⑭)는 별칭 해석이 아니다 —
                            //   아래 3부 이름 분기로.
                            Some(id)
                                if m.snap.object(id).is_some_and(|o| {
                                    matches!(
                                        o.kind,
                                        ObjectKind::Database
                                            | ObjectKind::Schema
                                            | ObjectKind::DbUser
                                            | ObjectKind::DbRole
                                            | ObjectKind::LinkedServer
                                            | ObjectKind::Login
                                            | ObjectKind::ServerRole
                                            | ObjectKind::Credential
                                            | ObjectKind::CryptoProvider
                                            | ObjectKind::ServerAudit
                                            | ObjectKind::AuditSpec
                                            | ObjectKind::BackupDevice
                                            | ObjectKind::Endpoint
                                            | ObjectKind::ServerTrigger
                                    )
                                }) =>
                            {
                                resolved = false;
                            }
                            Some(id) => match m.snap.columns(id) {
                                ColState::Loaded { cols, .. } => {
                                    for c in cols.iter() {
                                        push_col(&mut cands, id, c, m.names);
                                    }
                                }
                                ColState::Loading => self.loading = true,
                                _ => {
                                    self.loading = true;
                                    self.needs.push(NeedColumns {
                                        schema: a.schema.clone().or_else(|| {
                                            m.snap
                                                .current_schema
                                                .map(|s| m.names.get(s).to_string())
                                        }),
                                        table: a.table.clone(),
                                        urgent: true,
                                    });
                                }
                            },
                            None => {
                                // 카탈로그에 없는 이름(`sys.` · `DBMS_OUTPUT.` · 아직 안 읽은 스키마) — 채움은 요청하되 아래
                                // 정적 표·문서 단어로 이어지게 둔다.
                                resolved = false;
                                self.needs.push(NeedColumns {
                                    schema: a.schema.clone(),
                                    table: a.table.clone(),
                                    urgent: true,
                                });
                            }
                        }
                    }
                }
                // ★ SQL Server 3부 이름(10-01 ⑭ · 사용자 "`Database2.dbo.Table`은 현재 DB의 `dbo.Table`과 다른 객체"):
                //   `DB.` = 그 DB의 스키마 · `DB.스키마.` = 그 DB·스키마의 객체(복합 열쇠 `DB.스키마` 버킷 · 없으면 호스트가 `USE`로
                //   따라가 읽음) · `DB.스키마.테이블.`은 아래 "테이블 이름 직접" 경로가 복합 열쇠로 컬럼을 찾는다.
                let mut db_handled = false;
                if let (Some(m), Some(Dialect::Mssql), false) = (meta, dialect, resolved) {
                    let segs: Vec<&str> = qualifier.split('.').collect();
                    let is_db = m.names.find("").is_some_and(|e| {
                        m.snap
                            .prefix(m.names, e, ObjectKind::Database, "", usize::MAX)
                            .iter()
                            .any(|h| m.names.get(h.name).eq_ignore_ascii_case(segs[0]))
                    });
                    if std::env::var_os("NSQL_TRACE_META").is_some() {
                        eprintln!("[intel] member q={qualifier} segs={} is_db={is_db} resolved={resolved} dbs={}", segs.len(),
                            m.names.find("").map_or(0, |e| m.snap.prefix(m.names, e, ObjectKind::Database, "", usize::MAX).len()));
                    }
                    if is_db && segs.len() <= 2 {
                        db_handled = true;
                        resolved = true;
                        let key = segs.join(".");
                        match m.names.find(&key) {
                            Some(sc) if segs.len() == 1 => {
                                self.note_coverage(m, sc, ObjectKind::Schema);
                                for h in
                                    m.snap
                                        .prefix(m.names, sc, ObjectKind::Schema, "", usize::MAX)
                                {
                                    cands.push(obj_cand(
                                        m,
                                        &h,
                                        ObjectKind::Schema,
                                        dialect,
                                        show_types,
                                    ));
                                }
                            }
                            Some(sc) => {
                                for kind in from_kinds(dialect, self.cfg.routines) {
                                    self.note_coverage(m, sc, kind);
                                    for h in m.snap.prefix(m.names, sc, kind, "", usize::MAX) {
                                        if ctx.from_chain && !from_ok(m, &h, kind, dialect) {
                                            continue;
                                        }
                                        cands.push(obj_cand(m, &h, kind, dialect, show_types));
                                    }
                                }
                            }
                            None => {
                                self.loading_objects = true;
                                push_need(&mut self.need_objects, &key);
                            }
                        }
                    }
                }
                // 2) 스키마 이름 → 그 스키마 객체.
                if let Some(m) = meta {
                    let q = qualifier.rsplit('.').next().unwrap_or(qualifier);
                    let sc_hit = if db_handled {
                        None
                    } else {
                        m.snap
                            .schemas
                            .iter()
                            .copied()
                            .find(|s| m.names.get(*s).eq_ignore_ascii_case(q))
                    };
                    if let Some(sc) = sc_hit {
                        // ★ `스키마.`를 치는 순간 그 스키마를 읽어 캐시(사용자 09-23) — 종류는 FROM 자리와 같은 표(`from_kinds` · 뷰 = 테이블
                        //   레이어 · 함수·패키지·프로시저 = 낮은 레이어 · 09-24). 시퀀스는 트리가 채운 것만 보인다.
                        for kind in from_kinds(dialect, self.cfg.routines) {
                            self.note_coverage(m, sc, kind);
                            for h in m.snap.prefix(m.names, sc, kind, "", usize::MAX) {
                                // FROM 자리의 `스키마.`면 프로시저·스칼라 함수는 뺀다(사용자 09-24 SQL Server 지적).
                                if ctx.from_chain && !from_ok(m, &h, kind, dialect) {
                                    continue;
                                }
                                cands.push(obj_cand(m, &h, kind, dialect, show_types));
                            }
                        }
                        for h in m
                            .snap
                            .prefix(m.names, sc, ObjectKind::Sequence, "", usize::MAX)
                        {
                            cands.push(obj_cand(m, &h, ObjectKind::Sequence, dialect, show_types));
                        }
                        resolved = true;
                    } else if !resolved {
                        // 테이블·패키지 이름을 직접 쓴 경우(alias 없이 · `스키마.이름`이면 그 스키마에서) — 패키지면 멤버(09-24).
                        let (qs, qn) = match qualifier.rsplit_once('.') {
                            Some((s, n)) => (Some(s), n),
                            None => (None, q),
                        };
                        let found = m.snap.lookup(m.names, qs, qn);
                        let is_pkg = found
                            .and_then(|id| m.snap.object(id))
                            .is_some_and(|o| o.kind == ObjectKind::Package);
                        if ctx.from_chain && qualifier.contains('.') && !is_pkg {
                            // FROM 사슬인데 패키지가 아니다(테이블·뷰 뒤 `.`) = 고를 것 없음(§165).
                            resolved = true;
                        } else if let Some(id) = found {
                            resolved = true;
                            match m.snap.columns(id) {
                                ColState::Loaded { cols, .. } => {
                                    for c in cols.iter() {
                                        if is_pkg {
                                            push_member(&mut cands, id, c, m.names, ctx.from_chain);
                                        } else {
                                            push_col(&mut cands, id, c, m.names);
                                        }
                                    }
                                }
                                ColState::Loading => self.loading = true,
                                _ => {
                                    self.loading = true;
                                    self.needs.push(NeedColumns {
                                        schema: qs.map(String::from),
                                        table: qn.to_string(),
                                        urgent: true,
                                    });
                                }
                            }
                        }
                    }
                }
                // 2-b) 사전 스키마(`sys.` · `INFORMATION_SCHEMA.` · `pg_catalog.` …) = 서버에서 읽은 권한 반영 사전 버킷(09-23) —
                //   이름이 `스키마.이름` 꼴이라 앞부분이 맞는 것의 뒷부분을 후보로. 없으면 아래 정적 표.
                if !resolved {
                    if let Some((m, ds)) = meta.and_then(dict_sym) {
                        let want = format!("{}.", qualifier.to_ascii_lowercase());
                        for h in m
                            .snap
                            .prefix(m.names, ds, ObjectKind::View, &want, usize::MAX)
                        {
                            let full = m.names.get(h.name);
                            if full.len() <= want.len() {
                                continue;
                            }
                            cands.push(Cand {
                                text: full[want.len()..].to_string(),
                                kind: CandKind::View,
                                detail: if show_types {
                                    "system".into()
                                } else {
                                    String::new()
                                },
                                source: 4,
                                tag: 0,
                                mark: String::new(),
                                order: 0,
                                qualifier: String::new(),
                                layer: 0,
                            });
                            resolved = true;
                        }
                    }
                }
                // 3) 시스템 패키지(`DBMS_OUTPUT.`) · 사전 객체의 앞부분(`sys.` · `INFORMATION_SCHEMA.` · `pg_catalog.`) — 정적 표(T-178).
                if !resolved && functions {
                    if let Some(p) = builtins::package(dialect, qualifier) {
                        resolved = true;
                        for m in p.members {
                            cands.push(Cand {
                                text: m.name.to_string(),
                                kind: CandKind::Function,
                                detail: if show_types {
                                    m.sig.to_string()
                                } else {
                                    String::new()
                                },
                                source: 4,
                                tag: 0,
                                mark: String::new(),
                                order: 0,
                                qualifier: String::new(),
                                layer: 0,
                            });
                        }
                    }
                    let sys = builtins::system_members(dialect, qualifier);
                    if !sys.is_empty() {
                        resolved = true;
                        for n in sys {
                            cands.push(Cand {
                                text: n.to_string(),
                                kind: CandKind::View,
                                detail: if show_types {
                                    "system".into()
                                } else {
                                    String::new()
                                },
                                source: 4,
                                tag: 0,
                                mark: String::new(),
                                order: 0,
                                qualifier: String::new(),
                                layer: 0,
                            });
                        }
                    }
                }
                if !resolved && self.cfg.doc_words {
                    for w in &doc_words {
                        cands.push(Cand {
                            text: w.clone(),
                            kind: CandKind::Word,
                            detail: String::new(),
                            source: 6,
                            tag: 0,
                            mark: String::new(),
                            order: 0,
                            qualifier: String::new(),
                            layer: 0,
                        });
                    }
                }
            }
            // ★ 특정 종류만(10-01 ⑩): 키워드·문서 낱말 없이 메타의 그 종류 + 스키마.
            // ★ T-317: 사용자 루틴 인자 — 메타의 `ArgState`(없으면 요청 + 불러오는 중) · 이미 쓴 `@이름` 제외.
            CtxKind::ExecArgs { routine } => {
                if let Some(m) = meta {
                    let (schema, name) = routine
                        .rsplit_once('.')
                        .map_or((None, routine.as_str()), |(s, n)| (Some(s), n));
                    let cur = m.snap.current_schema.map(|s| m.names.get(s).to_string());
                    if let Some(id) =
                        m.snap
                            .lookup_resolvable_from(m.names, schema, cur.as_deref(), name)
                    {
                        match m.snap.args(id) {
                            nsql_run::meta::ArgState::Loaded(list) => {
                                let used = param_names_in(
                                    &doc[ctx.statement.start
                                        ..ctx.replace.start.max(ctx.statement.start)],
                                );
                                for (i, a) in list.iter().enumerate() {
                                    // ★ T-SQL 명명 인자 = `@이름 = `(카탈로그는 `@`를 뗀 이름 · 사용자 10-08 "DBMS 특징에 맞게").
                                    let n = m.names.get(a.name);
                                    let shown = if n.starts_with('@') {
                                        n.to_string()
                                    } else {
                                        format!("@{n}")
                                    };
                                    if used.iter().any(|u| u.eq_ignore_ascii_case(&shown)) {
                                        continue;
                                    }
                                    cands.push(Cand {
                                        text: format!("{shown} = "),
                                        kind: CandKind::Variable,
                                        detail: if show_types {
                                            m.names.get(a.detail).to_string()
                                        } else {
                                            String::new()
                                        },
                                        source: 4,
                                        tag: 0,
                                        mark: String::new(),
                                        order: i as u32,
                                        qualifier: String::new(),
                                        layer: 0,
                                    });
                                }
                            }
                            nsql_run::meta::ArgState::Loading => self.loading = true,
                            nsql_run::meta::ArgState::Unknown => {
                                self.need_args.push(id);
                                self.loading = true;
                            }
                        }
                    }
                }
            }
            CtxKind::Want(intel::Want::ProcParam(p)) => {
                // ★ T-316: 시스템 프로시저 파라미터 — 문장 안에 이미 쓴 `@이름`은 뺀다(메타 불필요).
                let used: Vec<String> = param_names_in(
                    &doc[ctx.statement.start..ctx.replace.start.max(ctx.statement.start)],
                );
                if let Some(sp) = nsql_script::builtins::proc_params(dialect, p) {
                    for (i, (name, ty)) in sp.params.iter().enumerate() {
                        if used.iter().any(|u| u.eq_ignore_ascii_case(name)) {
                            continue;
                        }
                        cands.push(Cand {
                            // T-SQL 명명 인자 꼴 `@이름 = `(사용자 10-08).
                            text: format!("{name} = "),
                            kind: CandKind::Variable,
                            detail: if show_types {
                                (*ty).to_string()
                            } else {
                                String::new()
                            },
                            source: 4,
                            tag: 0,
                            mark: String::new(),
                            order: i as u32,
                            qualifier: String::new(),
                            layer: 0,
                        });
                    }
                }
            }
            CtxKind::Want(w) => {
                self.want_cands(*w, meta, dialect, show_types, &mut cands);
            }
            CtxKind::Relation => {
                if let Some(m) = meta {
                    if let Some(cur) = m.snap.current_schema {
                        // ★ 현재 스키마의 관계 객체 = 접두 없이 바로(사용자 09-23). 버킷이 아직 없으면 호스트에 채움을 요청하고
                        //   "불러오는 중"을 보인다 · 상한 없이 다 넣고 랭킹이 자른다(종전 = 이름순 앞 200개만 → `M4S_*`가 잘렸다).
                        //   ★ 뷰·구체화 뷰·시노님은 테이블과 같은 레이어 · 함수·패키지·프로시저는 낮은 레이어(`from_kinds` · 사용자 09-24).
                        for kind in from_kinds(dialect, self.cfg.routines) {
                            self.note_coverage(m, cur, kind);
                            for h in m.snap.prefix(m.names, cur, kind, "", usize::MAX) {
                                if !from_ok(m, &h, kind, dialect) {
                                    continue;
                                }
                                cands.push(obj_cand(m, &h, kind, dialect, show_types));
                            }
                        }
                    }
                    for s in &m.snap.schemas {
                        cands.push(Cand {
                            text: m.names.get(*s).to_string(),
                            kind: CandKind::Schema,
                            detail: if show_types {
                                "schema".into()
                            } else {
                                String::new()
                            },
                            source: 4,
                            tag: 0,
                            mark: String::new(),
                            order: 0,
                            qualifier: String::new(),
                            layer: 0,
                        });
                    }
                }
                for a in ctx.aliases.iter().filter(|a| a.local && a.alias == a.table) {
                    cands.push(Cand {
                        text: a.alias.clone(),
                        kind: CandKind::Table,
                        detail: if show_types {
                            "cte".into()
                        } else {
                            String::new()
                        },
                        source: 2,
                        tag: 0,
                        mark: String::new(),
                        order: 0,
                        qualifier: String::new(),
                        layer: 0,
                    });
                }
                for (name, k) in &symbols {
                    if *k == SymKind::Object || *k == SymKind::Cte {
                        cands.push(Cand {
                            text: name.clone(),
                            kind: CandKind::Symbol,
                            detail: if show_types {
                                k.label().to_string()
                            } else {
                                String::new()
                            },
                            source: 3,
                            tag: 0,
                            mark: String::new(),
                            order: 0,
                            qualifier: String::new(),
                            layer: 0,
                        });
                    }
                }
                // 사전 객체(`ALL_TABLES` · `V$SESSION` · `sys.tables` · `pg_catalog.pg_class`) — ★ 접속 뒤에는 서버에서 읽은
                //   **권한 반영** 버킷(`DICT_SCHEMA` · 사용자 09-23 "권한에 맞춰 ALL_/DBA_") · 그 전(또는 아직 못 읽음)에는 정적 표(T-178).
                let dict_loaded = meta
                    .and_then(dict_sym)
                    .is_some_and(|(m, ds)| m.snap.coverage(ds, ObjectKind::View).has_list());
                if let Some((m, ds)) = meta.and_then(dict_sym) {
                    let cov = m.snap.coverage(ds, ObjectKind::View);
                    // 읽은 것(낡았어도)은 후보로 · 없거나 낡았으면 (다시) 읽기 청함.
                    if cov.has_list() {
                        for h in m.snap.prefix(m.names, ds, ObjectKind::View, "", usize::MAX) {
                            cands.push(Cand {
                                text: m.names.get(h.name).to_string(),
                                kind: CandKind::View,
                                detail: if show_types {
                                    "system".into()
                                } else {
                                    String::new()
                                },
                                source: 4,
                                tag: 0,
                                mark: String::new(),
                                order: 0,
                                qualifier: String::new(),
                                // 사전 뷰(`ALL_`·`DBA_`·`CDB_`·`V$`·`GV$` — 수백~수천)는 스키마 자체 객체 **아래 층**: 빈 접두에서는
                                // 뒤로 · 접두를 치면 등급으로 올라온다(사용자 09-24 "이런 테이블들은 뭐야").
                                layer: 1,
                            });
                        }
                    }
                    if matches!(cov, Coverage::Missing | Coverage::Stale { .. }) {
                        push_need(&mut self.need_objects, nsql_catalog::DICT_SCHEMA);
                    }
                } else if meta.is_some() {
                    push_need(&mut self.need_objects, nsql_catalog::DICT_SCHEMA);
                }
                if functions && !dict_loaded {
                    for o in builtins::system_objects(dialect) {
                        cands.push(Cand {
                            text: (*o).to_string(),
                            kind: CandKind::View,
                            detail: if show_types {
                                "system".into()
                            } else {
                                String::new()
                            },
                            source: 4,
                            tag: 0,
                            mark: String::new(),
                            order: 0,
                            qualifier: String::new(),
                            layer: 1,
                        });
                    }
                }
            }
            CtxKind::Expr | CtxKind::Start => {
                // ★ 이름 지정 인자(사용자 10-08 "Argument 이름에 직접 지정하는 문법에서도 자동완성" · T-317 확장): 괄호 주인이 메타의
                //   루틴이고 캐럿이 인자 시작(`(`·`,` 뒤)이면 `이름 => ` 조각(Oracle·PG 명명 표기 · SQL Server는 괄호 없는 `@이름` = T-317) ·
                //   이미 `이름 =>`로 적은 인자는 제외 · 인자를 모르면 요청 + 불러오는 중.
                if ctx.kind == CtxKind::Expr
                    && matches!(dialect, Some(Dialect::Oracle | Dialect::Postgres))
                {
                    if let (Some(owner), Some(m)) = (ctx.paren_owner.as_deref(), meta) {
                        let head =
                            &doc[ctx.statement.start..ctx.replace.start.max(ctx.statement.start)];
                        if named_arg_start(head) {
                            let (schema, name) = owner
                                .rsplit_once('.')
                                .map_or((None, owner), |(sc, n)| (Some(sc), n));
                            let cur = m.snap.current_schema.map(|c| m.names.get(c).to_string());
                            let id = m
                                .snap
                                .lookup_resolvable_from(m.names, schema, cur.as_deref(), name)
                                .filter(|&id| {
                                    m.snap.object(id).is_some_and(|o| o.kind.is_routine())
                                });
                            if let Some(id) = id {
                                match m.snap.args(id) {
                                    nsql_run::meta::ArgState::Loaded(list) => {
                                        let used = named_args_used(head);
                                        for (i, a) in list.iter().enumerate() {
                                            // 카탈로그는 이름을 대문자로 접는다(서명 대조용) — PG는 소문자 식별자가 자연스러워 소문자로(E2E 10-08).
                                            let raw = m.names.get(a.name);
                                            let n = if dialect == Some(Dialect::Postgres) {
                                                raw.to_lowercase()
                                            } else {
                                                raw.to_string()
                                            };
                                            if used.iter().any(|u| u.eq_ignore_ascii_case(&n)) {
                                                continue;
                                            }
                                            cands.push(Cand {
                                                text: format!("{n} => "),
                                                kind: CandKind::Variable,
                                                detail: if show_types {
                                                    m.names.get(a.detail).to_string()
                                                } else {
                                                    String::new()
                                                },
                                                source: 1,
                                                tag: 0,
                                                mark: String::new(),
                                                order: i as u32,
                                                qualifier: String::new(),
                                                layer: 0,
                                            });
                                        }
                                    }
                                    nsql_run::meta::ArgState::Loading => self.loading = true,
                                    nsql_run::meta::ArgState::Unknown => {
                                        self.need_args.push(id);
                                        self.loading = true;
                                    }
                                }
                            }
                        }
                    }
                }
                // ★ `*` / `A.*` 뒤 = "모든 컬럼 (N)" 조각(사용자 09-24): `A.*` = 그 별칭의 컬럼 · bare `*` = FROM의 모든 테이블 컬럼(둘 이상이거나
                //   별칭을 썼으면 `A.컬럼` · 하나뿐이고 별칭이 없으면 이름만). 안 읽은 테이블은 급한 채움 요청 + "불러오는 중".
                if ctx.kind == CtxKind::Expr && self.cfg.insert_columns {
                    if let (Some(star), Some(m)) = (ctx.star.as_ref(), meta) {
                        let targets: Vec<&intel::Alias> = ctx
                            .aliases
                            .iter()
                            .filter(|a| !a.local)
                            .filter(|a| {
                                star.qualifier
                                    .as_deref()
                                    .is_none_or(|q| a.alias.eq_ignore_ascii_case(q))
                            })
                            .collect();
                        let qualify = star.qualifier.is_some()
                            || targets.len() > 1
                            || targets
                                .first()
                                .is_some_and(|a| !a.alias.eq_ignore_ascii_case(&a.table));
                        let mut parts: Vec<String> = Vec::new();
                        let mut ready = !targets.is_empty();
                        for a in &targets {
                            match m.snap.lookup(m.names, a.schema.as_deref(), &a.table) {
                                Some(id) => match m.snap.columns(id) {
                                    ColState::Loaded { cols, .. } => {
                                        for c in cols.iter() {
                                            let name = m.names.get(c.name);
                                            parts.push(if qualify {
                                                format!("{}.{name}", a.alias)
                                            } else {
                                                name.to_string()
                                            });
                                        }
                                    }
                                    ColState::Loading => {
                                        ready = false;
                                        self.loading = true;
                                    }
                                    ColState::Error(_) => ready = false,
                                    ColState::Unknown => {
                                        ready = false;
                                        self.loading = true;
                                        self.needs.push(NeedColumns {
                                            schema: a.schema.clone(),
                                            table: a.table.clone(),
                                            urgent: true,
                                        });
                                    }
                                },
                                None => {
                                    ready = false;
                                    self.needs.push(NeedColumns {
                                        schema: a.schema.clone(),
                                        table: a.table.clone(),
                                        urgent: true,
                                    });
                                }
                            }
                        }
                        if ready && !parts.is_empty() {
                            // 구분 = 쉼표 + 공백(Space/Tab · `intel.star_comma_space`) · "여러 줄"이면 다음 줄 **맨 앞에 쉼표**(들여쓰기 없음 ·
                            //   사용자 09-24 "A / , B / , C") · 기본 = "한 줄"(`A, B, C`).
                            let ws = if self.cfg.star_tab.unwrap_or(self.editor_tab) {
                                "\t"
                            } else {
                                " "
                            };
                            let sep = if self.cfg.star_lines {
                                format!("\n,{ws}")
                            } else {
                                format!(",{ws}")
                            };
                            cands.push(Cand {
                                text: parts.join(&sep),
                                kind: CandKind::Snippet,
                                detail: nsql_i18n::tf(
                                    nsql_i18n::Msg::IntelAllColumnsN,
                                    &[&parts.len().to_string()],
                                ),
                                source: 0,
                                tag: 0,
                                mark: String::new(),
                                order: 0,
                                qualifier: String::new(),
                                layer: 0,
                            });
                        }
                    }
                }
                if ctx.kind == CtxKind::Expr && self.cfg.join_fk && ctx.prefix.is_empty() {
                    // ★ `JOIN 테이블 [별칭] ON` 바로 뒤 = 외래 키로 만든 조인 조건을 첫 후보로(T-178 · 지연 로딩: 제약은
                    //   이때 처음 읽는다 — 이미 있으면 즉시 · 없으면 "불러오는 중" 뒤 다시 그린다).
                    if let (Some(target), Some(m)) =
                        (join_on_target(&doc[..ctx.replace.start]), meta)
                    {
                        let mut sides: Vec<JoinSide> = Vec::new();
                        let mut joined: Option<usize> = None;
                        for a in ctx.aliases.iter().filter(|a| !a.local) {
                            let Some(id) = m.snap.lookup(m.names, a.schema.as_deref(), &a.table)
                            else {
                                continue;
                            };
                            match m.snap.detail(id) {
                                nsql_run::meta::DetailState::Loaded { detail, .. } => {
                                    if joined.is_none()
                                        && (a.alias.eq_ignore_ascii_case(&target)
                                            || a.table.eq_ignore_ascii_case(&target))
                                    {
                                        joined = Some(sides.len());
                                    }
                                    sides.push(JoinSide {
                                        alias: a.alias.clone(),
                                        table: a.table.clone(),
                                        keys: detail
                                            .keys
                                            .iter()
                                            .filter(|k| matches!(k.kind, 'P' | 'R'))
                                            .map(|k| JoinKey {
                                                name: m.names.get(k.name).to_string(),
                                                kind: k.kind,
                                                cols: k
                                                    .cols
                                                    .iter()
                                                    .map(|c| m.names.get(*c).to_string())
                                                    .collect(),
                                                ref_table: k
                                                    .ref_table
                                                    .map(|t| m.names.get(t).to_string()),
                                                ref_cols: k
                                                    .ref_cols
                                                    .iter()
                                                    .map(|c| m.names.get(*c).to_string())
                                                    .collect(),
                                            })
                                            .collect(),
                                    });
                                }
                                nsql_run::meta::DetailState::Loading => self.loading = true,
                                nsql_run::meta::DetailState::Unknown => {
                                    self.loading = true;
                                    self.need_details.push(id);
                                }
                            }
                        }
                        if let Some(j) = joined {
                            for (n, (cond, fk)) in fk_join_conds(&sides, j).into_iter().enumerate()
                            {
                                cands.push(Cand {
                                    text: cond,
                                    kind: CandKind::Snippet,
                                    detail: format!("FK {fk}"),
                                    source: 0,
                                    tag: 0,
                                    mark: String::new(),
                                    order: n as u32,
                                    qualifier: String::new(),
                                    layer: 0,
                                });
                            }
                        }
                    }
                }
                if ctx.kind == CtxKind::Expr
                    && self.cfg.insert_columns
                    && ctx.paren_into
                    && ctx.prefix.is_empty()
                {
                    // `INSERT INTO t (` 바로 뒤 = 그 테이블의 컬럼 전체를 한 조각으로(첫 후보 · T-178).
                    if let (Some(owner), Some(m)) = (ctx.paren_owner.as_deref(), meta) {
                        let (schema, table) = match intel::resolve_alias(&ctx.aliases, owner) {
                            Some(a) if !a.local => (a.schema.clone(), a.table.clone()),
                            _ => match owner.rsplit_once('.') {
                                Some((sc, t)) => (Some(sc.to_string()), t.to_string()),
                                None => (None, owner.to_string()),
                            },
                        };
                        match m.snap.lookup(m.names, schema.as_deref(), &table) {
                            Some(id) => match m.snap.columns(id) {
                                ColState::Loaded { cols, .. } if !cols.is_empty() => {
                                    let list: Vec<&str> =
                                        cols.iter().map(|c| m.names.get(c.name)).collect();
                                    cands.push(Cand {
                                        text: list.join(", "),
                                        kind: CandKind::Snippet,
                                        detail: nsql_i18n::tf(
                                            nsql_i18n::Msg::IntelAllColumnsN,
                                            &[&list.len().to_string()],
                                        ),
                                        source: 0,
                                        tag: 0,
                                        mark: String::new(),
                                        order: 0,
                                        qualifier: String::new(),
                                        layer: 0,
                                    });
                                }
                                ColState::Loading => self.loading = true,
                                ColState::Loaded { .. } | ColState::Error(_) => {}
                                ColState::Unknown => {
                                    self.loading = true;
                                    self.needs.push(NeedColumns {
                                        schema,
                                        table,
                                        urgent: true,
                                    });
                                }
                            },
                            None => self.needs.push(NeedColumns {
                                schema,
                                table,
                                urgent: true,
                            }),
                        }
                    }
                }
                if ctx.kind == CtxKind::Expr {
                    // 문장 alias 컬럼(로드된 것만) + alias 이름.
                    for (ai, a) in ctx.aliases.iter().enumerate() {
                        cands.push(Cand {
                            text: a.alias.clone(),
                            kind: CandKind::Alias,
                            detail: if show_types {
                                if a.local {
                                    "cte/subquery".into()
                                } else {
                                    a.table.clone()
                                }
                            } else {
                                String::new()
                            },
                            source: 2,
                            tag: 0,
                            mark: String::new(),
                            order: 0,
                            qualifier: String::new(),
                            layer: 0,
                        });
                        if a.local {
                            continue;
                        }
                        // ★ 접두 없이 고른 컬럼(JOIN 다중 테이블 · 사용자 09-24): 모든 alias 테이블의 컬럼을 보이고(오른쪽 = alias ·
                        //   FROM 순서 → 순번), 확정 때 `A.컬럼`(Alt = 컬럼만 · `intel.qualify_columns`). 아직 안 읽은 테이블은 채움 요청.
                        if let Some(m) = meta {
                            match m.snap.lookup(m.names, a.schema.as_deref(), &a.table) {
                                Some(id) => match m.snap.columns(id) {
                                    ColState::Loaded { cols, .. } => {
                                        let base = ai as u32 * 1000;
                                        for c in cols.iter() {
                                            push_col(&mut cands, id, c, m.names);
                                            if let Some(last) = cands.last_mut() {
                                                last.qualifier = a.alias.clone();
                                                last.order += base;
                                            }
                                        }
                                    }
                                    ColState::Loading => self.loading = true,
                                    ColState::Unknown => {
                                        self.loading = true;
                                        self.needs.push(NeedColumns {
                                            schema: a.schema.clone().or_else(|| {
                                                m.snap
                                                    .current_schema
                                                    .map(|s| m.names.get(s).to_string())
                                            }),
                                            table: a.table.clone(),
                                            urgent: true,
                                        });
                                    }
                                    ColState::Error(_) => {}
                                },
                                None => self.needs.push(NeedColumns {
                                    schema: a.schema.clone(),
                                    table: a.table.clone(),
                                    urgent: true,
                                }),
                            }
                        }
                    }
                    // 내장 함수(시그니처) + 시스템 패키지 이름 — 정적 표(T-178 · `intel.functions`).
                    if functions {
                        for b in builtins::functions(dialect) {
                            cands.push(Cand {
                                text: b.name.to_string(),
                                kind: CandKind::Function,
                                detail: if show_types {
                                    b.sig.to_string()
                                } else {
                                    String::new()
                                },
                                source: 4,
                                tag: 0,
                                mark: String::new(),
                                order: 0,
                                qualifier: String::new(),
                                layer: 0,
                            });
                        }
                        for p in builtins::packages(dialect) {
                            cands.push(Cand {
                                text: p.name.to_string(),
                                kind: CandKind::Package,
                                detail: if show_types {
                                    "package".into()
                                } else {
                                    String::new()
                                },
                                source: 4,
                                tag: 0,
                                mark: String::new(),
                                order: 0,
                                qualifier: String::new(),
                                layer: 0,
                            });
                        }
                    }
                    // ★ 사용자 정의 함수·패키지(사용자 10-09 "사용자 정의 함수 보완"): 식 자리에서 접두 없이 — 현재 스키마의 메타
                    //   버킷(L1 이름)에서 · 없으면 채움 요청 · 레이어는 FROM 자리와 같은 `obj_cand`(함수·패키지 = 2).
                    if let Some(m) = meta {
                        if let Some(cur) = m.snap.current_schema {
                            for kind in [ObjectKind::Function, ObjectKind::Package] {
                                if dialect
                                    .is_some_and(|d| !nsql_catalog::kinds_for(d).contains(&kind))
                                {
                                    continue;
                                }
                                self.note_coverage_kind(m, cur, kind);
                                for h in m.snap.prefix(m.names, cur, kind, "", usize::MAX) {
                                    cands.push(obj_cand(m, &h, kind, dialect, show_types));
                                }
                            }
                        }
                    }
                }
                for (name, k) in &symbols {
                    cands.push(Cand {
                        text: name.clone(),
                        kind: match k {
                            SymKind::Define | SymKind::Variable | SymKind::Declared => {
                                CandKind::Variable
                            }
                            SymKind::Procedure | SymKind::Function => CandKind::Routine,
                            _ => CandKind::Symbol,
                        },
                        detail: if show_types {
                            k.label().to_string()
                        } else {
                            String::new()
                        },
                        source: 3,
                        tag: 0,
                        mark: String::new(),
                        order: 0,
                        qualifier: String::new(),
                        layer: 0,
                    });
                }
                if self.cfg.keywords {
                    // ★ 문법 절 기반(09-24 · nsql-script `grammar` · `SELECT avg` 자리에 `HAVING` 없음): 문장 시작 = `[start]` · 절을 알면
                    //   그 절의 `next`만 · 모르는 자리(함수 괄호 안 · 낯선 절) = 공통 + 방언 키워드 전체.
                    let g = nsql_script::grammar::for_dialect(dialect);
                    let kws: Vec<String> = match (&ctx.kind, ctx.clause.as_deref()) {
                        _ if !self.cfg.grammar => {
                            intel::keywords_for(dialect).map(String::from).collect()
                        }
                        (CtxKind::Start, _) => g.start().to_vec(),
                        (_, Some(cl)) => match g.next(cl) {
                            Some(n) => n.to_vec(),
                            None => intel::keywords_for(dialect).map(String::from).collect(),
                        },
                        _ => intel::keywords_for(dialect).map(String::from).collect(),
                    };
                    for (i, k) in kws.iter().enumerate() {
                        cands.push(Cand {
                            text: k.clone(),
                            kind: CandKind::Keyword,
                            detail: String::new(),
                            source: 5,
                            tag: 0,
                            mark: String::new(),
                            // 키워드는 표 순서(자주 쓰는 것부터).
                            order: i as u32,
                            qualifier: String::new(),
                            layer: 0,
                        });
                    }
                }
                if self.cfg.doc_words {
                    for w in &doc_words {
                        cands.push(Cand {
                            text: w.clone(),
                            kind: CandKind::Word,
                            detail: String::new(),
                            source: 6,
                            tag: 0,
                            mark: String::new(),
                            order: 0,
                            qualifier: String::new(),
                            layer: 0,
                        });
                    }
                }
            }
        }
        // 접두를 뺀 문서 단어(자기 자신)는 후보에서 뺀다.
        let prefix = ctx.prefix.clone();
        cands.retain(|c| !(c.kind == CandKind::Word && c.text.eq_ignore_ascii_case(&prefix)));
        // ★ FROM 절의 나머지 테이블 컬럼은 **이어서 백그라운드로** 선적재(사용자 09-23 "alias 대상부터 · 이후 다른 테이블도
        //   순차로 · 메인 창에 영향 없이") — 워커 큐에서 급한 것(지금 팝업 대상) 뒤에 선다.
        if let Some(m) = meta {
            for a in ctx.aliases.iter().filter(|a| !a.local) {
                if self
                    .needs
                    .iter()
                    .any(|n| n.table.eq_ignore_ascii_case(&a.table))
                {
                    continue;
                }
                let Some(id) = m.snap.lookup(m.names, a.schema.as_deref(), &a.table) else {
                    continue;
                };
                if matches!(m.snap.columns(id), ColState::Unknown) {
                    self.needs.push(NeedColumns {
                        schema: a
                            .schema
                            .clone()
                            .or_else(|| m.snap.current_schema.map(|s| m.names.get(s).to_string())),
                        table: a.table.clone(),
                        urgent: false,
                    });
                }
            }
        }
        let mru: &[String] = if self.cfg.recent { &self.mru } else { &[] };
        // ★ `*`/`A.*` 뒤는 "모든 컬럼" 조각 말고는 고를 것이 없다(사용자 09-24) — 다른 후보를 전부 걷어낸다.
        if ctx.star.is_some() {
            cands.retain(|c| c.kind == CandKind::Snippet);
        }
        // ★ 한 세트 + 창 투영(76 §13 · T-196): 전부 랭킹해 `max_total`까지 들고, 팝업에는 첫 페이지(`max_items`)만 조립한다 —
        //   끝까지 스크롤하면 `extend`가 다음 페이지를 이어 붙인다(그리드 43 §5와 같은 사상).
        let mut ranked = intel::rank(cands, &prefix, self.cfg.mode, mru, usize::MAX);
        ranked.truncate(self.cfg.max_total);
        // ★ 건너편 컬럼(사용자 09-29): `B.PROJECT_CD = A.|` · UPDATE/MERGE `SET T.X = S.|` · MERGE ON — 연산자 왼쪽 이름과 같은
        //   컬럼이 있으면 1순위(없으면 평소대로 · 안정 정렬이라 나머지 순서는 그대로).
        let mut peer_hit = false;
        if matches!(ctx.kind, CtxKind::Member { .. } | CtxKind::Expr) {
            if let Some(peer) = peer_column(doc, ctx.replace.start) {
                let is_peer =
                    |c: &Cand| c.kind == CandKind::Column && c.text.eq_ignore_ascii_case(&peer);
                peer_hit = ranked.iter().any(is_peer);
                ranked.sort_by_key(|c| !is_peer(c));
            }
        }
        // 조각(컬럼 목록)은 MRU와 무관하게 맨 위(안정 정렬).
        ranked.sort_by_key(|c| c.kind != CandKind::Snippet);
        if ranked.is_empty() && !self.loading && !self.loading_objects {
            self.close();
            return false;
        }
        self.icon_kinds = if self.cfg.icons {
            ranked.iter().map(|c| icon_kind_of(c, meta)).collect()
        } else {
            Vec::new()
        };
        self.prefix_now = prefix.clone();
        self.cands = ranked;
        self.shown = self.cfg.max_items.min(self.cands.len());
        let items = self.build_items();
        // 새 목록 = 카드 대상 처음부터(첫 대상은 즉시).
        self.card_want = None;
        self.card_shown = None;
        self.ctx = Some(ctx);
        self.tab = Some(tab);
        self.dialect = dialect;
        // ★ 앵커 = **접두가 시작한 글자**의 아래 기준선(사용자 09-23 "처음 자리를 유지 · 필요하면 최소 이동"): 같은 낱말을
        //   이어 치는 동안은 같은 점이라 팝업이 움직이지 않고, 접두가 새로 시작하거나 줄이 바뀔 때만 옮겨진다. 좌표를 못 받으면
        //   (다시 그리기 전) 캐럿 좌표 → 열린 팝업은 제자리 → 창 원점(안전망).
        let p = match point_of(prefix_start).or(anchor) {
            Some(p) => p,
            None if self.menu.is_open() => self.last_anchor.unwrap_or(Point {
                x: host.x,
                y: host.y,
            }),
            // ★ 아직 안 그려 좌표가 없으면(트리거 글자 직후) 원점에 열지 않고 **다음 틱(그린 뒤)** 에 다시(09-24 —
            //   자동으로 뜬 팝업이 옛 배치 때문에 다음 줄에 붙던 결함 · 수동 Ctrl+Space와 같은 자리).
            None => {
                self.due = Some(Instant::now() + Duration::from_millis(16));
                return false;
            }
        };
        self.last_anchor = Some(p);
        self.menu.set_scale(scale);
        self.menu.set_max_rows(Some(self.cfg.rows));
        // 항목이 적어도 같은 높이(사용자 09-24) — 옆 상세 카드가 이 높이를 쓴다.
        self.menu.set_min_rows(Some(self.cfg.rows));
        self.menu.set_max_width(Some(self.cfg.popup_max_w));
        // 클릭 = 선택(카드) · 더블 클릭/Enter = 확정(사용자 09-24).
        self.menu.set_click_selects(true);
        self.text_w = (280.0 * scale) as i32;
        self.menu.open_at(p.x, p.y, items, host, self.text_w);
        // ★ 건너편 컬럼이 있으면 그 행을 **선택 상태**로(사용자 09-29 "순서만 말고 바로 선택") — Enter/Tab 한 번에 확정 · 카드도 그 항목.
        if peer_hit {
            let i = self
                .cands
                .iter()
                .position(|c| c.kind == CandKind::Column)
                .unwrap_or(0);
            self.menu.select(i);
        }
        self.due = None;
        let ms = t0.elapsed().as_millis();
        // 예산 비교는 Duration으로(정수 ms 비교는 1 ms 아래를 못 본다 · 예산 0 = 늘 초과).
        let over = t0.elapsed() > Duration::from_millis(self.cfg.budget_ms);
        self.over_budget = over.then_some((self.cands.len(), ms));
        // 자기 감속: 연속 2회 초과 → 그 탭의 문서 낱말 끔(다음 요청부터) · 호스트에 한 번 알림.
        if over {
            self.over_streak = self.over_streak.saturating_add(1);
            if self.over_streak >= 2 && self.words_off.insert(tab) {
                self.degraded = Some(tab);
            }
        } else {
            self.over_streak = 0;
        }
        true
    }

    /// 자기 감속이 막 일어난 탭(한 번만).
    pub(crate) fn take_degraded(&mut self) -> Option<u64> {
        self.degraded.take()
    }

    /// 팝업 항목 조립 — `cands[..shown]` + 끝 안내("N개 더" · "불러오는 중"). 요청·페이지 이어 붙이기가 같은 길을 쓴다.
    fn build_items(&mut self) -> Vec<CtxItem> {
        let prefix = self.prefix_now.clone();
        let use_icons = self.cfg.icons;
        let mut items: Vec<CtxItem> = Vec::with_capacity(self.shown + 2);
        for i in 0..self.shown {
            let c = &self.cands[i];
            // 일치 강조(사용자 09-24): 전체 일치 = 굵게+파랑 · 부분 일치 = 일치 글자만 강조색(바이트 범위).
            let exact = !prefix.is_empty() && c.text.eq_ignore_ascii_case(&prefix);
            let marks: Vec<std::ops::Range<usize>> = if exact || prefix.is_empty() {
                Vec::new()
            } else {
                let idx: Vec<(usize, char)> = c.text.char_indices().collect();
                let mut out: Vec<std::ops::Range<usize>> = Vec::new();
                for p in intel::match_positions(&c.text, &prefix) {
                    if let Some(&(b, ch)) = idx.get(p) {
                        let e = b + ch.len_utf8();
                        match out.last_mut() {
                            Some(last) if last.end == b => last.end = e,
                            _ => out.push(b..e),
                        }
                    }
                }
                out
            };
            // 컬럼 = 이름(본문색) + ` : 타입`(흐리게) · 조각 = 이름("모든 컬럼 (N)") + 앞 셋 미리보기 · 그 밖 = 이름 + 오른쪽 설명.
            let item = if c.kind == CandKind::Column && !c.detail.is_empty() {
                CtxItem::item(format!("intel:{i}"), c.text.clone())
                    .with_sub(format!(" : {}", c.detail))
                    .with_shortcut(c.qualifier.clone())
            } else if c.kind == CandKind::Snippet {
                let names: Vec<&str> = c
                    .text
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect();
                let preview = if names.len() > 3 {
                    format!("{}, {}, {}, …", names[0], names[1], names[2])
                } else {
                    names.join(", ")
                };
                CtxItem::item(format!("intel:{i}"), c.detail.clone()).with_shortcut(preview)
            } else {
                CtxItem::item(format!("intel:{i}"), c.text.clone()).with_shortcut(c.detail.clone())
            };
            let icon = if use_icons {
                self.icon_kinds
                    .get(i)
                    .copied()
                    .flatten()
                    .map(|k| menu_icon(&mut self.icons, k))
            } else {
                None
            };
            items.push(item.with_emphasis(exact).with_marks(marks).with_icon(icon));
        }
        let cut = self.cands.len().saturating_sub(self.shown);
        if cut > 0 {
            items.push(CtxItem::maybe(
                "intel:more",
                nsql_i18n::tf(nsql_i18n::Msg::StIntelMore, &[&cut.to_string()]),
                false,
            ));
        }
        if self.loading || self.loading_objects {
            items.push(CtxItem::maybe(
                "intel:loading",
                nsql_i18n::t(if self.loading {
                    nsql_i18n::Msg::StIntelLoading
                } else {
                    nsql_i18n::Msg::StIntelLoadingObjects
                }),
                false,
            ));
        }
        items
    }

    /// ★ 다음 페이지 이어 붙이기(끝 도달 신호 뒤 · 호스트) — 붙였으면 `true`. 열려 있지 않거나 더 없으면 `false`.
    pub(crate) fn extend(&mut self) -> bool {
        if !self.is_open() || self.shown >= self.cands.len() {
            return false;
        }
        self.shown = (self.shown + self.cfg.max_items).min(self.cands.len());
        let items = self.build_items();
        self.menu.replace_items(items, self.text_w);
        true
    }

    /// End = 남은 페이지 전부(그리드 End와 같은 뜻 · D-206).
    pub(crate) fn extend_all(&mut self) {
        if self.is_open() && self.shown < self.cands.len() {
            self.shown = self.cands.len();
            let items = self.build_items();
            self.menu.replace_items(items, self.text_w);
        }
    }

    /// 마지막 요청이 예산을 넘겼으면 (후보 수, ms) — 한 번만 돌려준다(호스트가 개발자 로그에).
    pub(crate) fn take_over_budget(&mut self) -> Option<(usize, u128)> {
        self.over_budget.take()
    }

    /// 시그니처 도움(`intel.signature_help` · T-178): 캐럿을 감싸는 안 닫힌 `(`의 주인이 내장 함수·패키지 멤버면 그 시그니처.
    /// ★ 캐럿이 루틴 괄호 안의 **인자 시작**(`(`·`,` 뒤)인가(Oracle·PG · 이름 지정 인자 자동 열림 · 사용자 10-08 "두 번째 인자도").
    pub(crate) fn named_arg_site(&self, doc: &str, caret: usize, dialect: Option<Dialect>) -> bool {
        if !self.cfg.enabled || !matches!(dialect, Some(Dialect::Oracle | Dialect::Postgres)) {
            return false;
        }
        let ctx = intel::context_at(doc, caret, dialect);
        ctx.kind == CtxKind::Expr
            && ctx.paren_owner.is_some()
            && named_arg_start(
                &doc[ctx.statement.start..ctx.replace.start.max(ctx.statement.start)],
            )
    }

    pub(crate) fn signature_at(
        &self,
        doc: &str,
        caret: usize,
        dialect: Option<Dialect>,
    ) -> Option<String> {
        if !self.cfg.enabled || !self.cfg.signature_help {
            return None;
        }
        let ctx = intel::context_at(doc, caret, dialect);
        let owner = ctx.paren_owner.as_deref()?;
        builtins::signature(dialect, owner).map(str::to_string)
    }

    /// 마지막 요청이 남긴 즉시 채움 요청(호스트가 탐색기 메타 큐에 넣는다).
    pub(crate) fn take_needs(&mut self) -> Vec<NeedColumns> {
        std::mem::take(&mut self.needs)
    }

    /// 마지막 요청이 남긴 테이블 상세 채움 요청(`JOIN … ON` 조건 조각) — 호스트가 탐색기에 넘긴다(T-178).
    pub(crate) fn take_need_details(&mut self) -> Vec<nsql_run::meta::ObjId> {
        std::mem::take(&mut self.need_details)
    }

    /// 루틴 인자 채움 요청(T-317) — 호스트가 탐색기 메타 큐(백그라운드)에 넣는다.
    pub(crate) fn take_need_args(&mut self) -> Vec<nsql_run::meta::ObjId> {
        std::mem::take(&mut self.need_args)
    }

    /// 마지막 요청이 남긴 객체 목록 채움 요청(스키마 이름 · 사전) — 호스트가 탐색기에 넘긴다(09-23).
    pub(crate) fn take_need_objects(&mut self) -> Vec<String> {
        std::mem::take(&mut self.need_objects)
    }

    pub(crate) fn take_need_kinds(&mut self) -> Vec<(String, ObjectKind)> {
        std::mem::take(&mut self.need_kinds)
    }

    /// 강조된 행의 전체 글(이름 + 오른쪽 열) — 긴 이름이 잘려 보일 때 상태줄에 그대로(09-23).
    pub(crate) fn hovered_full(&self) -> Option<String> {
        let i = self.menu.hovered()?;
        let c = self.cands.get(i)?;
        let mut s = c.text.clone();
        if !c.detail.is_empty() {
            s.push_str("  ");
            s.push_str(&c.detail);
        }
        if !c.mark.is_empty() {
            s.push_str("  ");
            s.push_str(&c.mark);
        }
        Some(s)
    }

    /// 상세 카드 대상 = 강조 행(없으면 첫 행 · 09-24).
    pub(crate) fn card_target(&self) -> Option<crate::intel_card::Target> {
        if !self.is_open() {
            return None;
        }
        // 머문 대상(`card_shown`) · 아직 없으면 지금 강조 행.
        let i = self
            .card_shown
            .unwrap_or_else(|| self.menu.hovered().unwrap_or(0));
        let c = self.cands.get(i)?;
        Some(crate::intel_card::Target::of(c, self.dialect))
    }

    /// 열린 팝업이 무언가를 기다리는 중인가(메타가 오면 호스트가 다시 요청한다).
    pub(crate) fn is_loading(&self) -> bool {
        self.is_open() && (self.loading || self.loading_objects)
    }

    /// 어느 열린 문서든 이 이름을 낱말로 쓰는가(대소문자 무시 · 문서 낱말 캐시 · `스키마.` 버킷의 미사용 판정 근거 · 09-23).
    pub(crate) fn doc_mentions(&self, name: &str) -> bool {
        self.docs
            .values()
            .any(|d| d.words.iter().any(|w| w.eq_ignore_ascii_case(name)))
    }

    /// (스키마, 종류) 버킷 상태를 이번 요청에 반영: 없으면 채움 요청 + "불러오는 중" · 읽는 중이면 "불러오는 중"만.
    /// ★ `CtxKind::Want` 후보(10-01 ⑩): 데이터베이스(`""` 버킷 · Database) · 스키마 · 루틴(현재 스키마 프로시저·함수·패키지 + 스키마 +
    /// SQL Server `sp_*`) · 한 종류(현재 스키마 + 스키마). 버킷이 없으면 호스트에 채움을 청하고 "불러오는 중".
    fn want_cands(
        &mut self,
        w: intel::Want,
        meta: Option<&MetaView<'_>>,
        dialect: Option<Dialect>,
        show_types: bool,
        cands: &mut Vec<Cand>,
    ) {
        // ★ 자료형 자리(사용자 10-09): 내장 자료형은 접속·메타 없이도(정적 표 · `intel.types`) · 사용자 정의 타입은 아래 메타 길
        //   (`ObjectKind::Type` · 현재 스키마 + `스키마.`).
        // ★ `COLLATE |` = 정렬 이름(정적 표 · 메타 없이 · 사용자 10-09). 키워드 종류라 확정 뒤 공백(`intel.insert_space`).
        if w == intel::Want::Collation {
            for (i, b) in nsql_script::builtins::collations(dialect)
                .iter()
                .enumerate()
            {
                cands.push(Cand {
                    text: b.name.to_string(),
                    kind: CandKind::Keyword,
                    detail: if show_types {
                        b.sig.to_string()
                    } else {
                        String::new()
                    },
                    source: 4,
                    tag: 0,
                    mark: String::new(),
                    order: i as u32,
                    qualifier: String::new(),
                    layer: 0,
                });
            }
            return;
        }
        if w == intel::Want::DataType && self.cfg.types {
            for (i, b) in nsql_script::builtins::types(dialect).iter().enumerate() {
                cands.push(Cand {
                    text: b.name.to_string(),
                    kind: CandKind::Type,
                    detail: if show_types {
                        b.sig.to_string()
                    } else {
                        String::new()
                    },
                    source: 4,
                    tag: 0,
                    mark: String::new(),
                    order: i as u32,
                    qualifier: String::new(),
                    layer: 0,
                });
            }
        }
        let Some(m) = meta else { return };
        let schema_cand = |m: &MetaView<'_>, s: Sym| Cand {
            text: m.names.get(s).to_string(),
            kind: CandKind::Schema,
            detail: if show_types {
                "schema".into()
            } else {
                String::new()
            },
            source: 4,
            tag: 0,
            mark: String::new(),
            order: 1,
            qualifier: String::new(),
            layer: 0,
        };
        let mut kinds: Vec<ObjectKind> = Vec::new();
        let mut with_schemas = false;
        let mut databases = false;
        match w {
            intel::Want::Database => {
                if dialect == Some(Dialect::Mysql) {
                    with_schemas = true;
                } else {
                    databases = true;
                }
            }
            intel::Want::Schema => with_schemas = true,
            intel::Want::Routine => {
                kinds.extend([
                    ObjectKind::Procedure,
                    ObjectKind::Function,
                    ObjectKind::Package,
                ]);
                with_schemas = true;
                if dialect == Some(Dialect::Mssql) {
                    for (i, p) in nsql_script::builtins::MSSQL_SYSTEM_PROCS.iter().enumerate() {
                        cands.push(Cand {
                            text: (*p).to_string(),
                            kind: CandKind::Routine,
                            detail: if show_types {
                                "system procedure".into()
                            } else {
                                String::new()
                            },
                            source: 5,
                            tag: 0,
                            mark: String::new(),
                            order: 10 + i as u32,
                            qualifier: String::new(),
                            layer: 3,
                        });
                    }
                }
            }
            intel::Want::Kind(code) => match ObjectKind::parse(code) {
                Some(ObjectKind::Database) => databases = true,
                Some(k) => {
                    kinds.push(k);
                    with_schemas = true;
                }
                None => {}
            },
            // 파라미터 자리는 호출자가 메타 없이 처리한다(T-316).
            intel::Want::ProcParam(_) => return,
            // 사용자 정의 타입(Oracle TYPE · PG 복합/열거/도메인 · SQL Server 별칭/테이블 타입) + `스키마.`.
            intel::Want::DataType => {
                kinds.push(ObjectKind::Type);
                with_schemas = true;
            }
            // 정렬 이름은 위에서 끝났다(메타 없음).
            intel::Want::Collation => return,
        }
        if databases {
            match m.names.find("") {
                Some(empty) => {
                    self.note_coverage(m, empty, ObjectKind::Database);
                    for h in m
                        .snap
                        .prefix(m.names, empty, ObjectKind::Database, "", usize::MAX)
                    {
                        cands.push(obj_cand(m, &h, ObjectKind::Database, dialect, show_types));
                    }
                }
                None => {
                    self.loading_objects = true;
                    push_need(&mut self.need_objects, "");
                }
            }
        }
        if let Some(cur) = m.snap.current_schema {
            for kind in kinds {
                if dialect.is_some_and(|d| !nsql_catalog::kinds_for(d).contains(&kind)) {
                    continue;
                }
                self.note_coverage_kind(m, cur, kind);
                for h in m.snap.prefix(m.names, cur, kind, "", usize::MAX) {
                    cands.push(obj_cand(m, &h, kind, dialect, show_types));
                }
            }
        }
        if with_schemas {
            for s in &m.snap.schemas {
                cands.push(schema_cand(m, *s));
            }
        }
    }

    fn note_coverage(&mut self, m: &MetaView<'_>, schema: Sym, kind: ObjectKind) {
        match m.snap.coverage(schema, kind) {
            Coverage::Loading => self.loading_objects = true,
            Coverage::Missing => {
                self.loading_objects = true;
                push_need(&mut self.need_objects, m.names.get(schema));
            }
            // 낡음(명시 갱신 뒤 · 79 §3): 목록은 그대로 후보로 내고 다시 읽기만 청한다(깜빡임 0).
            Coverage::Stale { .. } => push_need(&mut self.need_objects, m.names.get(schema)),
            // ★ L1 이름 층(85 §2): 이름만으로 즉시 후보 · 상태(유효성·부가)는 L2 승격이 뒤에서 채운다 — 완성은 청하지 않는다.
            Coverage::Names { .. } => {}
            _ => {}
        }
    }

    /// [`Self::note_coverage`]의 **종류 지정** 판 — 관계 넷이 아닌 종류(사용자 정의 타입 · 식 자리 함수·패키지)는 `request_objects`가
    /// 읽지 않으므로 `(스키마, 종류)`로 청한다(10-09 · 종전엔 Type 버킷이 영영 Missing → 팝업 재조립 반복 → 선택 불가).
    fn note_coverage_kind(&mut self, m: &MetaView<'_>, schema: Sym, kind: ObjectKind) {
        let name = m.names.get(schema);
        let push = |v: &mut Vec<(String, ObjectKind)>| {
            if !v
                .iter()
                .any(|(s, k)| s.eq_ignore_ascii_case(name) && *k == kind)
            {
                v.push((name.to_string(), kind));
            }
        };
        match m.snap.coverage(schema, kind) {
            Coverage::Loading => self.loading_objects = true,
            Coverage::Missing => {
                self.loading_objects = true;
                push(&mut self.need_kinds);
            }
            Coverage::Stale { .. } => push(&mut self.need_kinds),
            _ => {}
        }
    }

    /// 팝업이 고른 항목 → 확정(편집기 적용은 호스트).
    #[cfg(test)]
    pub(crate) fn pick(&mut self, id: &str) {
        self.pick_with(id, false);
    }

    /// `plain` = Alt를 누른 채 확정(한정자 규칙을 뒤집는다 · 09-24).
    pub(crate) fn pick_with(&mut self, id: &str, plain: bool) {
        let Some(i) = id
            .strip_prefix("intel:")
            .and_then(|n| n.parse::<usize>().ok())
        else {
            return;
        };
        let (Some(c), Some(ctx)) = (self.cands.get(i), self.ctx.as_ref()) else {
            return;
        };
        let mut text = intel::apply_case(&c.text, &ctx.prefix, &self.cfg.insert_case);
        let mut caret_back = 0;
        // 접두 없이 고른 컬럼 = `A.컬럼`(기본 `intel.qualify_columns` · Alt = 반대).
        if c.kind == CandKind::Column
            && !c.qualifier.is_empty()
            && (self.cfg.qualify_columns != plain)
        {
            text = format!("{}.{}", c.qualifier, text);
        }
        match c.kind {
            // 함수 = `NAME()` + 캐럿 안(인자가 없는 값 `SYSDATE`·`@@ROWCOUNT`는 그대로 · 시그니처의 괄호 유무로 판단).
            CandKind::Function if self.cfg.insert_parens => {
                let sig = builtins::signature(self.dialect, &c.text);
                let has_parens = sig.is_none_or(|s| s.contains('('));
                if has_parens && !text.ends_with('(') {
                    text.push_str("()");
                    if !sig.is_some_and(|s| s.contains("()")) {
                        caret_back = 1;
                    }
                }
            }
            // FROM 자리의 함수·프로시저(테이블 함수 · 09-24) = `NAME()` + 캐럿 안.
            CandKind::Routine if self.cfg.insert_parens && !text.ends_with('(') => {
                text.push_str("()");
                caret_back = 1;
            }
            CandKind::Keyword if self.cfg.insert_space => {
                if !text.ends_with('(') {
                    text.push(' ');
                }
            }
            // ★ 문법이 별칭을 허용하는 자리만(`Context::alias_ok` · 사용자 10-09 "ALTER TABLE t A"·"CREATE INDEX … ON t A" 안 붙게).
            CandKind::Table | CandKind::View | CandKind::Synonym
                if self.cfg.insert_alias && ctx.kind == CtxKind::Relation && ctx.alias_ok =>
            {
                // 치던 접두 자체가 alias 표에 테이블로 잡혀 있으면(`FROM sc|`) 그것은 제외.
                let taken: Vec<&str> = ctx
                    .aliases
                    .iter()
                    .filter(|a| {
                        !(a.alias.eq_ignore_ascii_case(&ctx.prefix)
                            && a.table.eq_ignore_ascii_case(&ctx.prefix))
                    })
                    .map(|a| a.alias.as_str())
                    .collect();
                let alias = if self.cfg.alias_letters {
                    gen_alias_letter(&taken)
                } else {
                    gen_alias(&c.text, &taken)
                };
                text.push(' ');
                text.push_str(&alias);
            }
            _ => {}
        }
        if c.kind != CandKind::Snippet {
            self.mru.retain(|m| !m.eq_ignore_ascii_case(&c.text));
            self.mru.insert(0, c.text.clone());
            self.mru.truncate(MRU_MAX);
        }
        self.accept = Some(Accept {
            replace: ctx.replace.clone(),
            text,
            caret_back,
        });
        self.close();
    }

    pub(crate) fn take_accept(&mut self) -> Option<Accept> {
        self.accept.take()
    }

    /// 팝업이 열린 탭.
    pub(crate) fn tab(&self) -> Option<u64> {
        self.tab
    }
}

/// 컬럼 오른쪽 열: `PK NUMBER(4)` · `FK VARCHAR2(10) NOT NULL` · `UQ …`.
/// 사전 버킷의 스키마 심볼(아직 이름조차 없으면 None = 한 번도 요청 안 함).
fn dict_sym<'a>(m: &'a MetaView<'a>) -> Option<(&'a MetaView<'a>, Sym)> {
    m.names.find(nsql_catalog::DICT_SCHEMA).map(|s| (m, s))
}

/// 채움 요청 목록에 한 번만(대소문자 무시).
fn push_need(v: &mut Vec<String>, name: &str) {
    if !v.iter().any(|x| x.eq_ignore_ascii_case(name)) {
        v.push(name.to_string());
    }
}

/// 객체 종류의 둘러보기 순서(테이블 → 뷰 → 구체화 뷰 → 시노님 → 그 밖 · 09-24).
fn kind_order(k: ObjectKind) -> u32 {
    match k {
        // ★ 같은 층(테이블·뷰·구체화 뷰·시노님)은 종류로 가르지 않는다 — 종류 우선이면 뷰가 테이블 449개 뒤로 밀려 상한(`intel.max_items`)
        //   밖에서 사라졌다(BISCM 실측 · 사용자 09-24 "V로 시작하는 뷰가 목록 끝에 보이지 않아").
        ObjectKind::Table
        | ObjectKind::View
        | ObjectKind::MaterializedView
        | ObjectKind::Synonym => 0,
        ObjectKind::Function => 1,
        ObjectKind::Package => 2,
        ObjectKind::Procedure => 3,
        _ => 4,
    }
}

/// ★ 레이어(사용자 09-24): 테이블·뷰·구체화 뷰·시노님 = 0(같은 층에서 일치 등급으로 경쟁) · 테이블 함수 = 1 · 함수·패키지 = 2 ·
/// 프로시저 = 3. 랭킹은 같은 일치 등급 안에서만 레이어를 본다(nsql-script `rank`).
fn layer_of(k: ObjectKind, table_fn: bool) -> u8 {
    match k {
        ObjectKind::Table
        | ObjectKind::View
        | ObjectKind::MaterializedView
        | ObjectKind::Synonym => 0,
        ObjectKind::Function if table_fn => 1,
        ObjectKind::Function | ObjectKind::Package => 2,
        ObjectKind::Procedure => 3,
        _ => 2,
    }
}

/// FROM 자리(`FROM |` · `스키마.|`)에 올릴 객체 종류 — 관계 넷 + `routines`면 함수·패키지·프로시저 · 방언이 지원하는 것만.
fn from_kinds(dialect: Option<Dialect>, routines: bool) -> Vec<ObjectKind> {
    let mut v = vec![
        ObjectKind::Table,
        ObjectKind::View,
        ObjectKind::MaterializedView,
        ObjectKind::Synonym,
    ];
    if routines {
        v.extend([
            ObjectKind::Function,
            ObjectKind::Package,
            ObjectKind::Procedure,
        ]);
    }
    v.retain(|k| dialect.is_none_or(|d| nsql_catalog::kinds_for(d).contains(k)));
    v
}

/// 창 방식 문맥 구간(09-24 §187): 캐럿 앞뒤 `WINDOW_BYTES` 안에서 빈 줄(`\n\n`) 경계를 찾아 자르고(없으면 줄 시작/끝) 글자 경계로 맞춘다.
/// 문장은 보통 빈 줄로 나뉘므로 캐럿 문장은 통째로 들어온다. 반환 = (시작, 끝) 절대 바이트.
const WINDOW_BYTES: usize = 256 * 1024;
/// 캐럿 앞 글(접두 제외)이 **인자 시작** 자리인가 — 공백을 걷은 마지막 글자가 `(` 또는 `,`(순수 · T-317 이름 지정 인자).
fn named_arg_start(head: &str) -> bool {
    matches!(head.trim_end().chars().last(), Some('(' | ','))
}

/// 이미 이름을 지정한 인자들(`이름 =>` 의 이름 · 순수).
fn named_args_used(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices("=>") {
        let before = text[..i].trim_end();
        let name: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '#')
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out
}

/// 글 안의 `@이름` 낱말들(시스템 프로시저 파라미터 자리에서 이미 쓴 것 · T-316 · 순수).
fn param_names_in(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' {
            let s = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'@') {
                i += 1;
            }
            if i > s + 1 {
                out.push(text[s..i].to_string());
            }
        } else {
            i += 1;
        }
    }
    out
}

fn window_of(doc: &str, caret: usize) -> (usize, usize) {
    let caret = caret.min(doc.len());
    let mut a = caret.saturating_sub(WINDOW_BYTES);
    if a > 0 {
        a = match doc[a..caret].find("\n\n") {
            Some(i) => a + i + 2,
            None => doc[a..caret].find('\n').map_or(a, |i| a + i + 1),
        };
    }
    let mut b = (caret + WINDOW_BYTES).min(doc.len());
    if b < doc.len() {
        b = match doc[caret..b].rfind("\n\n") {
            Some(i) => caret + i,
            None => doc[caret..b].rfind('\n').map_or(b, |i| caret + i),
        };
        if b <= caret {
            b = (caret + WINDOW_BYTES).min(doc.len());
        }
    }
    while a > 0 && !doc.is_char_boundary(a) {
        a -= 1;
    }
    while b < doc.len() && !doc.is_char_boundary(b) {
        b += 1;
    }
    (a, b)
}

/// ★ FROM 자리에 올 수 있는가(사용자 09-24 "SQL Server 프로시저가 결과를 FROM으로 주나?" — 아니다): 프로시저는 어느 DBMS든 제외 ·
/// SQL Server 스칼라·집계 함수(`FN`/`AF`/`FS`)도 제외(테이블 반환 `IF`/`TF`/`FT`만) · Oracle 함수는 컬렉션 반환이 사전에 안 보이므로 남긴다.
fn from_ok(
    m: &MetaView<'_>,
    h: &nsql_run::meta::Hit,
    kind: ObjectKind,
    dialect: Option<Dialect>,
) -> bool {
    match kind {
        ObjectKind::Procedure => false,
        ObjectKind::Function if dialect == Some(Dialect::Mssql) => {
            let extra = m
                .snap
                .object(h.id)
                .and_then(|o| o.extra)
                .map_or("", |e| m.names.get(e));
            nsql_catalog::table_function(Dialect::Mssql, kind, extra)
        }
        _ => true,
    }
}

/// 메타 객체 하나 → 후보(레이어 · 테이블 함수 표기 · 카드용 태그).
fn obj_cand(
    m: &MetaView<'_>,
    h: &nsql_run::meta::Hit,
    kind: ObjectKind,
    dialect: Option<Dialect>,
    show_types: bool,
) -> Cand {
    let extra = m
        .snap
        .object(h.id)
        .and_then(|o| o.extra)
        .map_or("", |e| m.names.get(e));
    let table_fn = dialect.is_some_and(|d| nsql_catalog::table_function(d, kind, extra));
    Cand {
        text: m.names.get(h.name).to_string(),
        kind: cand_kind(kind),
        detail: if !show_types {
            String::new()
        } else if table_fn {
            "table function".to_string()
        } else {
            kind_label(kind)
        },
        source: 3,
        tag: tag_object(h.id),
        mark: String::new(),
        order: kind_order(kind),
        qualifier: String::new(),
        layer: layer_of(kind, table_fn),
    }
}

/// 후보의 아이콘 종류 — 메타 객체(태그 2)는 실제 종류로 · 그 밖은 후보 종류로 · 키워드·낱말·조각은 없음.
fn icon_kind_of(c: &Cand, m: Option<&MetaView<'_>>) -> Option<IconKind> {
    if c.tag >> 56 == 2 {
        let id = nsql_run::meta::ObjId(((c.tag >> 16) & 0xFFFF_FFFF) as u32);
        if let Some(o) = m.and_then(|m| m.snap.object(id)) {
            return Some(match o.kind {
                ObjectKind::Table => IconKind::Table,
                ObjectKind::View => IconKind::View,
                ObjectKind::MaterializedView => IconKind::MatView,
                ObjectKind::Procedure => IconKind::Procedure,
                ObjectKind::Function => IconKind::Function,
                ObjectKind::Package => IconKind::Package,
                ObjectKind::PackageBody => IconKind::PackageBody,
                ObjectKind::Sequence => IconKind::Sequence,
                ObjectKind::Trigger => IconKind::Trigger,
                ObjectKind::Index => IconKind::Index,
                ObjectKind::Synonym => IconKind::Synonym,
                ObjectKind::Type => IconKind::Type,
                ObjectKind::ExternalTable | ObjectKind::ForeignTable => IconKind::Table,
                ObjectKind::Aggregate => IconKind::Function,
                ObjectKind::Queue => IconKind::Queue,
                ObjectKind::DbLink => IconKind::Link,
                ObjectKind::JavaClass => IconKind::Java,
                ObjectKind::Job | ObjectKind::SchedulerJob | ObjectKind::SchedulerProgram => {
                    IconKind::Job
                }
                ObjectKind::SchemaTrigger | ObjectKind::EventTrigger => IconKind::Trigger,
                ObjectKind::Extension => IconKind::Package,
                ObjectKind::Database
                | ObjectKind::DbUser
                | ObjectKind::DbRole
                | ObjectKind::Schema
                | ObjectKind::Login
                | ObjectKind::ServerRole => IconKind::Schema,
                ObjectKind::LinkedServer | ObjectKind::Endpoint => IconKind::Link,
                ObjectKind::Credential
                | ObjectKind::CryptoProvider
                | ObjectKind::ServerAudit
                | ObjectKind::AuditSpec => IconKind::Property,
                ObjectKind::BackupDevice => IconKind::Package,
                ObjectKind::ServerTrigger => IconKind::Trigger,
            });
        }
    }
    Some(match c.kind {
        CandKind::Column => IconKind::Column,
        CandKind::Table => IconKind::Table,
        CandKind::View => IconKind::View,
        CandKind::Synonym => IconKind::Synonym,
        CandKind::Sequence => IconKind::Sequence,
        CandKind::Package => IconKind::Package,
        CandKind::Routine | CandKind::Function => IconKind::Function,
        CandKind::Type => IconKind::Type,
        CandKind::Schema | CandKind::Database => IconKind::Schema,
        _ => return None,
    })
}

/// 종류 아이콘(탐색기 도형 · 종류 색 그대로 · 캐시).
fn menu_icon(cache: &mut HashMap<IconKind, MenuIcon>, k: IconKind) -> MenuIcon {
    cache
        .entry(k)
        .or_insert_with(|| {
            let img = exp_icons::image(k, k.color());
            let alpha: Vec<u8> = img.rgba.chunks(4).map(|p| p[3]).collect();
            MenuIcon {
                w: img.w,
                h: img.h,
                alpha: alpha.into(),
                rgba: Some(img.rgba.into()),
            }
        })
        .clone()
}

/// 후보 태그(상세 카드 · 09-24): 상위 8비트 종류(1 = 컬럼 · 2 = 객체) · 객체 id · 컬럼 순번.
pub(crate) fn tag_column(id: nsql_run::meta::ObjId, pos: u16) -> u64 {
    (1u64 << 56) | ((id.0 as u64) << 16) | pos as u64
}
pub(crate) fn tag_object(id: nsql_run::meta::ObjId) -> u64 {
    (2u64 << 56) | ((id.0 as u64) << 16)
}
/// 오른쪽 키 표식(`PK` · `FK` · `UQ`).
fn key_marks(key: u8, nullable: Option<bool>) -> String {
    let mut m: Vec<&str> = Vec::new();
    if key & nsql_run::meta::KEY_PK != 0 {
        m.push("PK");
    }
    if key & nsql_run::meta::KEY_FK != 0 {
        m.push("FK");
    }
    if key & nsql_run::meta::KEY_UQ != 0 {
        m.push("UQ");
    }
    // Not Null은 표식에서 뺀다(카드에 있다 · 사용자 09-24).
    let _ = nullable;
    m.join(" ")
}

/// 조인 조건 조각의 한쪽 — 문장에 나온 테이블 하나와 그 키(PK · FK)만(순수 판정용 · 메타 저장소와 떼어 시험한다).
#[derive(Clone, Debug, PartialEq, Eq)]
struct JoinSide {
    alias: String,
    table: String,
    keys: Vec<JoinKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JoinKey {
    name: String,
    /// `P` = 기본 키 · `R` = 외래 키.
    kind: char,
    cols: Vec<String>,
    ref_table: Option<String>,
    /// FK가 가리키는 컬럼들(비면 모름 → 부모 PK 순서).
    ref_cols: Vec<String>,
}

/// 캐럿 앞이 `JOIN [스키마.]테이블 [[AS] 별칭] ON`으로 끝나면 그 **조인되는 쪽의 이름**(별칭이 있으면 별칭 · 없으면 테이블).
/// `ON` 뒤에 무엇이든 이미 쳤으면(`ON a.x = `) 해당 없음 — 조건 조각은 빈 자리에서만 낸다.
fn join_on_target(before: &str) -> Option<String> {
    let mut words = before.split_whitespace().rev();
    if !words.next()?.eq_ignore_ascii_case("ON") {
        return None;
    }
    let name = words.next()?;
    // `ON` 앞 = 별칭 또는 테이블 · 그 앞으로 `AS` · 테이블 · `JOIN`이 이어져야 한다(`GRANT … ON` · `CREATE INDEX … ON` 제외).
    let mut prev = words.next()?;
    if prev.eq_ignore_ascii_case("AS") {
        prev = words.next()?;
    }
    let after_join = |w: &str| w.eq_ignore_ascii_case("JOIN") || w.eq_ignore_ascii_case("APPLY");
    if !after_join(prev) && !after_join(words.next()?) {
        return None;
    }
    let name = name.rsplit('.').next().unwrap_or(name);
    let name = name.trim_matches(|c| matches!(c, '"' | '`' | '[' | ']'));
    (!name.is_empty()).then(|| name.to_string())
}

/// 외래 키로 만든 조인 조건들 — `sides[joined]`(방금 `JOIN`한 테이블)와 나머지 사이에서, 한쪽의 FK가 다른 쪽을 가리킬 때
/// (`자식.fk = 부모.참조열` · 복합 키는 `AND`로). 부모 쪽 열 = FK의 **참조 컬럼**(`ref_cols` · T-178 10-06 · 유니크 키 참조도
/// 맞다) · 메타에 없으면 **같은 컬럼 수의 기본 키** 순서(없거나 수가 다르면 내지 않는다). 돌려주는 값 = (조건 글, FK 이름) ·
/// 문장에 나온 순서대로.
fn fk_join_conds(sides: &[JoinSide], joined: usize) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let Some(j) = sides.get(joined) else {
        return out;
    };
    let points_at = |k: &JoinKey, parent: &JoinSide| {
        k.kind == 'R'
            && k.ref_table.as_deref().is_some_and(|t| {
                t.rsplit('.')
                    .next()
                    .unwrap_or(t)
                    .eq_ignore_ascii_case(&parent.table)
            })
    };
    let mut emit = |child: &JoinSide, parent: &JoinSide| {
        let pk = parent.keys.iter().find(|k| k.kind == 'P');
        for fk in child.keys.iter().filter(|k| points_at(k, parent)) {
            if fk.cols.is_empty() {
                continue;
            }
            let parent_cols: &[String] = if fk.ref_cols.len() == fk.cols.len() {
                &fk.ref_cols
            } else {
                match pk {
                    Some(pk) if pk.cols.len() == fk.cols.len() => &pk.cols,
                    _ => continue,
                }
            };
            let cond = fk
                .cols
                .iter()
                .zip(parent_cols)
                .map(|(f, p)| format!("{}.{f} = {}.{p}", child.alias, parent.alias))
                .collect::<Vec<_>>()
                .join(" AND ");
            if !out.iter().any(|(c, _)| c == &cond) {
                out.push((cond, fk.name.clone()));
            }
        }
    };
    for (i, o) in sides.iter().enumerate() {
        if i == joined {
            continue;
        }
        // 방금 조인한 쪽이 자식(FK 보유)인 조건을 먼저 · 다음에 반대 방향.
        emit(j, o);
        emit(o, j);
    }
    out
}

/// 테이블 이름 → 짧은 alias(`sales_customer` → `sc` · `emp` → `e` · `MyTable` → `mt`) · 문장 안 alias와 겹치면 숫자 붙임 ·
/// 키워드 모양(`in`·`as`·`or` …)이면 `1`을 붙인다(`intel.insert_alias` · T-178).
/// ★ 건너편 컬럼 이름(사용자 09-29 "`B.PROJECT_CD = A.` 이면 A에도 PROJECT_CD가 있을 때 1번 추천"): 접두 시작 `at` 앞에서
/// 한정자(`A.`) → 공백 → 비교/대입 연산자(`=` `<>` `!=` `<` `>` `<=` `>=`) → 공백 → (한정) 식별자를 거꾸로 읽어 마지막 이름을
/// 돌려준다. JOIN ON · WHERE · UPDATE/MERGE SET · MERGE ON 모두 같은 모양. 연산자가 없으면 `None`.
fn peer_column(doc: &str, at: usize) -> Option<String> {
    let b = doc.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'#' | b'"');
    let mut i = at.min(b.len());
    while i > 0 && (ident(b[i - 1]) || b[i - 1] == b'.') {
        i -= 1;
    }
    while i > 0 && b[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let op_end = i;
    while i > 0 && matches!(b[i - 1], b'=' | b'<' | b'>' | b'!') {
        i -= 1;
    }
    if !matches!(&doc[i..op_end], "=" | "<>" | "!=" | "<" | ">" | "<=" | ">=") {
        return None;
    }
    while i > 0 && b[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let end = i;
    while i > 0 && (ident(b[i - 1]) || b[i - 1] == b'.') {
        i -= 1;
    }
    let name = doc[i..end].rsplit('.').next()?.trim_matches('"');
    let first = name.bytes().next()?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    Some(name.to_string())
}

/// A, B, C … 순서 alias(`intel.alias_style = letters` · 사용자 09-29): 문장 안에서 안 쓰인 첫 대문자 · Z까지 다 쓰였으면 `A2`, `B2` ….
fn gen_alias_letter(taken: &[&str]) -> String {
    for round in 0u32.. {
        for ch in 'A'..='Z' {
            let cand = if round == 0 {
                ch.to_string()
            } else {
                format!("{ch}{}", round + 1)
            };
            if !taken.iter().any(|t| t.eq_ignore_ascii_case(&cand)) {
                return cand;
            }
        }
    }
    "A".to_string()
}

fn gen_alias(name: &str, taken: &[&str]) -> String {
    let base = name.rsplit('.').next().unwrap_or(name);
    let mut a = String::new();
    let mut prev = '_';
    for ch in base.chars() {
        if ch.is_alphanumeric()
            && (prev == '_'
                || prev == '$'
                || prev == '#'
                || (ch.is_uppercase() && prev.is_lowercase()))
        {
            a.push(ch.to_ascii_lowercase());
        }
        prev = ch;
    }
    if a.is_empty() {
        a = base.chars().take(1).collect::<String>().to_lowercase();
    }
    if a.is_empty() {
        a.push('t');
    }
    const KW: &[&str] = &[
        "in", "as", "or", "on", "by", "is", "if", "to", "at", "do", "go", "no",
    ];
    if KW.contains(&a.as_str()) {
        a.push('1');
    }
    let mut out = a.clone();
    let mut n = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&out)) {
        out = format!("{a}{n}");
        n += 1;
    }
    out
}

/// 객체 종류 라벨(팝업 오른쪽 열 · 소문자).
fn kind_label(k: ObjectKind) -> String {
    format!("{k:?}").to_lowercase()
}

fn cand_kind(k: ObjectKind) -> CandKind {
    match k {
        ObjectKind::Table => CandKind::Table,
        ObjectKind::View => CandKind::View,
        ObjectKind::Synonym => CandKind::Synonym,
        ObjectKind::Sequence => CandKind::Sequence,
        ObjectKind::Package => CandKind::Package,
        ObjectKind::Procedure | ObjectKind::Function => CandKind::Routine,
        ObjectKind::Database => CandKind::Database,
        ObjectKind::Schema => CandKind::Schema,
        ObjectKind::Type => CandKind::Type,
        _ => CandKind::Symbol,
    }
}

/// 메모리 맵 보고(docs/80): 문서 낱말 캐시 · 후보(열린 동안만) · 종류 아이콘 캐시.
impl crate::memstat::MemSource for Intel {
    fn mem_report(&self, acc: &mut crate::memstat::Acc) {
        let words: usize = self
            .docs
            .values()
            .map(|d| d.words.iter().map(|w| w.len() + 24).sum::<usize>())
            .sum();
        let cands: usize = self
            .cands
            .iter()
            .map(|c| c.text.len() + c.detail.len() + c.qualifier.len() + 64)
            .sum();
        let icons = self.icons.len() * 32 * 32 * 5;
        acc.add(crate::memstat::Cat::Intel, (words + cands + icons) as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_run::meta::{MetaStore, NewCol, NewObj};

    /// 이름 지정 인자 도우미(T-317 확장 · 10-08): 인자 시작 = `(`·`,` 뒤 · 이미 적은 `이름 =>` 수집.
    #[test]
    fn named_arg_helpers() {
        assert!(named_arg_start("EXEC BISCM.SP_X("));
        assert!(named_arg_start("EXEC BISCM.SP_X(p_a => 1, "));
        assert!(!named_arg_start("EXEC BISCM.SP_X(p_a => "));
        assert!(!named_arg_start("SELECT fn(a"));
        assert_eq!(
            named_args_used("EXEC SP_X(p_pjt_code => 'A', p_result=>:r, "),
            vec!["p_pjt_code".to_string(), "p_result".to_string()]
        );
        assert!(named_args_used("EXEC SP_X(1, 2)").is_empty());
        assert_eq!(
            param_names_in("@a = 1, @level0type = N'x'"),
            vec!["@a", "@level0type"]
        );
    }

    fn cfg() -> IntelCfg {
        IntelCfg {
            enabled: true,
            auto: true,
            delay_ms: 250,
            triggers: vec!['.'],
            min_chars: 2,
            mode: MatchMode::Fuzzy,
            recent: true,
            max_items: 200,
            rows: 12,
            popup_max_w: 560,
            key_passthrough: true,
            detail_card: true,
            grammar: true,
            detail_bg_alpha: 20,
            detail_text_alpha: 50,
            qualify_columns: true,
            routines: false,
            icons: false,
            card_settle_ms: 0,
            star_lines: false,
            star_tab: Some(false),
            max_total: 5000,
            max_doc_kb: 65_536,
            keywords: true,
            doc_words: true,
            insert_case: "default".into(),
            show_types: true,
            functions: true,
            insert_parens: true,
            types: true,
            insert_alias: true,
            alias_letters: false,
            insert_space: true,
            insert_columns: true,
            join_fk: true,
            signature_help: true,
            signature_card: true,
            budget_ms: 30,
        }
    }

    fn store() -> MetaStore {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["SCOTT".into(), "HR".into()], Some("SCOTT"));
        m.load_bucket(
            "SCOTT",
            ObjectKind::Table,
            &[
                NewObj {
                    name: "EMP".into(),
                    ..Default::default()
                },
                NewObj {
                    name: "DEPT".into(),
                    ..Default::default()
                },
                NewObj {
                    name: "SALES_CUSTOMER".into(),
                    ..Default::default()
                },
            ],
            1,
        );
        let id = m
            .snapshot()
            .lookup(&m.names, Some("SCOTT"), "EMP")
            .expect("emp");
        m.set_columns(
            id,
            &[
                NewCol {
                    name: "EMPNO".into(),
                    data_type: "NUMBER(4)".into(),
                    key: nsql_run::meta::KEY_PK,
                    ..Default::default()
                },
                NewCol {
                    name: "ENAME".into(),
                    data_type: "VARCHAR2(10)".into(),
                    ..Default::default()
                },
            ],
            1,
        );
        m
    }

    /// T-178 JOIN 조건 조각(통합): 제약을 모르면 상세 요청 + 불러오는 중 → 채워지면 첫 후보 = FK 조건 조각 · 설정 끔 = 없음.
    /// ★ 자료형 자리(사용자 10-09): 메타 없이도 방언 내장 자료형(정적 표)이 후보 · `CAST(x AS NU` 접두로 NUMBER가 맨 위 ·
    /// `intel.types` 끄면 내장 자료형 0 · 메타가 있으면 사용자 정의 타입(ObjectKind::Type)도 함께.
    #[test]
    fn data_type_position_offers_builtin_and_user_types() {
        let host = Rect::new(0, 0, 800, 600);
        let ask = |it: &mut Intel, doc: &str, view: Option<&MetaView<'_>>| {
            it.request(
                1,
                1,
                doc,
                doc.len(),
                Some(Dialect::Oracle),
                view,
                Some(Point { x: 0, y: 0 }),
                host,
                1.0,
                &|_| None,
            )
        };
        let mut it = Intel::new(cfg());
        ask(&mut it, "SELECT CAST(a AS NU", None);
        assert!(
            it.cands
                .iter()
                .any(|c| c.kind == CandKind::Type && c.text == "NUMBER"),
            "{}",
            it.dump_cands()
        );
        assert!(
            it.cands.iter().all(|c| c.kind == CandKind::Type),
            "자료형만"
        );
        ask(&mut it, "DECLARE\n  v_x ", None);
        assert!(it.cands.iter().any(|c| c.text == "VARCHAR2"));
        assert!(
            it.cands.iter().any(|c| c.text == "SYS_REFCURSOR"),
            "PL/SQL 전용도"
        );
        // 끄면 0.
        let mut off_cfg = cfg();
        off_cfg.types = false;
        let mut off = Intel::new(off_cfg);
        ask(&mut off, "SELECT CAST(a AS ", None);
        assert!(!off.cands.iter().any(|c| c.kind == CandKind::Type));
        // 메타의 사용자 정의 타입.
        let mut m = store();
        m.load_bucket(
            "SCOTT",
            ObjectKind::Type,
            &[
                NewObj {
                    name: "T_ADDR".into(),
                    ..Default::default()
                },
                NewObj {
                    name: "T_NUM_TAB".into(),
                    ..Default::default()
                },
            ],
            1,
        );
        let view = MetaView {
            names: &m.names,
            snap: m.snapshot(),
        };
        ask(&mut it, "CREATE TABLE t (addr T_", Some(&view));
        assert!(
            it.cands
                .iter()
                .any(|c| c.kind == CandKind::Type && c.text == "T_ADDR"),
            "{}",
            it.dump_cands()
        );
        assert!(it.take_need_kinds().is_empty(), "버킷이 있으면 요청 없음");
        // ★ Type 버킷이 없으면 **종류 지정** 요청(10-09 "ALTER TABLE 자료형 팝업 선택 불가"의 뿌리 = `request_objects`가 Type을 안 읽어
        //   영영 "불러오는 중" → 팝업 재조립 반복). 내장 자료형은 그대로 후보.
        let m2 = store();
        let view2 = MetaView {
            names: &m2.names,
            snap: m2.snapshot(),
        };
        ask(&mut it, "ALTER TABLE t ADD c ", Some(&view2));
        assert!(it.loading_objects);
        let kinds = it.take_need_kinds();
        assert!(
            kinds
                .iter()
                .any(|(s, k)| s == "SCOTT" && *k == ObjectKind::Type),
            "{kinds:?}"
        );
        assert!(it.cands.iter().any(|c| c.text == "VARCHAR2"));
    }

    #[test]
    fn join_on_offers_fk_condition_after_details_load() {
        use nsql_run::meta::NewDetail;
        let mut m = store();
        let host = Rect::new(0, 0, 800, 600);
        let doc = "SELECT * FROM emp e JOIN dept d ON ";
        let ask = |it: &mut Intel, m: &MetaStore| {
            let view = MetaView {
                names: &m.names,
                snap: m.snapshot(),
            };
            it.request(
                1,
                1,
                doc,
                doc.len(),
                Some(Dialect::Oracle),
                Some(&view),
                Some(Point { x: 0, y: 0 }),
                host,
                1.0,
                &|_| None,
            )
        };
        let mut it = Intel::new(cfg());
        ask(&mut it, &m);
        let snap = m.snapshot();
        let emp = snap.lookup(&m.names, Some("SCOTT"), "EMP").expect("emp");
        let dept = snap.lookup(&m.names, Some("SCOTT"), "DEPT").expect("dept");
        let mut want = it.take_need_details();
        want.sort_by_key(|i| i.0);
        let mut both = vec![emp, dept];
        both.sort_by_key(|i| i.0);
        assert_eq!(want, both, "두 테이블의 제약을 요청");
        assert!(it.loading);
        assert!(!it.cands.iter().any(|c| c.kind == CandKind::Snippet));
        m.set_detail(
            emp,
            &NewDetail {
                keys: vec![
                    ("PK_EMP".into(), 'P', vec!["EMPNO".into()], None, vec![]),
                    (
                        "FK_DEPT".into(),
                        'R',
                        vec!["DEPTNO".into()],
                        Some("DEPT".into()),
                        vec!["DEPTNO".into()],
                    ),
                ],
                ..Default::default()
            },
            2,
        );
        m.set_detail(
            dept,
            &NewDetail {
                keys: vec![("PK_DEPT".into(), 'P', vec!["DEPTNO".into()], None, vec![])],
                ..Default::default()
            },
            2,
        );
        assert!(ask(&mut it, &m));
        assert!(it.take_need_details().is_empty());
        assert_eq!(it.cands[0].kind, CandKind::Snippet, "{}", it.dump_cands());
        assert_eq!(it.cands[0].text, "e.DEPTNO = d.DEPTNO");
        assert_eq!(it.cands[0].detail, "FK FK_DEPT");
        // 끔 = 조각 없음 · 요청도 없음.
        let mut off = Intel::new(IntelCfg {
            join_fk: false,
            ..cfg()
        });
        ask(&mut off, &m);
        assert!(!off.cands.iter().any(|c| c.kind == CandKind::Snippet));
    }

    /// T-178 JOIN 조건 조각 — `ON` 자리 판정(MC/DC: 끝 낱말 ON · 앞이 JOIN 사슬 · AS 유무 · 스키마/인용).
    #[test]
    fn join_on_target_rules() {
        let f = |s: &str| join_on_target(s);
        assert_eq!(
            f("SELECT * FROM emp e JOIN dept d ON ").as_deref(),
            Some("d")
        );
        assert_eq!(
            f("SELECT * FROM emp e LEFT JOIN hr.dept AS d\n  on").as_deref(),
            Some("d")
        );
        assert_eq!(
            f("FROM emp JOIN scott.\"DEPT\" ON ").as_deref(),
            Some("DEPT")
        );
        assert_eq!(
            f("FROM emp e JOIN dept d ON e.x = "),
            None,
            "이미 조건을 치는 중"
        );
        assert_eq!(f("GRANT SELECT ON "), None, "JOIN 사슬이 아니다");
        assert_eq!(f("CREATE INDEX ix ON "), None);
        assert_eq!(f("FROM emp e JOIN dept d "), None, "ON이 없다");
    }

    /// 외래 키 → 조건 글: 방향 둘(조인한 쪽이 자식/부모) · 복합 키 · PK 없는 부모·컬럼 수 불일치는 내지 않는다.
    #[test]
    fn fk_join_conds_rules() {
        let key = |name: &str, kind: char, cols: &[&str], rt: Option<&str>| JoinKey {
            name: name.into(),
            kind,
            cols: cols.iter().map(|c| c.to_string()).collect(),
            ref_table: rt.map(str::to_string),
            ref_cols: Vec::new(),
        };
        let emp = JoinSide {
            alias: "e".into(),
            table: "EMP".into(),
            keys: vec![
                key("PK_EMP", 'P', &["EMPNO"], None),
                key("FK_DEPT", 'R', &["DEPTNO"], Some("scott.dept")),
                key("FK_MGR", 'R', &["MGR"], Some("EMP")),
            ],
        };
        let dept = JoinSide {
            alias: "d".into(),
            table: "DEPT".into(),
            keys: vec![key("PK_DEPT", 'P', &["DEPTNO"], None)],
        };
        // 부모를 조인: `FROM emp e JOIN dept d ON |`
        assert_eq!(
            fk_join_conds(&[emp.clone(), dept.clone()], 1),
            vec![("e.DEPTNO = d.DEPTNO".to_string(), "FK_DEPT".to_string())]
        );
        // 자식을 조인: `FROM dept d JOIN emp e ON |`
        assert_eq!(
            fk_join_conds(&[dept.clone(), emp.clone()], 1),
            vec![("e.DEPTNO = d.DEPTNO".to_string(), "FK_DEPT".to_string())]
        );
        // 자기 참조: `FROM emp e JOIN emp m ON |` → 양방향 둘.
        let mgr = JoinSide {
            alias: "m".into(),
            ..emp.clone()
        };
        assert_eq!(
            fk_join_conds(&[emp, mgr], 1)
                .into_iter()
                .map(|(c, _)| c)
                .collect::<Vec<_>>(),
            ["m.MGR = e.EMPNO", "e.MGR = m.EMPNO"]
        );
        // 복합 키 · PK 없는 부모 · 컬럼 수 불일치.
        let line = JoinSide {
            alias: "l".into(),
            table: "ORD_LINE".into(),
            keys: vec![
                key("FK_ORD", 'R', &["PLANT", "ORD_NO"], Some("ORD")),
                key("FK_BAD", 'R', &["ORD_NO"], Some("ORD")),
            ],
        };
        let ord = JoinSide {
            alias: "o".into(),
            table: "ORD".into(),
            keys: vec![key("PK_ORD", 'P', &["PLANT", "ORD_NO"], None)],
        };
        assert_eq!(
            fk_join_conds(&[ord.clone(), line.clone()], 1),
            vec![(
                "l.PLANT = o.PLANT AND l.ORD_NO = o.ORD_NO".to_string(),
                "FK_ORD".to_string()
            )]
        );
        let no_pk = JoinSide {
            keys: Vec::new(),
            ..ord
        };
        assert!(fk_join_conds(&[no_pk.clone(), line], 1).is_empty());
        assert!(fk_join_conds(&[dept], 3).is_empty());
        // ★ 참조 컬럼이 있으면 PK와 무관(유니크 키 참조 · 열 이름이 다름) · 참조 컬럼 수가 FK 열 수와 다르면 PK 폴백.
        let by_uq = JoinSide {
            alias: "l".into(),
            table: "ORD_LINE".into(),
            keys: vec![JoinKey {
                ref_cols: vec!["ORD_CODE".into()],
                ..key("FK_ORD_CODE", 'R', &["ORD_NO"], Some("ORD"))
            }],
        };
        assert_eq!(
            fk_join_conds(&[no_pk, by_uq.clone()], 1),
            vec![(
                "l.ORD_NO = o.ORD_CODE".to_string(),
                "FK_ORD_CODE".to_string()
            )]
        );
        let bad_ref = JoinSide {
            keys: vec![JoinKey {
                ref_cols: vec!["A".into(), "B".into()],
                ..key("FK_ORD_CODE", 'R', &["ORD_NO"], Some("ORD"))
            }],
            ..by_uq
        };
        let ord1 = JoinSide {
            alias: "o".into(),
            table: "ORD".into(),
            keys: vec![key("PK_ORD", 'P', &["ORD_NO"], None)],
        };
        assert_eq!(
            fk_join_conds(&[ord1, bad_ref], 1),
            vec![("l.ORD_NO = o.ORD_NO".to_string(), "FK_ORD_CODE".to_string())]
        );
    }

    /// ★ 건너편 컬럼 1순위(사용자 09-29): `d.ENAME = e.|` → e의 컬럼 중 ENAME이 맨 위(평소는 EMPNO 먼저) · UPDATE SET도 같다.
    #[test]
    fn peer_column_ranks_first_across_operator() {
        let m = store();
        let view = MetaView {
            names: &m.names,
            snap: m.snapshot(),
        };
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        for doc in [
            "SELECT 1 FROM emp e JOIN dept d ON d.ENAME = e.",
            "UPDATE emp e SET e.EMPNO = 1, e.ENAME = e.",
            "MERGE INTO emp e USING dept d ON (d.ENAME = e.",
        ] {
            let caret = doc.len();
            assert!(it.request(
                1,
                caret as u64,
                doc,
                caret,
                Some(Dialect::Oracle),
                Some(&view),
                Some(Point { x: 0, y: 0 }),
                host,
                1.0,
                &|_| None
            ));
            let texts: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
            assert_eq!(texts.first().copied(), Some("ENAME"), "{doc}: {texts:?}");
            assert_eq!(it.menu.hovered(), Some(0), "선택 상태로 열린다: {doc}");
            it.close();
        }
        // 건너편이 없으면 평소 순서(EMPNO 먼저).
        let doc = "SELECT e. FROM emp e";
        assert!(it.request(
            1,
            99,
            doc,
            9,
            Some(Dialect::Oracle),
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        assert_eq!(it.cands[0].text, "EMPNO");
        assert_eq!(
            it.menu.hovered(),
            None,
            "건너편이 없으면 선택 없음(평소대로)"
        );
    }

    #[test]
    fn member_columns_relation_tables_and_loading_request() {
        let m = store();
        let snap = m.snapshot();
        let view = MetaView {
            names: &m.names,
            snap,
        };
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        let doc = "SELECT e. FROM emp e, dept d";
        assert!(it.request(
            1,
            1,
            doc,
            9,
            Some(Dialect::Oracle),
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        let texts: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, vec!["EMPNO", "ENAME"]);
        assert!(it.cands[0].mark.contains("PK") && it.cands[0].detail == "NUMBER(4)");
        // dept = 컬럼 없음 → 즉시 채움 요청 + 불러오는 중.
        let doc = "SELECT d.| FROM emp e, dept d".replace('|', "");
        assert!(it.request(
            1,
            1,
            &doc,
            9,
            Some(Dialect::Oracle),
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        assert!(it.loading);
        assert_eq!(
            it.take_needs(),
            vec![NeedColumns {
                schema: Some("SCOTT".into()),
                table: "dept".into(),
                urgent: true,
            }]
        );
        // FROM 뒤 = 테이블 + 스키마 · 접두 sc = 퍼지(sales_customer) + 스키마 SCOTT.
        let doc = "SELECT * FROM sc";
        assert!(it.request(
            1,
            1,
            doc,
            doc.len(),
            Some(Dialect::Oracle),
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        let texts: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
        assert!(
            texts.contains(&"SCOTT") && texts.contains(&"SALES_CUSTOMER"),
            "{texts:?}"
        );
        // 확정 = 접두 구간 교체 · MRU.
        let i = texts
            .iter()
            .position(|t| *t == "SALES_CUSTOMER")
            .expect("sc");
        it.pick(&format!("intel:{i}"));
        let a = it.take_accept().expect("accept");
        assert_eq!(
            (a.replace.clone(), a.text.as_str()),
            (14..16, "SALES_CUSTOMER sc"),
            "insert_alias = 짧은 alias까지"
        );
        assert!(!it.is_open());
        assert_eq!(it.mru[0], "SALES_CUSTOMER");
    }

    /// T-178: 내장 함수 = `NAME()` + 캐럿 안 · 패키지 멤버 · 사전 객체 · 시그니처 도움 · INSERT 컬럼 목록 조각 · 컬럼 상세 · alias.
    #[test]
    fn builtins_snippet_and_signature() {
        let m = store();
        let snap = m.snapshot();
        let view = MetaView {
            names: &m.names,
            snap,
        };
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        let ora = Some(Dialect::Oracle);
        // 식 자리 `nv` → NVL(시그니처) · 확정 = `NVL()` 캐럿 1 뒤로.
        let doc = "SELECT nv FROM emp";
        assert!(it.request(
            1,
            1,
            doc,
            9,
            ora,
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        let i = it.cands.iter().position(|c| c.text == "NVL").expect("nvl");
        assert_eq!(it.cands[i].detail, "NVL(expr1, expr2)");
        it.pick(&format!("intel:{i}"));
        let a = it.take_accept().expect("accept");
        assert_eq!((a.text.as_str(), a.caret_back), ("NVL()", 1));
        // 인자 없는 값 = 괄호 없이.
        let doc = "SELECT sysd";
        assert!(it.request(
            1,
            1,
            doc,
            doc.len(),
            ora,
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        let i = it
            .cands
            .iter()
            .position(|c| c.text == "SYSDATE")
            .expect("sysdate");
        it.pick(&format!("intel:{i}"));
        assert_eq!(it.take_accept().expect("a").text, "SYSDATE");
        // 패키지 멤버 `DBMS_OUTPUT.` → PUT_LINE …(문서 단어로 떨어지지 않음).
        let doc = "BEGIN DBMS_OUTPUT.";
        assert!(it.request(
            1,
            1,
            doc,
            doc.len(),
            ora,
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        assert!(it
            .cands
            .iter()
            .any(|c| c.text == "PUT_LINE" && c.kind == CandKind::Function));
        assert!(!it.cands.iter().any(|c| c.kind == CandKind::Word));
        // 사전 객체: FROM 뒤 `all_t` → ALL_TABLES · MSSQL `sys.` → tables.
        let doc = "SELECT * FROM all_t";
        assert!(it.request(
            1,
            1,
            doc,
            doc.len(),
            ora,
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        assert!(it.cands.iter().any(|c| c.text == "ALL_TABLES"));
        let doc = "SELECT * FROM sys.";
        let ms = Some(Dialect::Mssql);
        assert!(it.request(
            1,
            1,
            doc,
            doc.len(),
            ms,
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        assert!(it
            .cands
            .iter()
            .any(|c| c.text == "tables" && c.kind == CandKind::View));
        // 키워드 + 공백.
        assert!(it.request(
            1,
            1,
            "sel",
            3,
            ora,
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        it.pick("intel:0");
        assert_eq!(it.take_accept().expect("kw").text, "SELECT ");
        // 시그니처 도움.
        assert_eq!(
            it.signature_at("SELECT NVL(a, ", 14, ora).as_deref(),
            Some("NVL(expr1, expr2)")
        );
        assert_eq!(it.signature_at("SELECT foo(", 11, ora), None);
        // INSERT INTO emp ( → 첫 후보 = 컬럼 전체 조각 · MRU에 안 남음.
        let doc = "INSERT INTO emp (";
        assert!(it.request(
            1,
            1,
            doc,
            doc.len(),
            ora,
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        assert_eq!(it.cands[0].kind, CandKind::Snippet);
        assert_eq!(it.cands[0].text, "EMPNO, ENAME");
        it.pick("intel:0");
        assert_eq!(it.take_accept().expect("snip").text, "EMPNO, ENAME");
        assert_ne!(it.mru[0], "EMPNO, ENAME");
        // 키 표식(팝업 오른쪽 · 09-24 `이름 : 타입` 형식) · alias 생성.
        assert_eq!(key_marks(nsql_run::meta::KEY_PK, Some(false)), "PK");
        assert_eq!(key_marks(nsql_run::meta::KEY_FK, Some(false)), "FK");
        assert_eq!(key_marks(0, Some(true)), "");
        assert_eq!(gen_alias("sales_customer", &[]), "sc");
        assert_eq!(gen_alias("EMP", &["e"]), "e2");
        assert_eq!(gen_alias("MyTable", &[]), "mt");
        assert_eq!(gen_alias("orders", &[]), "o");
        assert_eq!(gen_alias("sch.item_set", &[]), "is1", "키워드 모양이면 1");
        // 건너편 컬럼(사용자 09-29): 연산자 왼쪽의 (한정) 이름 · 없으면 None.
        let d = "ON 1=1\n\tAND\tB.PROJECT_CD\t=\tA.";
        assert_eq!(peer_column(d, d.len()).as_deref(), Some("PROJECT_CD"));
        let d = "SET T.QTY = S.Q";
        assert_eq!(peer_column(d, d.len() - 1).as_deref(), Some("QTY"));
        let d = "WHERE a.x <> b.";
        assert_eq!(peer_column(d, d.len()).as_deref(), Some("x"));
        let d = "WHERE 1 = a.";
        assert_eq!(peer_column(d, d.len()), None, "숫자는 컬럼이 아니다");
        let d = "SELECT a.";
        assert_eq!(peer_column(d, d.len()), None);
        // A, B, C 방식(사용자 09-29): 쓰인 것을 건너뛰고 첫 빈 문자 · 26개 다 쓰이면 A2.
        assert_eq!(gen_alias_letter(&[]), "A");
        assert_eq!(gen_alias_letter(&["a", "B"]), "C");
        let all: Vec<String> = ('A'..='Z').map(|c| c.to_string()).collect();
        let all_ref: Vec<&str> = all.iter().map(String::as_str).collect();
        assert_eq!(gen_alias_letter(&all_ref), "A2");
        assert!(it.take_over_budget().is_none());
    }

    #[test]
    fn document_symbols_keywords_and_triggers() {
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        let doc = "DEFINE v_user = 'x'\nVARIABLE rc REFCURSOR\nSELECT v_ FROM dual";
        let caret = doc.rfind("v_").expect("v_") + 2;
        assert!(it.request(
            7,
            3,
            doc,
            caret,
            Some(Dialect::Oracle),
            None,
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        let texts: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts[0], "v_user");
        assert!(it.cands[0].kind == CandKind::Variable);
        // 문장 시작 = 키워드.
        assert!(it.request(
            7,
            3,
            "sel",
            3,
            Some(Dialect::Oracle),
            None,
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        assert_eq!(it.cands[0].text, "SELECT");
        // 트리거 규칙.
        assert!(it.after_char('.', 0), "트리거 문자 = 즉시");
        it.close();
        assert!(!it.after_char('a', 1), "min_chars 미만 = 아직");
        assert!(it.due.is_none());
        assert!(!it.after_char('b', 2), "지연 예약");
        assert!(it.due.is_some());
        assert!(it.due(Instant::now() + Duration::from_millis(300)));
        assert!(!it.after_char(' ', 0));
        // 문자열 안 = 안 뜬다.
        assert!(!it.request(
            7,
            3,
            "SELECT 'a",
            9,
            Some(Dialect::Oracle),
            None,
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
    }

    /// 09-23(사용자 "FROM 뒤 M4S_ 테이블이 안 보인다 · MI40으로도 · 권한 반영 사전 뷰 · 기본 스키마 접속 시 · 스키마. 시점 캐싱"):
    /// 버킷이 없으면 채움 요청 + "불러오는 중" · 채워지면 이름순 상한 없이 전부가 랭킹 대상 · 약어 퍼지 · 사전 버킷 우선 · 스키마. 요청 · 문서 언급 판정.
    /// ★ FROM 자리(사용자 09-24): 뷰·구체화 뷰 = 테이블과 같은 레이어(일치 등급으로 경쟁) · 함수(테이블 함수 먼저)·패키지·
    /// 프로시저 = 낮은 레이어 · `스키마.`에서도 같은 표 · 함수 확정 = `NAME()` 캐럿 안.
    /// ★ BISCM 실측 재현(사용자 09-24): 테이블 449 + 뷰 1(`VM4S_I002040`) + 프로시저 72 · 페이지 200 → 첫 페이지 200 + "322개 더" ·
    /// 끝에 닿을 때마다 200씩 이어 붙어 전부(76 §13 · T-196).
    #[test]
    fn view_survives_max_items_cut_among_many_tables() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["BISCM".into()], Some("BISCM"));
        let tables: Vec<NewObj> = (0..449)
            .map(|i| NewObj {
                name: format!("M4S_I{i:06}"),
                ..Default::default()
            })
            .collect();
        m.load_bucket("BISCM", ObjectKind::Table, &tables, 1);
        m.load_bucket(
            "BISCM",
            ObjectKind::View,
            &[NewObj {
                name: "VM4S_I002040".into(),
                ..Default::default()
            }],
            1,
        );
        m.load_bucket("BISCM", ObjectKind::MaterializedView, &[], 1);
        m.load_bucket("BISCM", ObjectKind::Synonym, &[], 1);
        m.load_bucket("BISCM", ObjectKind::Function, &[], 1);
        m.load_bucket("BISCM", ObjectKind::Package, &[], 1);
        let procs: Vec<NewObj> = (0..72)
            .map(|i| NewObj {
                name: format!("SP_{i:03}"),
                ..Default::default()
            })
            .collect();
        m.load_bucket("BISCM", ObjectKind::Procedure, &procs, 1);
        m.load_bucket(nsql_catalog::DICT_SCHEMA, ObjectKind::View, &[], 1);
        let mut c = cfg();
        c.routines = true;
        c.max_items = 200;
        let mut it = Intel::new(c);
        let view = MetaView {
            names: &m.names,
            snap: m.snapshot(),
        };
        let doc = "SELECT * FROM ";
        assert!(it.request(
            1,
            1,
            doc,
            doc.len(),
            Some(Dialect::Oracle),
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            Rect::new(0, 0, 800, 600),
            1.0,
            &|_| None
        ));
        // 449 + 1 + 스키마 이름 1 = 451(프로시저는 FROM 자리에서 제외 · §181).
        assert_eq!(it.cands.len(), 451, "한 세트 = 전부");
        let ids = it.menu.item_ids();
        assert_eq!(ids.len(), 201, "첫 페이지 200 + 안내 1");
        assert!(ids.contains(&"intel:more"), "N개 더 안내: {ids:?}");
        assert!(
            it.cands[..200].iter().all(|c| c.text.starts_with("M4S_")),
            "첫 페이지 = 같은 층에서 길이·이름순 앞 200"
        );
        // ★ 페이지 로딩(T-196): End로 끝에 닿으면 200씩 이어 붙는다 · 뷰(길이순 끝)는 셋째 페이지 · 프로시저(아래 층)는 맨 뒤.
        let end = nexa_ctl::InputEvent::Key {
            key: nexa_ctl::Key::End,
            shift: false,
            primary: false,
        };
        it.menu.on_event(&end);
        assert!(it.menu.take_reached_end() && it.extend());
        assert_eq!(it.menu.item_ids().len(), 401, "둘째 페이지 + 안내");
        it.menu.on_event(&end);
        assert!(it.menu.take_reached_end() && it.extend());
        assert_eq!(it.menu.item_ids().len(), 451, "전부 = 안내 없음");
        assert!(!it.extend(), "더 없음");
        let all: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
        let view_at = all.iter().position(|n| *n == "VM4S_I002040").expect("view");
        assert!(view_at > 200, "{view_at}");
        assert!(
            !all.iter().any(|n| n.starts_with("SP_")),
            "FROM 자리 = 프로시저 없음"
        );
    }

    /// ★ 카드 머무름(사용자 09-24): 빠르게 ↓를 세 번 → 카드는 첫 대상 그대로 · 머무름 시간이 지난 틱에 **마지막** 행만 넘긴다 ·
    /// 깨움 시각 = 마지막 변경 + settle.
    #[test]
    fn card_settles_on_last_hover_only() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["hr".into()], Some("hr"));
        let objs: Vec<NewObj> = (0..6)
            .map(|i| NewObj {
                name: format!("T{i}"),
                ..Default::default()
            })
            .collect();
        m.load_bucket("hr", ObjectKind::Table, &objs, 1);
        for k in [
            ObjectKind::View,
            ObjectKind::MaterializedView,
            ObjectKind::Synonym,
        ] {
            m.load_bucket("hr", k, &[], 1);
        }
        let mut c = cfg();
        c.card_settle_ms = 150;
        let mut it = Intel::new(c);
        let view = MetaView {
            names: &m.names,
            snap: m.snapshot(),
        };
        let doc = "SELECT * FROM hr.";
        assert!(it.request(
            1,
            1,
            doc,
            doc.len(),
            Some(Dialect::Oracle),
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            Rect::new(0, 0, 800, 600),
            1.0,
            &|_| None
        ));
        let t0 = Instant::now();
        it.note_hover(t0);
        assert!(it.card_tick(t0), "첫 대상은 즉시");
        assert_eq!(it.card_index(), Some(0));
        let down = nexa_ctl::InputEvent::Key {
            key: nexa_ctl::Key::Down,
            shift: false,
            primary: false,
        };
        for k in 1..=3 {
            it.menu.on_event(&down);
            it.note_hover(t0 + Duration::from_millis(10 * k));
            assert!(
                !it.card_tick(t0 + Duration::from_millis(10 * k)),
                "머무는 중"
            );
        }
        assert_eq!(it.card_index(), Some(0), "빠른 이동 중엔 카드 그대로");
        assert_eq!(
            it.next_wake(),
            Some(t0 + Duration::from_millis(30 + 150)),
            "마지막 변경 + settle"
        );
        assert!(!it.card_tick(t0 + Duration::from_millis(100)));
        assert!(
            it.card_tick(t0 + Duration::from_millis(200)),
            "머문 뒤 넘김"
        );
        // 처음 강조가 없으면 첫 ↓가 0행이라 세 번 뒤는 2행.
        assert_eq!(it.card_index(), Some(2), "마지막 행만");
        assert!(it.next_wake().is_none(), "넘긴 뒤 깨움 없음");
    }

    /// ★ `*`/`A.*` → "모든 컬럼 (N)" 조각(사용자 09-24): `A.*` = 그 별칭 컬럼 전부 `A.컬럼` · bare `*` + 두 테이블 = 둘 다 별칭 붙여 ·
    /// 한 테이블 별칭 없음 = 이름만 · 확정 = 별표(와 별칭)를 통째로 치환 · 한 줄에 하나씩(들여쓰기 유지) 옵션.
    #[test]
    fn star_expands_to_all_columns_with_aliases() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["hr".into()], Some("hr"));
        m.load_bucket(
            "hr",
            ObjectKind::Table,
            &[
                NewObj {
                    name: "EMP".into(),
                    ..Default::default()
                },
                NewObj {
                    name: "DEPT".into(),
                    ..Default::default()
                },
            ],
            1,
        );
        let col = |n: &str| NewCol {
            name: n.into(),
            data_type: "NUMBER".into(),
            ..Default::default()
        };
        let emp = m
            .snapshot()
            .lookup(&m.names, Some("hr"), "EMP")
            .expect("emp");
        let dept = m
            .snapshot()
            .lookup(&m.names, Some("hr"), "DEPT")
            .expect("dept");
        m.set_columns(emp, &[col("EMPNO"), col("ENAME")], 1);
        m.set_columns(dept, &[col("DEPTNO")], 1);
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        let rev = std::cell::Cell::new(0u64);
        let req = |it: &mut Intel, m: &MetaStore, doc: &str, caret: usize| {
            rev.set(rev.get() + 1);
            let view = MetaView {
                names: &m.names,
                snap: m.snapshot(),
            };
            assert!(it.request(
                1,
                rev.get(),
                doc,
                caret,
                Some(Dialect::Oracle),
                Some(&view),
                Some(Point { x: 0, y: 0 }),
                host,
                1.0,
                &|_| None
            ));
        };
        let doc = "SELECT A.* FROM EMP A, DEPT B";
        req(&mut it, &m, doc, 10);
        assert_eq!(it.cands.len(), 1, "별표 뒤 = 조각만");
        assert_eq!(it.cands[0].kind, CandKind::Snippet);
        assert_eq!(it.cands[0].text, "A.EMPNO, A.ENAME");
        assert_eq!(it.cands[0].detail, "All columns (2)");
        it.pick("intel:0");
        let acc = it.take_accept().expect("snippet");
        assert_eq!(&doc[acc.replace.clone()], "A.*", "별칭+별표 통째로 치환");
        assert_eq!(acc.text, "A.EMPNO, A.ENAME");
        let doc = "SELECT * FROM EMP A, DEPT B";
        req(&mut it, &m, doc, 8);
        assert_eq!(
            it.cands[0].text, "A.EMPNO, A.ENAME, B.DEPTNO",
            "bare * = 전 테이블 · 별칭"
        );
        let doc = "SELECT * FROM EMP";
        req(&mut it, &m, doc, 8);
        assert_eq!(
            it.cands[0].text, "EMPNO, ENAME",
            "한 테이블 · 별칭 없음 = 이름만"
        );
        // 여러 줄 = 다음 줄 맨 앞에 쉼표(들여쓰기 없음 · 사용자 09-24) · 쉼표 뒤 공백은 Space/Tab.
        let mut c = cfg();
        c.star_lines = true;
        let mut it = Intel::new(c);
        let doc = "SELECT\n    A.* FROM EMP A";
        req(&mut it, &m, doc, 14);
        assert_eq!(it.cands[0].text, "A.EMPNO\n, A.ENAME");
        let mut c = cfg();
        c.star_tab = Some(true);
        let mut it = Intel::new(c);
        let doc = "SELECT A.* FROM EMP A";
        req(&mut it, &m, doc, 10);
        assert_eq!(it.cands[0].text, "A.EMPNO,\tA.ENAME");
        // 편집기 설정 따름(기본): 호스트가 넣은 활성 탭 단위 — 탭이면 탭, 공백이면 공백.
        let mut c = cfg();
        c.star_tab = None;
        let mut it = Intel::new(c);
        it.set_editor_tab(true);
        req(&mut it, &m, doc, 10);
        assert_eq!(it.cands[0].text, "A.EMPNO,\tA.ENAME");
        it.set_editor_tab(false);
        req(&mut it, &m, doc, 10);
        assert_eq!(it.cands[0].text, "A.EMPNO, A.ENAME");
    }

    /// ★ 패키지 멤버(09-24 검토 ①~③): `FROM hr.PKG.` = 테이블 함수(층 1) → 함수(층 2) · 프로시저 제외 · 확정 = `NAME()` ·
    /// `SELECT hr.PKG.` = 전부 · `FROM hr.EMP.`(테이블 뒤 `.`) = 팝업 없음.
    #[test]
    fn package_members_in_from_chain_and_expr() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["hr".into()], Some("hr"));
        m.load_bucket(
            "hr",
            ObjectKind::Table,
            &[NewObj {
                name: "EMP".into(),
                ..Default::default()
            }],
            1,
        );
        m.load_bucket(
            "hr",
            ObjectKind::Package,
            &[NewObj {
                name: "PKG".into(),
                ..Default::default()
            }],
            1,
        );
        let pkg = m
            .snapshot()
            .lookup(&m.names, Some("hr"), "PKG")
            .expect("pkg");
        let emp = m
            .snapshot()
            .lookup(&m.names, Some("hr"), "EMP")
            .expect("emp");
        let mem = |n: &str, ty: &str, pos: u16| NewCol {
            name: n.into(),
            data_type: ty.into(),
            position: pos,
            ..Default::default()
        };
        m.set_columns(
            pkg,
            &[
                mem("PRC_LOAD", "PROCEDURE", 1),
                mem("FN_SCALAR", "FUNCTION → NUMBER", 2),
                mem("FN_ROWS", "TABLE FUNCTION → TABLE", 3),
            ],
            1,
        );
        m.set_columns(emp, &[mem("EMPNO", "NUMBER", 1)], 1);
        let mut c = cfg();
        c.show_types = true;
        // 방금 고른 것 우선(MRU)은 끈다 — 순번 검사.
        c.recent = false;
        let mut it = Intel::new(c);
        let host = Rect::new(0, 0, 800, 600);
        let rev = std::cell::Cell::new(0u64);
        let req = |it: &mut Intel, m: &MetaStore, doc: &str| -> bool {
            rev.set(rev.get() + 1);
            let view = MetaView {
                names: &m.names,
                snap: m.snapshot(),
            };
            it.request(
                1,
                rev.get(),
                doc,
                doc.len(),
                Some(Dialect::Oracle),
                Some(&view),
                Some(Point { x: 0, y: 0 }),
                host,
                1.0,
                &|_| None,
            )
        };
        assert!(req(&mut it, &m, "SELECT * FROM hr.PKG."));
        let names: Vec<(String, u8)> = it.cands.iter().map(|c| (c.text.clone(), c.layer)).collect();
        assert_eq!(
            names,
            vec![("FN_ROWS".to_string(), 1), ("FN_SCALAR".to_string(), 2)],
            "FROM 사슬 = 테이블 함수 먼저 · 프로시저 제외"
        );
        it.pick("intel:0");
        let acc = it.take_accept().expect("accept");
        assert_eq!((acc.text.as_str(), acc.caret_back), ("FN_ROWS()", 1));
        assert!(req(&mut it, &m, "SELECT hr.PKG."));
        let names: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            names,
            vec!["PRC_LOAD", "FN_SCALAR", "FN_ROWS"],
            "식 자리 = 전부 · 순번"
        );
        assert!(
            !req(&mut it, &m, "SELECT * FROM hr.EMP."),
            "테이블 뒤 `.` = 없음"
        );
    }

    /// ★ 창 방식(09-24 §187): 문턱을 넘는 문서는 캐럿 앞뒤 창만 읽고 구간은 절대 바이트 · 문서 낱말 없음 · 확정 치환 구간이 맞는다.
    #[test]
    fn window_mode_keeps_absolute_offsets_and_skips_doc_words() {
        let filler = "SELECT 1 FROM dual;\n\n".repeat(400); // ≈ 8 KB
        let doc = format!("{filler}SELECT zzword FROM emp e WHERE e.na");
        let mut c = cfg();
        c.max_doc_kb = 4; // 4 KB 문턱 → 창 방식
        c.doc_words = true;
        let mut it = Intel::new(c);
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["hr".into()], Some("hr"));
        m.load_bucket(
            "hr",
            ObjectKind::Table,
            &[NewObj {
                name: "EMP".into(),
                ..Default::default()
            }],
            1,
        );
        let emp = m
            .snapshot()
            .lookup(&m.names, Some("hr"), "EMP")
            .expect("emp");
        m.set_columns(
            emp,
            &[NewCol {
                name: "NAME".into(),
                data_type: "VARCHAR2".into(),
                ..Default::default()
            }],
            1,
        );
        let view = MetaView {
            names: &m.names,
            snap: m.snapshot(),
        };
        assert!(it.request(
            1,
            1,
            &doc,
            doc.len(),
            Some(Dialect::Oracle),
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            Rect::new(0, 0, 800, 600),
            1.0,
            &|_| None
        ));
        let names: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
        assert!(names.contains(&"NAME"), "{names:?}");
        assert!(
            !names.contains(&"zzword"),
            "창 방식 = 문서 낱말 없음: {names:?}"
        );
        it.pick("intel:0");
        let acc = it.take_accept().expect("accept");
        assert_eq!(
            &doc[acc.replace.start..acc.replace.end],
            "na",
            "치환 구간은 절대 바이트"
        );
        // 작은 문서 = 창이 전체 · 큰 문서(≈ 512 KB) = 캐럿 앞 256 KB 안의 빈 줄 경계에서 시작.
        assert_eq!(window_of(&doc, doc.len()), (0, doc.len()));
        let big = filler.repeat(64);
        let (a, b) = window_of(&big, big.len());
        assert!(
            a > 0 && b == big.len() && &big[a - 2..a] == "\n\n",
            "{a} {b}"
        );
        let (a2, b2) = window_of(&big, 10);
        assert_eq!(a2, 0);
        assert!(b2 < big.len() && big[b2..].starts_with("\n\n"), "{b2}");
    }

    /// ★ 자기 감속(09-24 §187): 예산 0 ms → 요청마다 초과 → 두 번째 요청 뒤 그 탭의 문서 낱말이 꺼지고 알림 한 번.
    #[test]
    fn budget_overrun_twice_turns_off_doc_words() {
        let mut c = cfg();
        c.budget_ms = 0;
        c.doc_words = true;
        let mut it = Intel::new(c);
        let doc = "SELECT zzword FROM emp WHERE zz";
        for rev in 1..=3 {
            let _ = it.request(
                7,
                rev,
                doc,
                doc.len(),
                Some(Dialect::Oracle),
                None,
                Some(Point { x: 0, y: 0 }),
                Rect::new(0, 0, 800, 600),
                1.0,
                &|_| None,
            );
            let has = it.cands.iter().any(|c| c.text == "zzword");
            if rev <= 2 {
                assert!(has, "rev {rev}: 아직 문서 낱말");
            } else {
                assert!(!has, "세 번째부터 문서 낱말 없음");
            }
        }
        assert_eq!(it.take_degraded(), Some(7));
        assert_eq!(it.take_degraded(), None, "한 번만");
    }

    /// ★ 문법 절 기반 키워드(사용자 09-24 "SQLite SELECT 목록에 HAVING?"): SELECT 목록 = HAVING 없음 · GROUP BY 뒤 = HAVING · 문장 시작 =
    /// `[start]`(SQLite `PRAGMA`) · 함수 괄호 안 = 전체 표(폴백).
    #[test]
    fn keywords_follow_grammar_clause() {
        let mut c = cfg();
        c.keywords = true;
        c.doc_words = false;
        let mut it = Intel::new(c);
        let host = Rect::new(0, 0, 800, 600);
        let rev = std::cell::Cell::new(0u64);
        let req = |it: &mut Intel, doc: &str| -> Vec<String> {
            rev.set(rev.get() + 1);
            let _ = it.request(
                1,
                rev.get(),
                doc,
                doc.len(),
                Some(Dialect::Sqlite),
                None,
                Some(Point { x: 0, y: 0 }),
                host,
                1.0,
                &|_| None,
            );
            it.cands
                .iter()
                .filter(|c| c.kind == CandKind::Keyword)
                .map(|c| c.text.clone())
                .collect()
        };
        let k = req(&mut it, "SELECT a, fr");
        assert!(k.iter().any(|x| x == "FROM"), "{k:?}");
        let k = req(&mut it, "SELECT a, ha");
        assert!(
            !k.iter().any(|x| x == "HAVING"),
            "SELECT 목록 뒤에 HAVING 없음: {k:?}"
        );
        let k = req(&mut it, "SELECT a FROM t GROUP BY a ha");
        assert!(k.iter().any(|x| x == "HAVING"), "{k:?}");
        let k = req(&mut it, "pra");
        assert!(
            k.iter().any(|x| x == "PRAGMA") && !k.iter().any(|x| x == "HAVING"),
            "{k:?}"
        );
        let k = req(&mut it, "SELECT coalesce(a, gr");
        assert!(
            k.iter().any(|x| x == "GROUP BY"),
            "함수 괄호 안 = 전체 표: {k:?}"
        );
    }

    /// ★ SQL Server FROM 자리(사용자 09-24): 프로시저 없음 · 스칼라 함수(`FN`) 없음 · 테이블 반환 함수(`TF`)만 · `FROM dbo.`도 같다.
    #[test]
    fn mssql_from_hides_procedures_and_scalar_functions() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["dbo".into()], Some("dbo"));
        let obj = |n: &str, extra: Option<&str>| NewObj {
            name: n.into(),
            extra: extra.map(String::from),
            ..Default::default()
        };
        m.load_bucket("dbo", ObjectKind::Table, &[obj("T1", None)], 1);
        for k in [ObjectKind::View, ObjectKind::Synonym] {
            m.load_bucket("dbo", k, &[], 1);
        }
        m.load_bucket(
            "dbo",
            ObjectKind::Function,
            &[obj("FN_SCALAR", Some("FN")), obj("TF_ROWS", Some("TF"))],
            1,
        );
        m.load_bucket("dbo", ObjectKind::Procedure, &[obj("SP_TEST", None)], 1);
        m.load_bucket(nsql_catalog::DICT_SCHEMA, ObjectKind::View, &[], 1);
        let mut c = cfg();
        c.routines = true;
        let mut it = Intel::new(c);
        let view = MetaView {
            names: &m.names,
            snap: m.snapshot(),
        };
        for (i, doc) in ["SELECT * FROM ", "SELECT * FROM dbo."].iter().enumerate() {
            assert!(it.request(
                1,
                i as u64 + 1,
                doc,
                doc.len(),
                Some(Dialect::Mssql),
                Some(&view),
                Some(Point { x: 0, y: 0 }),
                Rect::new(0, 0, 800, 600),
                1.0,
                &|_| None
            ));
            let names: Vec<&str> = it.cands.iter().map(|c| c.text.as_str()).collect();
            assert!(
                names.contains(&"T1") && names.contains(&"TF_ROWS"),
                "{doc}: {names:?}"
            );
            assert!(
                !names.contains(&"SP_TEST") && !names.contains(&"FN_SCALAR"),
                "{doc}: {names:?}"
            );
        }
    }

    #[test]
    fn from_lists_views_with_tables_and_routines_lower() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["hr".into()], Some("hr"));
        let obj = |n: &str, extra: Option<&str>| NewObj {
            name: n.into(),
            extra: extra.map(String::from),
            ..Default::default()
        };
        m.load_bucket("hr", ObjectKind::Table, &[obj("EMP", None)], 1);
        m.load_bucket("hr", ObjectKind::View, &[obj("EMP_V", None)], 1);
        m.load_bucket(
            "hr",
            ObjectKind::MaterializedView,
            &[obj("EMP_MV", None)],
            1,
        );
        m.load_bucket("hr", ObjectKind::Synonym, &[], 1);
        m.load_bucket(
            "hr",
            ObjectKind::Function,
            &[obj("EMP_FN", Some("PIPELINED")), obj("EMP_SCALAR", None)],
            1,
        );
        m.load_bucket("hr", ObjectKind::Package, &[obj("EMP_PKG", None)], 1);
        m.load_bucket("hr", ObjectKind::Procedure, &[obj("EMP_PRC", None)], 1);
        let mut c = cfg();
        c.routines = true;
        c.show_types = true;
        let mut it = Intel::new(c);
        let host = Rect::new(0, 0, 800, 600);
        let rev = std::cell::Cell::new(0u64);
        let req = |it: &mut Intel, m: &MetaStore, doc: &str| {
            rev.set(rev.get() + 1);
            let view = MetaView {
                names: &m.names,
                snap: m.snapshot(),
            };
            assert!(it.request(
                1,
                rev.get(),
                doc,
                doc.len(),
                Some(Dialect::Oracle),
                Some(&view),
                Some(Point { x: 0, y: 0 }),
                host,
                1.0,
                &|_| None
            ));
            it.cands
                .iter()
                .map(|c| (c.text.clone(), c.layer, c.detail.clone()))
                .collect::<Vec<_>>()
        };
        // 1) `스키마.` 접두 없이: 레이어 0(테이블·뷰·MV) → 1(테이블 함수) → 2(함수·패키지) → 3(프로시저) · 모두 읽었으니 요청 없음.
        let got = req(&mut it, &m, "SELECT * FROM hr.");
        assert!(!it.is_loading() && it.take_need_objects().is_empty());
        let names: Vec<&str> = got.iter().map(|(n, _, _)| n.as_str()).collect();
        let pos = |n: &str| names.iter().position(|x| *x == n).unwrap_or(usize::MAX);
        assert!(
            pos("EMP") < pos("EMP_FN")
                && pos("EMP_V") < pos("EMP_FN")
                && pos("EMP_MV") < pos("EMP_FN"),
            "{names:?}"
        );
        assert!(
            pos("EMP_FN") < pos("EMP_SCALAR") && pos("EMP_FN") < pos("EMP_PKG"),
            "{names:?}"
        );
        assert!(
            !names.contains(&"EMP_PRC"),
            "FROM 자리 = 프로시저 없음: {names:?}"
        );
        assert!(
            got.iter()
                .any(|(n, l, d)| n == "EMP_FN" && *l == 1 && d == "table function"),
            "{got:?}"
        );
        assert!(
            got.iter().any(|(n, l, _)| n == "EMP_V" && *l == 0),
            "{got:?}"
        );
        // 2) 접두 `EMP_` = 뷰·MV가 함수보다 위(같은 접두 등급 · 레이어) · 테이블 `EMP`는 접두 불일치로 제외.
        let got = req(&mut it, &m, "SELECT * FROM hr.EMP_");
        let names: Vec<&str> = got.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec!["EMP_V", "EMP_MV", "EMP_FN", "EMP_PKG", "EMP_SCALAR"],
            "{names:?}"
        );
        // 3) 현재 스키마 FROM(접두 없음)에서도 같은 표 · 함수 확정 = `EMP_FN()` + 캐럿 안.
        let got = req(&mut it, &m, "SELECT * FROM EMP_F");
        let i = got
            .iter()
            .position(|(n, _, _)| n == "EMP_FN")
            .expect("EMP_FN");
        it.pick(&format!("intel:{i}"));
        let acc = it.take_accept().expect("accept");
        assert_eq!(acc.text, "EMP_FN()");
        assert_eq!(acc.caret_back, 1);
    }

    #[test]
    fn relation_requests_missing_buckets_then_lists_all_and_fuzzy_abbrev() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["dbo".into(), "sales".into()], Some("dbo"));
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        // 본문 세대(rev)가 문서 캐시의 열쇠라 호출마다 올린다.
        let rev = std::cell::Cell::new(0u64);
        let req = |it: &mut Intel, m: &MetaStore, doc: &str| {
            rev.set(rev.get() + 1);
            let view = MetaView {
                names: &m.names,
                snap: m.snapshot(),
            };
            assert!(it.request(
                1,
                rev.get(),
                doc,
                doc.len(),
                Some(Dialect::Mssql),
                Some(&view),
                Some(Point { x: 0, y: 0 }),
                host,
                1.0,
                &|_| None
            ));
            it.cands.iter().map(|c| c.text.clone()).collect::<Vec<_>>()
        };
        // 1) 아무 버킷도 없음 → 현재 스키마(dbo)·사전 채움 요청 · 불러오는 중 · 정적 사전 표는 폴백으로 남는다.
        let texts = req(&mut it, &m, "SELECT * FROM ");
        assert!(it.is_loading(), "객체를 기다리는 중");
        let needs = it.take_need_objects();
        assert!(
            needs.iter().any(|s| s == "dbo")
                && needs.iter().any(|s| s == nsql_catalog::DICT_SCHEMA),
            "{needs:?}"
        );
        assert!(
            texts.contains(&"dbo".to_string()) && texts.contains(&"sys.tables".to_string()),
            "{texts:?}"
        );
        // 2) 테이블 300개(이름순으로 M4S_*가 200번째 뒤) + 권한 반영 사전 버킷 → 상한 없이 · 퍼지 약어 · 사전은 서버 것.
        let mut objs: Vec<NewObj> = (0..300)
            .map(|i| NewObj {
                name: format!("A{i:03}"),
                ..Default::default()
            })
            .collect();
        objs.push(NewObj {
            name: "M4S_I002040".into(),
            ..Default::default()
        });
        objs.push(NewObj {
            name: "M4S_I002050".into(),
            ..Default::default()
        });
        m.load_bucket("dbo", ObjectKind::Table, &objs, 1);
        m.load_bucket("dbo", ObjectKind::View, &[], 1);
        m.load_bucket("dbo", ObjectKind::Synonym, &[], 1);
        m.load_bucket(
            nsql_catalog::DICT_SCHEMA,
            ObjectKind::View,
            &[
                NewObj {
                    name: "sys.tables".into(),
                    ..Default::default()
                },
                NewObj {
                    name: "sys.dm_exec_sessions".into(),
                    ..Default::default()
                },
                NewObj {
                    name: "INFORMATION_SCHEMA.TABLES".into(),
                    ..Default::default()
                },
            ],
            1,
        );
        let texts = req(&mut it, &m, "SELECT * FROM M4S_");
        assert!(!it.is_loading());
        assert!(it.take_need_objects().is_empty());
        assert_eq!(
            &texts[..2],
            &["M4S_I002040".to_string(), "M4S_I002050".to_string()],
            "{texts:?}"
        );
        let texts = req(&mut it, &m, "SELECT * FROM MI40");
        assert!(
            texts.contains(&"M4S_I002040".to_string()),
            "약어 퍼지: {texts:?}"
        );
        let texts = req(&mut it, &m, "SELECT * FROM sys");
        assert!(
            texts.contains(&"sys.dm_exec_sessions".to_string()),
            "서버 사전 버킷: {texts:?}"
        );
        assert!(
            !texts.contains(&"sys.columns".to_string()),
            "정적 표는 사전을 읽은 뒤에는 안 쓴다: {texts:?}"
        );
        // 3) `sys.` = 사전 버킷의 뒷부분 · `sales.` = 그 스키마 채움 요청(스키마. 시점 캐싱) · 문서 언급 판정.
        let texts = req(&mut it, &m, "SELECT * FROM sys.");
        assert!(
            texts.contains(&"tables".to_string())
                && texts.contains(&"dm_exec_sessions".to_string()),
            "{texts:?}"
        );
        let _ = req(&mut it, &m, "SELECT * FROM sales.");
        assert!(it.is_loading());
        assert_eq!(it.take_need_objects(), vec!["sales".to_string()]);
        assert!(it.doc_mentions("SALES") && !it.doc_mentions("zzz"));
    }

    /// JOIN 다중 테이블 · 접두 없음(사용자 09-24): 모든 alias 컬럼(오른쪽 = alias · FROM 순서 → 순번) · 확정 = `e.EMPNO` · Alt = `EMPNO`.
    #[test]
    fn unqualified_columns_from_all_aliases_and_alt_plain() {
        let m = store();
        let view = MetaView {
            names: &m.names,
            snap: m.snapshot(),
        };
        let mut it = Intel::new(cfg());
        let host = Rect::new(0, 0, 800, 600);
        let doc = "SELECT em FROM emp e, dept d";
        assert!(it.request(
            1,
            1,
            doc,
            9,
            Some(Dialect::Oracle),
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        let i = it
            .cands
            .iter()
            .position(|c| c.text == "EMPNO")
            .expect("EMPNO");
        assert_eq!(it.cands[i].qualifier, "e");
        assert!(it.loading, "dept 컬럼은 아직 = 채움 요청");
        assert!(it
            .take_needs()
            .iter()
            .any(|n| n.table == "dept" && n.urgent));
        it.pick(&format!("intel:{i}"));
        assert_eq!(
            it.take_accept().expect("a").text,
            "e.EMPNO",
            "기본 = alias 붙임"
        );
        assert!(it.request(
            1,
            2,
            doc,
            9,
            Some(Dialect::Oracle),
            Some(&view),
            Some(Point { x: 0, y: 0 }),
            host,
            1.0,
            &|_| None
        ));
        let i = it
            .cands
            .iter()
            .position(|c| c.text == "EMPNO")
            .expect("EMPNO");
        it.pick_with(&format!("intel:{i}"), true);
        assert_eq!(it.take_accept().expect("a").text, "EMPNO", "Alt = 컬럼만");
    }
}
