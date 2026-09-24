//! ★ 완성 **상세 카드**(사용자 09-24 · docs/76 §10 · 77 §1-3 `HoverCard`의 첫 구현): 팝업 옆에 같은 높이로 붙는 반투명 카드.
//! 구성은 대상(컬럼 · 테이블/뷰 · 함수 · 스키마 · 키워드)마다 채우는 내용만 다르고 틀은 하나 — 제목 · 부제 · 속성 행 · 절(목록).
//! 배경 = `intel.detail_bg_alpha`(투명도 % · 기본 80 % 투명) · 글자 = `intel.detail_text_alpha`(기본 50 %) · 제목은 조금 진하게.
//! 순수 모델(`build`)은 시험으로 · 그리기(`paint`)는 팝업 층에서 호스트가 부른다.

use crate::intel::IntelCfg;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::Rect;
use nexa_ctl::theme::Theme;
use nsql_core::Dialect;
use nsql_i18n::{t, Msg};
use nsql_run::meta::{ColState, DetailState, Interner, ObjId, Snapshot};
use nsql_script::builtins;
use nsql_script::intel::{Cand, CandKind};

/// 카드 대상.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Column(ObjId, u16),
    Object(ObjId),
    Function(String),
    Schema(String),
    Keyword(String),
    Word(String),
    /// (넣을 본문, 이름 — `모든 컬럼 (N)`).
    Snippet(String, String),
    None,
}

impl Target {
    pub(crate) fn of(c: &Cand, _dialect: Option<Dialect>) -> Target {
        let kind = c.tag >> 56;
        let id = ObjId(((c.tag >> 16) & 0xFFFF_FFFF) as u32);
        match (kind, c.kind) {
            (1, _) => Target::Column(id, (c.tag & 0xFFFF) as u16),
            (2, _) => Target::Object(id),
            (_, CandKind::Function) => Target::Function(c.text.clone()),
            (_, CandKind::Schema) => Target::Schema(c.text.clone()),
            (_, CandKind::Keyword) => Target::Keyword(c.text.clone()),
            (_, CandKind::Snippet) => Target::Snippet(c.text.clone(), c.detail.clone()),
            (_, CandKind::Word) => Target::Word(c.text.clone()),
            _ => Target::None,
        }
    }

    /// 메타를 더 읽어야 하는 객체 id(호스트가 탐색기에 요청).
    pub(crate) fn needs(&self) -> Option<ObjId> {
        match self {
            Target::Column(id, _) | Target::Object(id) => Some(*id),
            _ => None,
        }
    }
}

/// 카드 모델(그리기와 분리 · 시험 대상).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Card {
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<(String, String)>,
    pub sections: Vec<(String, Vec<String>)>,
    pub loading: bool,
}

/// 대상 → 카드(메타 스냅샷에서 · 없는 것은 "읽는 중").
pub(crate) fn build(
    target: &Target,
    names: &Interner,
    snap: &Snapshot,
    dialect: Option<Dialect>,
) -> Option<Card> {
    let mut card = Card::default();
    match target {
        Target::None => return None,
        Target::Column(id, pos) => {
            let o = snap.object(*id)?;
            let table = names.get(o.name).to_string();
            let schema = names.get(o.schema).to_string();
            card.rows
                .push((t(Msg::CardTable).into(), format!("{schema}.{table}")));
            match snap.columns(*id) {
                ColState::Loaded { cols, .. } => {
                    let c = cols.iter().find(|c| c.position == *pos)?;
                    let cname = names.get(c.name).to_string();
                    card.title = cname.clone();
                    card.subtitle = names.get(c.data_type).to_string();
                    card.rows.push((t(Msg::CardColumn).into(), cname.clone()));
                    card.rows
                        .push((t(Msg::CardSeq).into(), c.position.to_string()));
                    card.rows
                        .push((t(Msg::CardType).into(), names.get(c.data_type).to_string()));
                    // 설명 행은 값이 없어도 둔다(사용자 09-24): 읽는 중 `…` · 없음 `-`.
                    let desc = match snap.detail(*id) {
                        DetailState::Loaded { detail, .. } => detail
                            .col_comments
                            .iter()
                            .find(|(n, _)| names.get(*n).eq_ignore_ascii_case(&cname))
                            .map_or_else(|| "-".to_string(), |(_, c)| names.get(*c).to_string()),
                        _ => "…".to_string(),
                    };
                    card.rows.push((t(Msg::CardDescription).into(), desc));
                    card.rows.push((
                        t(Msg::CardNotNull).into(),
                        match c.nullable {
                            Some(false) => "true".into(),
                            Some(true) => "false".into(),
                            None => "-".into(),
                        },
                    ));
                    if let Some(d) = c.default {
                        card.rows
                            .push((t(Msg::CardDefault).into(), names.get(d).to_string()));
                    }
                    match snap.detail(*id) {
                        DetailState::Loaded { detail, .. } => {
                            // 이 컬럼이 그 키/인덱스의 몇 번째인지(`#i/n` · 사용자 09-24).
                            let pos_of = |cols: &[nsql_run::meta::Sym]| -> Option<String> {
                                cols.iter()
                                    .position(|s| names.get(*s).eq_ignore_ascii_case(&cname))
                                    .map(|i| format!("#{}/{}", i + 1, cols.len()))
                            };
                            let mut keys: Vec<String> = Vec::new();
                            let mut cons: Vec<String> = Vec::new();
                            for k in &detail.keys {
                                let Some(at) = pos_of(&k.cols) else {
                                    continue;
                                };
                                let n = names.get(k.name).to_string();
                                match k.kind {
                                    'P' => keys.push(format!("PK  {n}  {at}")),
                                    'U' => keys.push(format!("UQ  {n}  {at}")),
                                    'R' => cons.push(format!(
                                        "FK  {n}  {at} → {}",
                                        k.ref_table
                                            .map(|r| names.get(r).to_string())
                                            .unwrap_or_default()
                                    )),
                                    _ => cons.push(format!("CK  {n}")),
                                }
                            }
                            let idx: Vec<String> = detail
                                .indexes
                                .iter()
                                .filter_map(|i| {
                                    pos_of(&i.cols).map(|at| {
                                        format!(
                                            "{}  {at}{}",
                                            names.get(i.name),
                                            if i.unique { "  (unique)" } else { "" }
                                        )
                                    })
                                })
                                .collect();
                            if !keys.is_empty() {
                                card.sections.push((t(Msg::CardKeys).into(), keys));
                            }
                            if !idx.is_empty() {
                                card.sections.push((t(Msg::CardIndexes).into(), idx));
                            }
                            if !cons.is_empty() {
                                card.sections.push((t(Msg::CardConstraints).into(), cons));
                            }
                        }
                        _ => card.loading = true,
                    }
                }
                _ => {
                    card.title = format!("#{pos}");
                    card.loading = true;
                }
            }
        }
        Target::Object(id) => {
            let o = snap.object(*id)?;
            card.title = names.get(o.name).to_string();
            card.subtitle = format!("{:?}", o.kind).to_lowercase();
            card.rows
                .push((t(Msg::CardSchema).into(), names.get(o.schema).to_string()));
            card.rows
                .push((t(Msg::CardKind).into(), card.subtitle.clone()));
            if o.kind.is_relation() {
                let desc = match snap.detail(*id) {
                    DetailState::Loaded { detail, .. } => detail
                        .comment
                        .map_or_else(|| "-".to_string(), |c| names.get(c).to_string()),
                    _ => "…".to_string(),
                };
                card.rows.push((t(Msg::CardDescription).into(), desc));
            }
            let status = match o.status {
                nsql_run::meta::ObjStatus::Valid => "VALID",
                nsql_run::meta::ObjStatus::Invalid => "INVALID",
                nsql_run::meta::ObjStatus::Unknown => "",
            };
            if !status.is_empty() {
                card.rows.push((t(Msg::CardStatus).into(), status.into()));
            }
            if let Some(e) = o.extra {
                card.rows
                    .push((t(Msg::CardExtra).into(), names.get(e).to_string()));
            }
            if o.kind.is_relation() {
                match snap.columns(*id) {
                    ColState::Loaded { cols, .. } => {
                        card.rows
                            .push((t(Msg::CardColumns).into(), cols.len().to_string()));
                        let pk: Vec<String> = cols
                            .iter()
                            .filter(|c| c.key & nsql_run::meta::KEY_PK != 0)
                            .map(|c| names.get(c.name).to_string())
                            .collect();
                        if !pk.is_empty() {
                            card.sections.push((t(Msg::CardKeys).into(), pk));
                        }
                        let mut list: Vec<String> = cols
                            .iter()
                            .take(12)
                            .map(|c| format!("{} : {}", names.get(c.name), names.get(c.data_type)))
                            .collect();
                        if cols.len() > 12 {
                            list.push(format!("… +{}", cols.len() - 12));
                        }
                        card.sections.push((t(Msg::CardColumns).into(), list));
                    }
                    _ => card.loading = true,
                }
                if let DetailState::Loaded { detail, .. } = snap.detail(*id) {
                    let idx: Vec<String> = detail
                        .indexes
                        .iter()
                        .map(|i| names.get(i.name).to_string())
                        .collect();
                    if !idx.is_empty() {
                        card.sections.push((t(Msg::CardIndexes).into(), idx));
                    }
                }
            }
        }
        Target::Function(name) => {
            card.title = name.clone();
            card.subtitle = t(Msg::CardBuiltin).into();
            if let Some(sig) = builtins::signature(dialect, name) {
                card.rows
                    .push((t(Msg::CardSignature).into(), sig.to_string()));
            }
        }
        Target::Schema(name) => {
            card.title = name.clone();
            card.subtitle = t(Msg::CardSchema).into();
            // 읽어 둔 버킷의 종류별 수(테이블·뷰·…) · 아직 안 읽었으면 "…"(`스키마.`를 치면 그때 읽는다 · 09-24 빈 카드 지적).
            if let Some(sym) = names.find(name) {
                let mut kinds: Vec<(String, usize)> = snap
                    .buckets
                    .values()
                    .filter(|b| b.schema == sym && !b.objs.is_empty())
                    .map(|b| (format!("{:?}", b.kind).to_lowercase(), b.objs.len()))
                    .collect();
                kinds.sort();
                let total: usize = kinds.iter().map(|(_, n)| n).sum();
                card.rows.push((
                    t(Msg::CardObjects).into(),
                    if total > 0 {
                        total.to_string()
                    } else {
                        "…".to_string()
                    },
                ));
                if !kinds.is_empty() {
                    card.sections.push((
                        t(Msg::CardObjects).into(),
                        kinds
                            .into_iter()
                            .map(|(k, n)| format!("{k}  {n}"))
                            .collect(),
                    ));
                }
            }
        }
        Target::Keyword(k) => {
            card.title = k.clone();
            card.subtitle = t(Msg::CardKeyword).into();
        }
        Target::Word(w) => {
            card.title = w.clone();
            card.subtitle = t(Msg::CardDocWord).into();
        }
        Target::Snippet(s, label) => {
            // "모든 컬럼 (N)" 조각(09-24): 제목 = 이름 · 컬럼 수 · 목록(쉼표 기준 · 줄바꿈 무관 · 40개 넘으면 … +N).
            card.title = label.clone();
            card.subtitle = t(Msg::CardSnippet).into();
            let names: Vec<String> = s
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect();
            card.rows
                .push((t(Msg::CardColumns).into(), names.len().to_string()));
            let mut list: Vec<String> = names.iter().take(40).cloned().collect();
            if names.len() > 40 {
                list.push(format!("… (+{})", names.len() - 40));
            }
            card.sections.push((t(Msg::CardColumns).into(), list));
        }
    }
    Some(card)
}

/// 카드 그리기 — 팝업(`menu`) 오른쪽에 같은 높이 · 오른쪽에 자리가 없으면 왼쪽 · 표면 밖이면 안 그림.
pub(crate) fn paint(
    dc: &mut dyn DrawCtx,
    th: &Theme,
    menu: Rect,
    card: &Card,
    cfg: &IntelCfg,
    scale: f32,
    surface: (i32, i32),
) {
    let s = |v: f32| (v * scale).round() as i32;
    let w = s(300.0);
    let gap = s(4.0);
    let x = if menu.right() + gap + w <= surface.0 {
        menu.right() + gap
    } else if menu.x - gap - w >= 0 {
        menu.x - gap - w
    } else {
        return;
    };
    let r = Rect::new(x, menu.y, w, menu.h);
    let radius = s(6.0);
    let bg_a = 1.0 - f32::from(cfg.detail_bg_alpha) / 100.0;
    let text_t = f32::from(cfg.detail_text_alpha) / 100.0;
    // 배경(투명) + 테두리(같은 투명도) — 팝업과 같은 모양.
    dc.fill_round_rect_alpha(r, radius, th.panel_bg, bg_a);
    dc.stroke_round_rect(r, radius, th.border.lerp(th.window_bg, 1.0 - bg_a), 1.0);
    let fg = th.text.lerp(th.window_bg, text_t);
    let dim = th.text_dim.lerp(th.window_bg, text_t);
    let title = th.accent.lerp(th.window_bg, (text_t * 0.6).min(0.5));
    let pad = s(10.0);
    let clip = Rect::new(r.x + 1, r.y + 1, r.w - 2, r.h - 2);
    let mut y = r.y + s(8.0);
    // 제목(굵게) + 부제(흐리게 · 오른쪽).
    dc.select_font(FontSlot::Base, true);
    let lh = dc.text_height();
    dc.text(r.x + pad, y, clip, &card.title, title);
    dc.select_font(FontSlot::Base, false);
    if !card.subtitle.is_empty() {
        let tw = dc.text_width(&card.subtitle);
        dc.text(r.right() - pad - tw, y, clip, &card.subtitle, dim);
    }
    y += lh + s(6.0);
    dc.fill_rect_alpha(
        Rect::new(r.x + pad, y, r.w - pad * 2, 1),
        th.border,
        (1.0 - text_t).max(0.2),
    );
    y += s(6.0);
    // 속성 행: 라벨 열(흐림) + 값.
    let label_w = card
        .rows
        .iter()
        .map(|(l, _)| dc.text_width(l))
        .max()
        .unwrap_or(0)
        + s(12.0);
    let row_h = lh + s(3.0);
    for (l, v) in &card.rows {
        if y + row_h > r.bottom() - s(4.0) {
            return;
        }
        dc.text(r.x + pad, y, clip, l, dim);
        let vclip = Rect::new(r.x + pad + label_w, y, r.w - pad * 2 - label_w, row_h);
        let shown = nexa_ctl::draw::ellipsize_middle(dc, v, vclip.w);
        dc.text(vclip.x, y, clip, &shown, fg);
        y += row_h;
    }
    // 절: 머리글(흐림 · 위 여백) + 항목(들여쓰기).
    for (h, items) in &card.sections {
        y += s(4.0);
        if !h.is_empty() {
            if y + row_h > r.bottom() - s(4.0) {
                return;
            }
            dc.text(r.x + pad, y, clip, h, dim);
            y += row_h;
        }
        for it in items {
            if y + row_h > r.bottom() - s(4.0) {
                return;
            }
            let shown = nexa_ctl::draw::ellipsize_middle(dc, it, r.w - pad * 2 - s(12.0));
            dc.text(r.x + pad + s(12.0), y, clip, &shown, fg);
            y += row_h;
        }
    }
    if card.loading && y + row_h <= r.bottom() - s(4.0) {
        y += s(4.0);
        dc.text(r.x + pad, y, clip, t(Msg::StIntelLoading), dim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_run::meta::{MetaStore, NewCol, NewDetail, NewObj};

    /// 컬럼 카드 = 테이블·컬럼·순번·타입·Not Null + 이 컬럼이 든 PK/인덱스/제약만 · 상세 전 = 읽는 중 · 테이블 카드 = 컬럼 수·PK.
    #[test]
    fn column_and_table_cards_from_snapshot() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["BISCM".into()], Some("BISCM"));
        m.load_bucket(
            "BISCM",
            nsql_catalog::ObjectKind::Table,
            &[NewObj {
                name: "M4S_I002040".into(),
                ..Default::default()
            }],
            1,
        );
        let id = m
            .snapshot()
            .lookup(&m.names, Some("BISCM"), "M4S_I002040")
            .expect("card");
        m.set_columns(
            id,
            &[
                NewCol {
                    name: "ITEM_CD".into(),
                    data_type: "VARCHAR2(20)".into(),
                    nullable: Some(false),
                    position: 1,
                    key: nsql_run::meta::KEY_PK,
                    ..Default::default()
                },
                NewCol {
                    name: "TOTAL_WEIGHT".into(),
                    data_type: "NUMBER(21,6)".into(),
                    nullable: Some(true),
                    position: 54,
                    ..Default::default()
                },
            ],
            1,
        );
        let snap = m.snapshot();
        let c = build(
            &Target::Column(id, 54),
            &m.names,
            &snap,
            Some(Dialect::Oracle),
        )
        .expect("card");
        assert_eq!(c.title, "TOTAL_WEIGHT");
        assert!(c.loading, "상세 전 = 읽는 중");
        assert!(c
            .rows
            .iter()
            .any(|(l, v)| l == t(Msg::CardSeq) && v == "54"));
        assert!(c
            .rows
            .iter()
            .any(|(l, v)| l == t(Msg::CardNotNull) && v == "false"));
        m.set_detail(
            id,
            &NewDetail {
                keys: vec![
                    ("PK_M4S".into(), 'P', vec!["ITEM_CD".into()], None),
                    ("CK_W".into(), 'C', vec!["TOTAL_WEIGHT".into()], None),
                ],
                indexes: vec![
                    ("IX_W".into(), false, vec!["TOTAL_WEIGHT".into()]),
                    ("PK_M4S".into(), true, vec!["ITEM_CD".into()]),
                ],
                comment: Some("품목 마스터".into()),
                col_comments: vec![("TOTAL_WEIGHT".into(), "총 중량(kg)".into())],
            },
            2,
        );
        let snap = m.snapshot();
        let c = build(
            &Target::Column(id, 54),
            &m.names,
            &snap,
            Some(Dialect::Oracle),
        )
        .expect("card");
        assert!(!c.loading);
        assert!(
            c.rows
                .iter()
                .any(|(l, v)| l == t(Msg::CardDescription) && v == "총 중량(kg)"),
            "컬럼 코멘트: {:?}",
            c.rows
        );
        let names: Vec<&str> = c.sections.iter().map(|(h, _)| h.as_str()).collect();
        assert!(
            names.contains(&t(Msg::CardIndexes)) && names.contains(&t(Msg::CardConstraints)),
            "{names:?}"
        );
        assert!(
            !names.contains(&t(Msg::CardKeys)),
            "PK는 ITEM_CD에만: {names:?}"
        );
        assert!(c.sections.iter().any(|(_, v)| v
            .iter()
            .any(|s| s.starts_with("IX_W") && s.contains("#1/1"))));
        let c = build(
            &Target::Column(id, 1),
            &m.names,
            &snap,
            Some(Dialect::Oracle),
        )
        .expect("card");
        assert!(c.sections.iter().any(|(h, v)| h == t(Msg::CardKeys)
            && v[0].contains("PK_M4S")
            && v[0].contains("#1/1")));
        let tcard =
            build(&Target::Object(id), &m.names, &snap, Some(Dialect::Oracle)).expect("card");
        assert_eq!(tcard.title, "M4S_I002040");
        assert!(tcard
            .rows
            .iter()
            .any(|(l, v)| l == t(Msg::CardColumns) && v == "2"));
        assert!(tcard
            .rows
            .iter()
            .any(|(l, v)| l == t(Msg::CardDescription) && v == "품목 마스터"));
        assert!(
            build(&Target::Keyword("SELECT".into()), &m.names, &snap, None)
                .expect("card")
                .subtitle
                == t(Msg::CardKeyword)
        );
        assert!(build(&Target::None, &m.names, &snap, None).is_none());
    }
}
