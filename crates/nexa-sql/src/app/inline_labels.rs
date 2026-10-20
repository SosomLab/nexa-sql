//! ★ 줄 끝 라벨 합성기(docs/111 · T-337 북마크 Ctrl 홀드 · T-338 객체 설명 · 사용자 10-13).
//!
//! 새 부품은 이것 하나 — 그리기(nexa-ctl `TextBox::set_inline_labels` = 줄당 라벨 하나) · 객체 판정(`objlink::scan` + `Resolver`) ·
//! 설명 원천(MetaStore 코멘트 = hover 카드와 같은 값 `objlink_describe`) · Ctrl 상태(`App.primary`)는 있는 것을 그대로 쓴다.
//! 북마크 라벨과 객체 설명은 **같은 개념**(상시 켬 / Ctrl 홀드 켬 / 범위 키 하나 공유 · D-282)으로 다룬다.
//!
//! - 순수 함수 둘(52 §12 규칙 · 조건 2개 이상 = MC/DC 시험): [`wanted`](무엇을 어느 범위에) · [`compose`](한 줄 합성).
//! - 다시 계산 = 사건마다 한 번([`App::inline_tick`]의 열쇠 비교 · 매 프레임 그리기 비용 0): Ctrl 누름/뗌 · 본문 변경 · 캐럿 문장(statement) ·
//!   보이는 영역(visible) · 메타 stamp · 북마크 변경(`bm_refresh_tab`).
//! - 데이터 = 캐시에 있는 코멘트만(왕복 0 · 61 §1-8) · 코멘트가 아직 없는 객체만 1회 선적재(`objlink_prefetch` · D-284 · 응답 = stamp 변화 →
//!   다시 합성).
//! - 향상 모드 = `perf` FORCED(`ctrl.bookmark_label`·`ctrl.desc_inline` = off · 종전 `bookmark.inline_label_ctrl`·`editor.desc_inline_ctrl` · D-313) · 큰 파일 모드 = Ctrl 표시 꺼짐 · `document` 범위 +
//!   부분 분석(`objlink.max_kb` 초과) = 꺼짐.

use crate::*;
use std::collections::BTreeMap;

/// Ctrl 표시 범위(`ctrl.desc_scope` · 종전 `editor.inline_ctrl_scope`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Scope {
    #[default]
    Statement,
    Visible,
    Document,
}

impl Scope {
    pub(crate) fn parse(s: &str) -> Scope {
        match s {
            "document" => Scope::Document,
            "visible" => Scope::Visible,
            _ => Scope::Statement,
        }
    }
}

/// 설명 대상(`editor.desc_inline_what`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum What {
    Table,
    Column,
    #[default]
    Both,
}

impl What {
    pub(crate) fn parse(s: &str) -> What {
        match s {
            "table" => What::Table,
            "column" => What::Column,
            _ => What::Both,
        }
    }

    /// 이 종류의 링크를 설명 대상으로 삼는가(루틴·제약은 테이블/컬럼 설명이 아니다).
    pub(crate) fn takes(self, kind: crate::app::objlink::LinkKind) -> bool {
        use crate::app::objlink::LinkKind as K;
        matches!(
            (self, kind),
            (What::Table | What::Both, K::Table) | (What::Column | What::Both, K::Column)
        )
    }
}

/// 합성 조건(설정 + 문서 상태).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Cfg {
    pub bm_always: bool,
    pub bm_ctrl: bool,
    pub desc_always: bool,
    pub desc_ctrl: bool,
    /// 객체 설명의 Ctrl 범위(`ctrl.desc_scope`).
    pub scope: Scope,
    /// 북마크 라벨의 Ctrl 범위(`ctrl.bookmark_scope` · 사용자 10-13 = 따로 · 기본 visible).
    pub bm_scope: Scope,
    /// 큰 파일 모드(L1 이상) — Ctrl 표시 둘 다 꺼짐(objlink와 같은 규칙).
    pub large: bool,
    /// 부분 분석(문서가 `objlink.max_kb`보다 큼) — `document` 범위는 보이는 영역으로 낮춘다.
    pub partial: bool,
    /// ★ 문서 용량 단계(사용자 10-13): `doc_kb` = 지금 문서(KB) · `full_kb` = 이 용량까지 `document` 그대로(0 = 제한 없음) ·
    /// `off_kb` = 이 용량을 넘으면 Ctrl 표시 끔(0 = 큰 파일 모드만).
    pub doc_kb: usize,
    pub full_kb: usize,
    pub off_kb: usize,
}

/// 결론: 무엇을 어느 범위에(없음 = `None`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Want {
    pub bookmarks: Option<Scope>,
    pub descs: Option<Scope>,
}

/// ★ 무엇을 어느 범위에 그릴지(순수 · 111 §2): 북마크 상시 = 문서 전체(지금 규칙) · Ctrl 홀드 = 범위 키 · 설명 상시도 범위 키 ·
/// 큰 파일 = Ctrl 표시 없음 · `document` 범위인데 부분 분석이면 없음.
pub(crate) fn wanted(cfg: &Cfg, ctrl: bool) -> Want {
    // 용량 단계: > off(또는 큰 파일 모드) = 끔 · > full 또는 부분 분석 = `document` → 보이는 영역 · 그 밖 = 설정대로.
    let off = cfg.large || (cfg.off_kb > 0 && cfg.doc_kb > cfg.off_kb);
    let lower_doc = cfg.partial || (cfg.full_kb > 0 && cfg.doc_kb > cfg.full_kb);
    let scoped = |on: bool, scope: Scope| -> Option<Scope> {
        if !on || off {
            None
        } else if scope == Scope::Document && lower_doc {
            Some(Scope::Visible)
        } else {
            Some(scope)
        }
    };
    let bookmarks = if cfg.bm_always {
        Some(Scope::Document)
    } else {
        scoped(cfg.bm_ctrl && ctrl, cfg.bm_scope)
    };
    let descs = if cfg.desc_always {
        scoped(true, cfg.scope)
    } else {
        scoped(cfg.desc_ctrl && ctrl, cfg.scope)
    };
    Want { bookmarks, descs }
}

/// ★ "현재 문장"의 줄 범위를 **딸린 주석까지** 넓힌다(사용자 10-13 "주석 영역의 북마크가 Ctrl에도 안 보인다"): 문장 바로 위의 연속 주석 줄
/// (`--` 줄 · `/* … */` 블록 · 사이의 빈 줄은 건너뛰되 끝은 주석 줄에서) · 끝의 같은 줄 꼬리 주석은 이미 그 줄이라 그대로.
/// 그 위의 문장(`;`로 끝나는 코드 줄)이나 코드 줄을 만나면 멈춘다 — 앞 문장의 꼬리 주석을 가져오지 않는다. 순수(`line_at` = 줄 글).
pub(crate) fn widen_comment_lines(
    line_at: &dyn Fn(usize) -> String,
    la: usize,
    lb: usize,
) -> (usize, usize) {
    let mut top = la;
    let mut l = la;
    let mut in_block = false;
    while l > 0 {
        l -= 1;
        let raw = line_at(l);
        let t = raw.trim();
        if in_block {
            // 여는 줄: `/*`로 **시작**하는 줄만 주석 줄 · 코드 뒤에 붙은 `… /* x`는 코드 줄이라 거기서 멈춘다(124차 리뷰).
            if let Some(p) = t.find("/*") {
                if p == 0 {
                    top = l;
                }
                in_block = false;
                if p != 0 {
                    break;
                }
            } else {
                top = l;
            }
            continue;
        }
        if t.is_empty() {
            continue;
        }
        if t.starts_with("--") {
            top = l;
        } else if t.starts_with("/*") && t.ends_with("*/") {
            // 한 줄짜리 블록 주석.
            top = l;
        } else if t.ends_with("*/") && !t.contains("/*") {
            // 블록 주석의 닫는 줄 → 여는 줄까지 올라간다(여는 줄이 코드 뒤 `/*`면 그 줄은 제외).
            top = l;
            in_block = true;
        } else {
            break;
        }
    }
    (top, lb)
}

/// 글자 수 상한(넘치면 `…`).
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        s.chars()
            .take(max.saturating_sub(1))
            .chain(['\u{2026}'])
            .collect()
    } else {
        s.to_string()
    }
}

/// ★ 한 줄 합성(순수 · 111 §6): `◆ 북마크`(이미 꼴 갖춤) 먼저 · 설명은 ` · ` 뒤 `» 설명` 등장 순서 · 설명 부분만 `desc_max`에서 `…` ·
/// 빈 설명은 그리지 않는다 · 둘 다 없으면 `None`(줄 끝에 아무것도 안 그림).
pub(crate) fn compose(bookmark: Option<&str>, descs: &[String], desc_max: usize) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(b) = bookmark.map(str::trim).filter(|b| !b.is_empty()) {
        parts.push(b.to_string());
    }
    let ds: Vec<String> = descs
        .iter()
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .map(|d| format!("\u{bb} {d}"))
        .collect();
    if !ds.is_empty() {
        parts.push(clip(&ds.join(" \u{b7} "), desc_max.max(8)));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" \u{b7} "))
    }
}

impl App {
    fn inline_cfg(&self) -> Cfg {
        let i = self.editors.active();
        Cfg {
            bm_always: self.bookmarks.enabled && self.settings.flag("bookmark.inline_label"),
            bm_ctrl: self.bookmarks.enabled && self.settings.flag("ctrl.bookmark_label"),
            desc_always: self.settings.flag("editor.desc_inline") && self.inline_desc_suitable(),
            desc_ctrl: self.settings.flag("ctrl.desc_inline") && self.inline_desc_suitable(),
            scope: Scope::parse(self.settings.get("ctrl.desc_scope").unwrap_or("statement")),
            bm_scope: Scope::parse(
                self.settings
                    .get("ctrl.bookmark_scope")
                    .unwrap_or("visible"),
            ),
            large: self.editors.is_large(i),
            partial: self.objlink_partial(),
            doc_kb: self.editors.cur().buf().len_bytes() / 1024,
            full_kb: self.settings.int("ctrl.full_kb").max(0) as usize,
            off_kb: self.settings.int("ctrl.off_kb").max(0) as usize,
        }
    }

    /// 설명을 붙일 수 있는 탭인가(SQL 구문 · 객체 링크 켜짐 · 접속 있음).
    fn inline_desc_suitable(&self) -> bool {
        self.editors.syntax_name() == "SQL"
            && self.settings.flag("objlink.enabled")
            && self.sess.spec.is_some()
    }

    /// 상시 설명 모드인가 — objlink 분석을 Ctrl 없이도 유지해야 한다(밑줄은 그리지 않는다 · `objlink_apply_marks`).
    pub(crate) fn inline_desc_always(&self) -> bool {
        self.settings.flag("editor.desc_inline")
            && self.inline_desc_suitable()
            && !self.editors.is_large(self.editors.active())
    }

    /// 범위 → 글자 구간(`[a, b)`) · `Statement` = 캐럿 문장(부분 분석이면 분석 구간 안에서만 찾는다 = 큰 문서 전체를 훑지 않는다).
    fn inline_range(&self, scope: Scope) -> Option<(usize, usize)> {
        let tb = self.editors.cur();
        let len = tb.buf().len();
        match scope {
            Scope::Document => Some((0, len)),
            Scope::Visible => tb.visible_range().map(|(a, b)| (a.min(len), b.min(len))),
            Scope::Statement => {
                let region = (self.objlinks.active
                    && self.objlinks.partial
                    && self.objlinks.tab == self.editors.active_id())
                .then_some(self.objlinks.region);
                let (base, text): (usize, String) = match region {
                    Some(r) => (r.0, tb.buf().slice(r.0, r.1.min(len)).into_owned()),
                    None => (0, tb.text()),
                };
                let caret_abs = tb.caret();
                if let Some(r) = region {
                    if caret_abs < r.0 || caret_abs > r.1 {
                        return None; // 분석 구간 밖(124차 리뷰 · `saturating_sub` = 구간 첫 문장으로 오판)
                    }
                }
                let caret = caret_abs - base;
                let byte_pos = text
                    .char_indices()
                    .nth(caret)
                    .map_or(text.len(), |(b, _)| b);
                let it = nsql_script::statement_at_in(&text, byte_pos, Some(self.sess.dialect))?;
                let s = it.span.start.min(text.len());
                let e = it.span.end.clamp(s, text.len());
                let a = text[..s].chars().count();
                let b = a + text[s..e].chars().count();
                Some((base + a, base + b))
            }
        }
    }

    /// 북마크용 줄 범위(둘 다 포함): `Document` = 전부 · `Visible` = 보이는 줄 · `Statement` = 캐럿 문장 줄 + 딸린 주석 줄.
    fn inline_line_range(&self, scope: Scope) -> Option<(usize, usize)> {
        if scope == Scope::Document {
            return Some((0, usize::MAX));
        }
        let (a, b) = self.inline_range(scope)?;
        let buf = self.editors.cur().buf();
        let (la, lb) = (buf.line_of(a), buf.line_of(b.saturating_sub(1).max(a)));
        if scope == Scope::Statement {
            let line_at = |n: usize| buf.line_text(n).into_owned();
            Some(widen_comment_lines(&line_at, la, lb))
        } else {
            Some((la, lb))
        }
    }

    /// 틱: 열쇠(탭 · 본문 세대 · Ctrl · 보이는 영역 · 캐럿 · 메타 stamp · 링크 수)가 바뀌었을 때만 다시 합성(매 프레임 비용 = 비교 하나).
    pub(crate) fn inline_tick(&mut self) {
        let tb = self.editors.cur();
        let stmt_scope = |key: &str, def: &str| {
            Scope::parse(self.settings.get(key).unwrap_or(def)) == Scope::Statement
        };
        let caret = if stmt_scope("ctrl.desc_scope", "statement")
            || stmt_scope("ctrl.bookmark_scope", "visible")
        {
            tb.caret()
        } else {
            0
        };
        let stamp = self.explorer.meta_view(self.sess.spec.as_ref()).1.stamp;
        let key = format!(
            "{}|{}|{}|{:?}|{caret}|{stamp}|{}",
            self.editors.active_id(),
            tb.rev(),
            self.primary,
            tb.visible_range(),
            self.objlinks.links.len()
        );
        if key != self.inline_key {
            // 탭이 바뀌면 "마지막으로 쓴 라벨"은 **다른 탭의 것** → 비교 캐시를 비워 이 탭에 반드시 다시 쓴다(124차 리뷰:
            //   A에서 Ctrl 라벨 → B로 → 뗌 → A 복귀 때 합성 `[]` == 캐시 `[]`라 A의 옛 라벨이 남았다).
            let tab_changed = self.inline_key.split('|').next()
                != Some(self.editors.active_id().to_string().as_str());
            if tab_changed {
                self.inline_last = inline_stale();
            }
            self.inline_key = key;
            self.inline_sync();
        }
    }

    /// ★ 활성 탭의 줄 끝 라벨을 다시 합성해 넣는다(111 §2) — 북마크 라벨 + 객체 설명 → 줄당 하나.
    pub(crate) fn inline_sync(&mut self) {
        let i = self.editors.active();
        let cfg = self.inline_cfg();
        let want = wanted(&cfg, self.primary);
        let mut lines: BTreeMap<usize, (Option<String>, Vec<String>)> = BTreeMap::new();
        if let Some(sc) = want.bookmarks {
            let cap = self.settings.int("bookmark.inline_label_chars").max(8) as usize;
            let (la, lb) = self.inline_line_range(sc).unwrap_or((1, 0));
            for (l, s) in self.bookmarks.inline_for(&self.editors, i, cap) {
                if l >= la && l <= lb {
                    lines.entry(l).or_default().0 = Some(s);
                }
            }
        }
        if let Some(sc) = want.descs {
            // 링크 분석이 낡았으면(스크롤로 구간 밖 · 본문 변경) 다시 — Ctrl이나 상시 모드가 켜져 있어 `objlink_sync`가 분석한다.
            if !self.objlink_fresh() {
                self.objlink_sync();
            }
            if self.objlinks.active && self.objlinks.tab == self.editors.active_id() {
                if let Some((a, b)) = self.inline_range(sc) {
                    let what = What::parse(
                        self.settings
                            .get("editor.desc_inline_what")
                            .unwrap_or("both"),
                    );
                    let mut need: Vec<usize> = Vec::new();
                    for k in 0..self.objlinks.links.len() {
                        let l = &self.objlinks.links[k];
                        if l.range.0 < a || l.range.0 >= b || !l.known || !what.takes(l.kind) {
                            continue;
                        }
                        let line = self.editors.cur().buf().line_of(l.range.0);
                        match self.objlink_describe(k) {
                            Some((_, _, Some(d))) => lines.entry(line).or_default().1.push(d),
                            Some(_) => need.push(k),
                            None => {}
                        }
                    }
                    // D-284: 코멘트가 아직 없는 객체만 **1회** 선적재(같은 탭·같은 이름은 다시 묻지 않는다) · 응답 = stamp 변화 → 다시 합성.
                    let tab = self.editors.active_id();
                    // 열쇠 = (탭, 접속 문자열 + 이름) — 같은 탭이 다른 서버에 붙으면 같은 이름도 다시 1회(124차 리뷰).
                    let conn = self
                        .sess
                        .spec
                        .as_ref()
                        .map(|s| s.connection_string())
                        .unwrap_or_default();
                    for k in need {
                        let name = format!("{conn}\u{1}{}", self.objlinks.links[k].qualified(true));
                        if self.inline_prefetched.insert((tab, name)) {
                            self.objlink_prefetch(k);
                        }
                    }
                }
            }
        }
        let desc_max = self.settings.int("editor.desc_inline_chars").clamp(8, 200) as usize;
        let labels: Vec<(usize, String)> = lines
            .into_iter()
            .filter_map(|(l, (bm, ds))| compose(bm.as_deref(), &ds, desc_max).map(|s| (l, s)))
            .collect();
        if labels != self.inline_last {
            if let Some(tb) = self.editors.tab_box_mut(i) {
                tb.set_inline_labels(labels.clone());
            }
            self.inline_last = labels;
            self.redraw();
        }
    }

    /// 비교 캐시 무효 표식 — 어떤 합성 결과와도 같지 않다(탭 전환 · 북마크 쪽이 TextBox 라벨을 직접 바꾼 뒤).
    pub(crate) fn inline_stale_mark(&mut self) {
        self.inline_last = inline_stale();
    }

    /// 자체 시험 덤프(`editor.inline_dump:<경로>`): `줄\t라벨` 한 줄씩(줄 번호 0 기준).
    pub(crate) fn inline_dump_text(&self) -> String {
        let mut s = String::new();
        for (l, lab) in self.inline_last.iter().filter(|(l, _)| *l != usize::MAX) {
            s.push_str(&format!("{l}\t{lab}\n"));
        }
        s
    }
}

/// 비교 캐시 무효 값(줄 번호 `usize::MAX` = 실제 합성에 나올 수 없다).
fn inline_stale() -> Vec<(usize, String)> {
    vec![(usize::MAX, String::new())]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Cfg {
        Cfg {
            bm_always: false,
            bm_ctrl: true,
            desc_always: false,
            desc_ctrl: true,
            scope: Scope::Statement,
            bm_scope: Scope::Statement,
            large: false,
            partial: false,
            doc_kb: 10,
            full_kb: 256,
            off_kb: 2048,
        }
    }

    /// MC/DC(52 §12): 조건 하나씩 뒤집어 결과가 독립적으로 바뀐다.
    #[test]
    fn wanted_each_condition_flips_independently() {
        let c = base();
        // 기준: Ctrl 눌림 = 둘 다 현재 문장.
        assert_eq!(
            wanted(&c, true),
            Want {
                bookmarks: Some(Scope::Statement),
                descs: Some(Scope::Statement)
            }
        );
        // Ctrl 뗌 = 둘 다 없음.
        assert_eq!(wanted(&c, false), Want::default());
        // 북마크 Ctrl 끔 = 북마크만 없음.
        assert_eq!(
            wanted(
                &Cfg {
                    bm_ctrl: false,
                    ..c
                },
                true
            )
            .bookmarks,
            None
        );
        assert_eq!(
            wanted(
                &Cfg {
                    bm_ctrl: false,
                    ..c
                },
                true
            )
            .descs,
            Some(Scope::Statement)
        );
        // 설명 Ctrl 끔 = 설명만 없음.
        assert_eq!(
            wanted(
                &Cfg {
                    desc_ctrl: false,
                    ..c
                },
                true
            )
            .descs,
            None
        );
        // 북마크 상시 = Ctrl 없이도 문서 전체.
        assert_eq!(
            wanted(
                &Cfg {
                    bm_always: true,
                    ..c
                },
                false
            )
            .bookmarks,
            Some(Scope::Document)
        );
        // 설명 상시 = Ctrl 없이도 범위 키대로.
        assert_eq!(
            wanted(
                &Cfg {
                    desc_always: true,
                    scope: Scope::Visible,
                    ..c
                },
                false
            )
            .descs,
            Some(Scope::Visible)
        );
        // 큰 파일 = Ctrl 표시 없음(상시 북마크는 그대로).
        assert_eq!(wanted(&Cfg { large: true, ..c }, true), Want::default());
        assert_eq!(
            wanted(
                &Cfg {
                    large: true,
                    bm_always: true,
                    ..c
                },
                true
            )
            .bookmarks,
            Some(Scope::Document)
        );
        // document 범위 + 부분 분석 = 보이는 영역으로 낮춤 · 부분 아님 = 문서 전체.
        assert_eq!(
            wanted(
                &Cfg {
                    scope: Scope::Document,
                    bm_scope: Scope::Document,
                    partial: true,
                    ..c
                },
                true
            ),
            Want {
                bookmarks: Some(Scope::Visible),
                descs: Some(Scope::Visible)
            }
        );
        assert_eq!(
            wanted(
                &Cfg {
                    scope: Scope::Document,
                    ..c
                },
                true
            )
            .descs,
            Some(Scope::Document)
        );
        // ★ 용량 단계(사용자 10-13): ≤ full = 그대로 · > full = document → visible(statement·visible은 그대로) · > off = 끔 · 0 = 제한 없음.
        let big = Cfg {
            scope: Scope::Document,
            bm_scope: Scope::Statement,
            doc_kb: 300,
            ..c
        };
        assert_eq!(
            wanted(&big, true),
            Want {
                bookmarks: Some(Scope::Statement),
                descs: Some(Scope::Visible)
            }
        );
        assert_eq!(
            wanted(&Cfg { doc_kb: 256, ..big }, true).descs,
            Some(Scope::Document)
        );
        assert_eq!(
            wanted(
                &Cfg {
                    full_kb: 0,
                    doc_kb: 5000,
                    off_kb: 0,
                    ..big
                },
                true
            )
            .descs,
            Some(Scope::Document)
        );
        assert_eq!(
            wanted(
                &Cfg {
                    doc_kb: 3000,
                    ..big
                },
                true
            ),
            Want::default()
        );
        assert_eq!(
            wanted(
                &Cfg {
                    doc_kb: 3000,
                    bm_always: true,
                    ..big
                },
                true
            )
            .bookmarks,
            Some(Scope::Document)
        );
        assert_eq!(Scope::parse("visible"), Scope::Visible);
        assert_eq!(Scope::parse("x"), Scope::Statement);
        // 북마크 범위는 자기 키(`bm_scope`) · 설명 범위는 `scope` — 서로 독립(사용자 10-13).
        let two = Cfg {
            bm_scope: Scope::Visible,
            ..c
        };
        assert_eq!(
            wanted(&two, true),
            Want {
                bookmarks: Some(Scope::Visible),
                descs: Some(Scope::Statement)
            }
        );
        assert_eq!(
            wanted(
                &Cfg {
                    bm_scope: Scope::Document,
                    partial: true,
                    ..c
                },
                true
            )
            .bookmarks,
            Some(Scope::Visible)
        );
    }

    /// 현재 문장 = 딸린 주석 줄까지(위 연속 주석 · 빈 줄 건너뜀 · 블록 주석 · 앞 문장의 꼬리 주석은 안 가져온다).
    #[test]
    fn statement_range_widens_to_attached_comment_lines() {
        let doc: Vec<&str> = vec![
            "SELECT 1 FROM dual; -- 앞 문장 꼬리", // 0
            "",                                    // 1
            "/* 블록",                             // 2
            "   주석 */",                          // 3
            "",                                    // 4
            "-- 자재 마스터",                      // 5
            "SELECT *",                            // 6
            "FROM t",                              // 7
            ";",                                   // 8
        ];
        let line_at = |n: usize| doc.get(n).map_or(String::new(), |s| (*s).to_string());
        assert_eq!(widen_comment_lines(&line_at, 6, 8), (2, 8));
        // 바로 위가 코드 줄이면 그대로.
        assert_eq!(widen_comment_lines(&line_at, 0, 0), (0, 0));
        // 한 줄짜리 블록 주석만 위에 있을 때.
        let doc2: Vec<&str> = vec!["x;", "/* a */", "SELECT 2;"];
        let line_at2 = |n: usize| doc2.get(n).map_or(String::new(), |s| (*s).to_string());
        assert_eq!(widen_comment_lines(&line_at2, 2, 2), (1, 2));
        // 코드 줄 끝의 블록 주석(`x; /* a */`)은 코드 줄 — 앞 문장이 딸려 오지 않는다(124차 리뷰).
        let doc3: Vec<&str> = vec!["x; /* a */", "SELECT 2;"];
        let line_at3 = |n: usize| doc3.get(n).map_or(String::new(), |s| (*s).to_string());
        assert_eq!(widen_comment_lines(&line_at3, 1, 1), (1, 1));
        // 여러 줄 블록 주석의 여는 줄이 코드 뒤에 붙은 꼴(`x; /* a` … `b */`) = 닫는 줄까지만.
        let doc4: Vec<&str> = vec!["x; /* a", " b */", "SELECT 3;"];
        let line_at4 = |n: usize| doc4.get(n).map_or(String::new(), |s| (*s).to_string());
        assert_eq!(widen_comment_lines(&line_at4, 2, 2), (1, 2));
        // 여는 줄이 `/*`로 시작하면 그 줄까지.
        let doc5: Vec<&str> = vec!["x;", "/* a", " b */", "SELECT 4;"];
        let line_at5 = |n: usize| doc5.get(n).map_or(String::new(), |s| (*s).to_string());
        assert_eq!(widen_comment_lines(&line_at5, 3, 3), (1, 3));
    }

    #[test]
    fn compose_joins_bookmark_then_descriptions_and_clips() {
        assert_eq!(compose(None, &[], 40), None);
        assert_eq!(
            compose(Some("\u{25c6} 급여"), &[], 40).as_deref(),
            Some("\u{25c6} 급여")
        );
        assert_eq!(
            compose(
                Some("\u{25c6} 급여"),
                &["사원 이름".into(), "  ".into(), "급여(원)".into()],
                40
            )
            .as_deref(),
            Some("\u{25c6} 급여 \u{b7} \u{bb} 사원 이름 \u{b7} \u{bb} 급여(원)")
        );
        assert_eq!(
            compose(None, &["설명".into()], 40).as_deref(),
            Some("\u{bb} 설명")
        );
        let long = compose(None, &["가나다라마바사아자차카타파하".into()], 8).expect("clip");
        assert_eq!(long.chars().count(), 8);
        assert!(long.ends_with('\u{2026}'));
        assert!(What::Both.takes(crate::app::objlink::LinkKind::Column));
        assert!(!What::Table.takes(crate::app::objlink::LinkKind::Column));
        assert!(!What::Both.takes(crate::app::objlink::LinkKind::Routine));
    }
}
