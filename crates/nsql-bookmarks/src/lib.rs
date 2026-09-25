//! 북마크 코어([docs/69](../../../docs/69-bookmarks.md) · T-167 B1) — **문서 안의 자리**만 북마크라 부른다(69 §2-1).
//!
//! - 모델: [`DocKey`](파일 · 이름 없는 탭 · 객체 DDL) · [`Anchor`](줄 + 원문 + 앞뒤 문맥 + 문서 해시) · [`Bookmark`] · [`Group`] · [`Store`].
//! - **L0 줄 보정**([`Store::apply_change`]) — 편집 버퍼의 줄 변경 기록(`first, removed, inserted`)을 그대로 소비한다(69 §3-1).
//! - **L1 재탐색**([`relocate`]) — 해시 같음 → 정합 표(있으면) → 줄 원문 → 양방향 유사도(Sørensen–Dice 바이그램) → 유일 줄 → 무효(69 §3-2).
//!   순수 함수: 본문은 `&[&str]`로 받고 정합 표는 `Option<&[Option<usize>]>`로 받는다(nexa-ctl을 부르지 않는다).
//! - JSON: [`Store::to_json`] / [`Store::from_json`](69 §7-2 형식 · 모르는 키는 무시 · 깨진 항목은 그 항목만 버림).
//! - 정리·상한: [`Store::prune`](무효 N일) · `max_per_doc`/`max_total`(69 §5).
//!
//! UI·CLI는 이 crate의 순수 함수에 본문을 넣어 부른다 — 여기서는 파일을 열지도, 시계를 읽지도 않는다(시각은 인자).

use nsql_settings::json::{parse, Json};
use std::collections::HashMap;

/// 문서 열쇠(69 §2-2) — 파일 = 정규화 경로 · 이름 없는 탭 = 워크스페이스 탭 id · 객체 DDL = 접속 좌표 + 객체(공용 저장에는 프로필만).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DocKey {
    File {
        path: String,
    },
    Scratch {
        tab: u64,
    },
    Object {
        profile: String,
        dialect: String,
        schema: String,
        kind: String,
        name: String,
        part: String,
    },
}

impl DocKey {
    /// 파일 경로 정규화 — 역슬래시를 슬래시로 · Windows는 비교만 대소문자 무시(저장은 원문 그대로).
    #[must_use]
    pub fn file(path: &str) -> Self {
        DocKey::File {
            path: path.replace('\\', "/"),
        }
    }

    /// 표시용 짧은 이름(파일 이름 · `Script_7` · `SCHEMA.NAME`).
    /// 이름 없는 탭(`Scratch`)은 **탭 id**로만 묶여 있고 지금 이름은 호스트(GUI)가 id로 찾아 보여 준다 — 여기의 `Script_{id}`는
    /// 탭을 모르는 곳(CLI · 닫힌 탭)의 대체 표기일 뿐 만들 때의 탭 이름이 아니다(사용자 09-23).
    #[must_use]
    pub fn short_name(&self) -> String {
        match self {
            DocKey::File { path } => path.rsplit('/').next().unwrap_or(path).to_string(),
            DocKey::Scratch { tab } => format!("Script_{tab}"),
            DocKey::Object {
                schema, name, part, ..
            } => {
                if part.is_empty() || part == "source" {
                    format!("{schema}.{name}")
                } else {
                    format!("{schema}.{name} ({part})")
                }
            }
        }
    }

    /// 같은 문서인가 — 파일은 대소문자 무시 비교(`case_insensitive` = Windows/macOS).
    #[must_use]
    pub fn same(&self, other: &DocKey, case_insensitive: bool) -> bool {
        match (self, other) {
            (DocKey::File { path: a }, DocKey::File { path: b }) if case_insensitive => {
                a.eq_ignore_ascii_case(b)
            }
            _ => self == other,
        }
    }
}

/// 위치 앵커(69 §2-3) — `line`은 0-기준.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Anchor {
    pub line: u32,
    pub col: u32,
    pub text: String,
    pub before: String,
    pub after: String,
    pub doc_hash: u64,
    pub doc_lines: u32,
}

/// 무효 사유(69 §2-3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    FileMissing,
    ObjectMissing,
    TextGone,
    ServerUnknown,
    TooBig,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Reason::FileMissing => "file_missing",
            Reason::ObjectMissing => "object_missing",
            Reason::TextGone => "text_gone",
            Reason::ServerUnknown => "server_unknown",
            Reason::TooBig => "too_big",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "file_missing" => Reason::FileMissing,
            "object_missing" => Reason::ObjectMissing,
            "text_gone" => Reason::TextGone,
            "server_unknown" => Reason::ServerUnknown,
            "too_big" => Reason::TooBig,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Live,
    Invalid { since: u64, reason: Reason },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bookmark {
    pub id: u64,
    pub doc: DocKey,
    pub anchor: Anchor,
    pub label: Option<String>,
    pub note: Option<String>,
    pub mnemonic: Option<u8>,
    pub group: u32,
    pub shared: bool,
    pub state: State,
    pub created: u64,
    pub visited: u64,
    /// 재탐색이 줄 삭제로 경계에 붙였다(정확한 자리가 아니다 · 69 §3-1).
    pub shifted: bool,
}

impl Bookmark {
    #[must_use]
    pub fn is_live(&self) -> bool {
        matches!(self.state, State::Live)
    }
    /// 목록 표시 글 — 이름이 있으면 이름 · 없으면 줄 원문(빈 줄이면 `(empty)` · 69 C-24).
    #[must_use]
    pub fn display(&self) -> String {
        if let Some(l) = self.label.as_deref().filter(|l| !l.trim().is_empty()) {
            return l.to_string();
        }
        let t = self.anchor.text.trim();
        if t.is_empty() {
            "(empty)".into()
        } else {
            t.to_string()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: u32,
    pub name: String,
    pub default: bool,
    pub enabled: bool,
}

/// 재탐색 옵션(설정 `bookmark.*` · 69 §9-3).
#[derive(Clone, Debug)]
pub struct RelocateOpts {
    /// 양방향 탐색 범위(줄).
    pub search_lines: usize,
    /// 유사도 문턱(0 = 정확 일치만).
    pub similarity: f32,
    /// 마지막 수단 = 문서에서 유일한 줄.
    pub relocate_unique: bool,
    /// 이보다 큰 문서는 재탐색을 건너뛴다(줄 번호만 · `TooBig`).
    pub max_lines: usize,
    /// 좌우 공백을 접고 비교.
    pub trim: bool,
    /// 앞뒤 줄 문맥을 앵커에 저장.
    pub context: bool,
    /// 앵커 한 줄 저장 길이.
    pub anchor_chars: usize,
}

impl Default for RelocateOpts {
    fn default() -> Self {
        RelocateOpts {
            search_lines: 400,
            similarity: 0.6,
            relocate_unique: true,
            max_lines: 200_000,
            trim: true,
            context: true,
            anchor_chars: 200,
        }
    }
}

/// 재탐색 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relocated {
    /// 그대로(해시 같음 · 정확 일치).
    Same,
    /// 다른 줄로 옮김(정합·유사도·유일).
    Moved(usize),
    /// 큰 문서라 건너뜀(줄 번호 그대로 · `TooBig` 표식).
    TooBig,
    /// 못 찾음(무효).
    Gone,
}

/// 문서 전체 해시(FNV-1a 64 · 줄 단위 · `\r` 무시).
#[must_use]
pub fn doc_hash(lines: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for l in lines {
        for b in l.trim_end_matches('\r').bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^= 0x0a;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn cut(s: &str, max: usize) -> String {
    s.trim_end_matches('\r').chars().take(max).collect()
}

/// 본문에서 앵커를 만든다(줄 원문 + 앞뒤 문맥 + 해시).
#[must_use]
pub fn make_anchor(lines: &[&str], line: usize, col: u32, opts: &RelocateOpts) -> Anchor {
    let get = |i: Option<usize>| -> String {
        i.and_then(|i| lines.get(i))
            .map(|l| cut(l, opts.anchor_chars))
            .unwrap_or_default()
    };
    Anchor {
        line: line as u32,
        col,
        text: get(Some(line)),
        before: if opts.context {
            get(line.checked_sub(1))
        } else {
            String::new()
        },
        after: if opts.context {
            get(Some(line + 1))
        } else {
            String::new()
        },
        doc_hash: doc_hash(lines),
        doc_lines: lines.len() as u32,
    }
}

/// Sørensen–Dice 바이그램 유사도(0.0~1.0 · 짧은 문자열은 글자 단위).
#[must_use]
pub fn dice(a: &str, b: &str) -> f32 {
    let ga: Vec<(char, char)> = bigrams(a);
    let gb: Vec<(char, char)> = bigrams(b);
    if ga.is_empty() && gb.is_empty() {
        return if a == b { 1.0 } else { 0.0 };
    }
    if ga.is_empty() || gb.is_empty() {
        return 0.0;
    }
    let mut counts: HashMap<(char, char), i32> = HashMap::new();
    for g in &ga {
        *counts.entry(*g).or_insert(0) += 1;
    }
    let mut inter = 0;
    for g in &gb {
        if let Some(c) = counts.get_mut(g) {
            if *c > 0 {
                *c -= 1;
                inter += 1;
            }
        }
    }
    (2.0 * inter as f32) / (ga.len() + gb.len()) as f32
}

fn bigrams(s: &str) -> Vec<(char, char)> {
    let cs: Vec<char> = s.chars().collect();
    if cs.len() < 2 {
        return cs.first().map(|&c| vec![(c, '\0')]).unwrap_or_default();
    }
    cs.windows(2).map(|w| (w[0], w[1])).collect()
}

fn norm(s: &str, trim: bool) -> &str {
    let s = s.trim_end_matches('\r');
    if trim {
        s.trim()
    } else {
        s
    }
}

/// ★ L1 재탐색(69 §3-2) — 옛 앵커와 새 본문(줄 배열)으로 자리를 다시 찾는다. `map` = 옛 줄 → 새 줄 정합 표(있으면).
/// 앵커는 갱신하지 않는다(호출자가 확정 뒤 [`make_anchor`]로 새로 만든다).
#[must_use]
pub fn relocate(
    anchor: &Anchor,
    lines: &[&str],
    map: Option<&[Option<usize>]>,
    opts: &RelocateOpts,
) -> Relocated {
    let n = lines.len();
    if n == 0 {
        return Relocated::Gone;
    }
    if opts.max_lines > 0 && n > opts.max_lines {
        return Relocated::TooBig;
    }
    // 1. 해시 같음 = 그대로.
    if anchor.doc_hash != 0 && anchor.doc_hash == doc_hash(lines) && (anchor.line as usize) < n {
        return Relocated::Same;
    }
    let old = anchor.line as usize;
    let want = norm(&anchor.text, opts.trim);
    let eq = |i: usize| -> bool { i < n && norm(lines[i], opts.trim) == want };
    // 2. 정합 표.
    let mut cand = old.min(n.saturating_sub(1));
    if let Some(m) = map {
        if let Some(Some(j)) = m.get(old) {
            cand = (*j).min(n - 1);
        } else {
            // 삭제된 줄 → 근처의 정합 줄(위쪽부터).
            let near = (1..=m.len()).find_map(|d| {
                let up = old.checked_sub(d).and_then(|i| m.get(i).copied().flatten());
                let down = m.get(old + d).copied().flatten();
                up.or(down)
            });
            if let Some(j) = near {
                cand = j.min(n - 1);
            }
        }
    }
    // 빈 앵커(공백뿐)는 어디에나 있어 원문 탐색을 건너뛴다 — 줄 번호만.
    if want.is_empty() {
        return if cand == old {
            Relocated::Same
        } else {
            Relocated::Moved(cand)
        };
    }
    // 3. 후보 줄 원문 일치.
    if eq(cand) {
        return if cand == old {
            Relocated::Same
        } else {
            Relocated::Moved(cand)
        };
    }
    // 4. 양방향 탐색(정확 일치 우선 · 아니면 유사도 최대 · 같으면 가까운 쪽).
    let before = norm(&anchor.before, opts.trim);
    let after = norm(&anchor.after, opts.trim);
    let score = |i: usize| -> f32 {
        let t = dice(norm(lines[i], opts.trim), want) * 2.0;
        let b = if before.is_empty() {
            0.0
        } else {
            i.checked_sub(1)
                .map(|k| dice(norm(lines[k], opts.trim), before))
                .unwrap_or(0.0)
        };
        let a = if after.is_empty() {
            0.0
        } else {
            lines
                .get(i + 1)
                .map(|l| dice(norm(l, opts.trim), after))
                .unwrap_or(0.0)
        };
        t + b + a
    };
    let ctx_n = usize::from(!before.is_empty()) + usize::from(!after.is_empty());
    let threshold = opts.similarity * (2.0 + ctx_n as f32);
    // 후보 줄 자체(d = 0)도 유사도로 본다 — 그 줄을 고친 경우가 가장 흔하다.
    let mut best: Option<(f32, usize)> = None;
    if opts.similarity > 0.0 {
        let s0 = score(cand);
        if s0 >= threshold {
            best = Some((s0, cand));
        }
    }
    for d in 1..=opts.search_lines.max(1) {
        let mut any = false;
        for i in [cand.checked_sub(d), Some(cand + d)].into_iter().flatten() {
            if i >= n {
                continue;
            }
            any = true;
            if eq(i) {
                return Relocated::Moved(i);
            }
            if opts.similarity > 0.0 {
                let s = score(i);
                if s >= threshold && best.is_none_or(|(bs, _)| s > bs) {
                    best = Some((s, i));
                }
            }
        }
        if !any {
            break;
        }
    }
    if let Some((_, i)) = best {
        return Relocated::Moved(i);
    }
    // 5. 문서에서 유일한 줄.
    if opts.relocate_unique {
        let hits: Vec<usize> = (0..n).filter(|&i| eq(i)).collect();
        if hits.len() == 1 {
            return Relocated::Moved(hits[0]);
        }
    }
    Relocated::Gone
}

/// L0 줄 보정 한 건(69 §3-1) — `line`이 어디로 가는가 · 삭제 구간 안이면 경계로(`shifted`).
#[must_use]
pub fn shift_line(line: usize, first: usize, removed: usize, inserted: usize) -> (usize, bool) {
    if line < first {
        (line, false)
    } else if line >= first + removed {
        ((line + inserted).saturating_sub(removed), false)
    } else if inserted > 0 {
        (first + (line - first).min(inserted - 1), false)
    } else {
        (first, true)
    }
}

/// 정리 판정(69 §5 · MC/DC 대상): 무효이고 · `stale_days > 0`이고 · 경과가 그 이상이면 버린다.
#[must_use]
pub fn should_prune(state: State, now: u64, stale_days: u32) -> bool {
    match state {
        State::Invalid { since, .. } => {
            stale_days > 0 && now.saturating_sub(since) >= u64::from(stale_days) * 86_400
        }
        State::Live => false,
    }
}

/// 저장소 — 항목·그룹·다음 id. 파일 읽기/쓰기는 호출자(`to_json`/`from_json`만).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Store {
    pub items: Vec<Bookmark>,
    pub groups: Vec<Group>,
    pub next_id: u64,
    pub next_group: u32,
}

pub const DEFAULT_GROUP: u32 = 1;

impl Store {
    /// 진단: 저장소에 있는 문서 열쇠와 각 건수(`bm.stat` · 09-25).
    #[must_use]
    pub fn doc_keys_debug(&self) -> String {
        let mut m: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for b in &self.items {
            *m.entry(format!("{:?}", b.doc)).or_insert(0) += 1;
        }
        m.into_iter()
            .map(|(k, n)| format!("{k}×{n}"))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    #[must_use]
    pub fn new() -> Self {
        Store {
            items: Vec::new(),
            groups: vec![Group {
                id: DEFAULT_GROUP,
                name: String::new(),
                default: true,
                enabled: true,
            }],
            next_id: 1,
            next_group: 2,
        }
    }

    fn ensure_default_group(&mut self) {
        if !self.groups.iter().any(|g| g.default) {
            if let Some(g) = self.groups.first_mut() {
                g.default = true;
            } else {
                self.groups.push(Group {
                    id: DEFAULT_GROUP,
                    name: String::new(),
                    default: true,
                    enabled: true,
                });
            }
        }
    }

    #[must_use]
    pub fn default_group(&self) -> u32 {
        self.groups
            .iter()
            .find(|g| g.default)
            .map_or(DEFAULT_GROUP, |g| g.id)
    }

    #[must_use]
    pub fn group_enabled(&self, id: u32) -> bool {
        self.groups
            .iter()
            .find(|g| g.id == id)
            .is_none_or(|g| g.enabled)
    }

    pub fn add_group(&mut self, name: &str) -> u32 {
        let id = self.next_group;
        self.next_group += 1;
        self.groups.push(Group {
            id,
            name: name.to_string(),
            default: false,
            enabled: true,
        });
        id
    }

    /// 문서의 항목(줄 순).
    #[must_use]
    pub fn for_doc(&self, doc: &DocKey, ci: bool) -> Vec<&Bookmark> {
        let mut v: Vec<&Bookmark> = self.items.iter().filter(|b| b.doc.same(doc, ci)).collect();
        v.sort_by_key(|b| b.anchor.line);
        v
    }

    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Bookmark> {
        self.items.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Bookmark> {
        self.items.iter_mut().find(|b| b.id == id)
    }

    #[must_use]
    pub fn at_line(&self, doc: &DocKey, line: usize, ci: bool) -> Option<&Bookmark> {
        self.items
            .iter()
            .find(|b| b.doc.same(doc, ci) && b.is_live() && b.anchor.line as usize == line)
    }

    /// 더하기 — 같은 줄에 이미 있으면 그것(C-1) · 상한(`max_per_doc`/`max_total` · 0 = 없음)을 넘으면 `Err`.
    pub fn add(
        &mut self,
        doc: DocKey,
        anchor: Anchor,
        now: u64,
        ci: bool,
        max_per_doc: usize,
        max_total: usize,
    ) -> Result<u64, &'static str> {
        if let Some(b) = self.at_line(&doc, anchor.line as usize, ci) {
            return Ok(b.id);
        }
        if max_total > 0 && self.items.len() >= max_total {
            self.drop_one_invalid();
            if self.items.len() >= max_total {
                return Err("max_total");
            }
        }
        if max_per_doc > 0 && self.for_doc(&doc, ci).len() >= max_per_doc {
            return Err("max_per_doc");
        }
        let id = self.next_id;
        self.next_id += 1;
        let group = self.default_group();
        self.items.push(Bookmark {
            id,
            doc,
            anchor,
            label: None,
            note: None,
            mnemonic: None,
            group,
            shared: false,
            state: State::Live,
            created: now,
            visited: now,
            shifted: false,
        });
        Ok(id)
    }

    fn drop_one_invalid(&mut self) {
        if let Some(i) = self.items.iter().position(|b| !b.is_live()) {
            self.items.remove(i);
        }
    }

    pub fn remove(&mut self, id: u64) -> Option<Bookmark> {
        let i = self.items.iter().position(|b| b.id == id)?;
        Some(self.items.remove(i))
    }

    /// 토글 — 그 줄에 있으면 지우고(`Ok(None)`) · 없으면 더한다(`Ok(Some(id))`).
    pub fn toggle(
        &mut self,
        doc: DocKey,
        anchor: Anchor,
        now: u64,
        ci: bool,
        max_per_doc: usize,
        max_total: usize,
    ) -> Result<Option<u64>, &'static str> {
        if let Some(b) = self.at_line(&doc, anchor.line as usize, ci) {
            let id = b.id;
            self.remove(id);
            return Ok(None);
        }
        self.add(doc, anchor, now, ci, max_per_doc, max_total)
            .map(Some)
    }

    /// 문서의 항목 전부 제거 — 지운 것들.
    pub fn clear_doc(&mut self, doc: &DocKey, ci: bool) -> Vec<Bookmark> {
        let (gone, keep): (Vec<Bookmark>, Vec<Bookmark>) =
            self.items.drain(..).partition(|b| b.doc.same(doc, ci));
        self.items = keep;
        gone
    }

    /// 다음/이전(문서 안 · 켜진 그룹 · 살아 있는 것 · 순환) — `from` 줄 기준.
    #[must_use]
    pub fn next_in_doc(
        &self,
        doc: &DocKey,
        from: usize,
        forward: bool,
        ci: bool,
    ) -> Option<&Bookmark> {
        let mut v: Vec<&Bookmark> = self
            .for_doc(doc, ci)
            .into_iter()
            .filter(|b| b.is_live() && self.group_enabled(b.group))
            .collect();
        if v.is_empty() {
            return None;
        }
        if forward {
            v.iter()
                .find(|b| b.anchor.line as usize > from)
                .or(v.first())
                .copied()
        } else {
            v.reverse();
            v.iter()
                .find(|b| (b.anchor.line as usize) < from)
                .or(v.first())
                .copied()
        }
    }

    /// L0: 문서의 줄 변경 한 건을 전 항목에 적용.
    pub fn apply_change(
        &mut self,
        doc: &DocKey,
        first: usize,
        removed: usize,
        inserted: usize,
        ci: bool,
    ) {
        for b in self
            .items
            .iter_mut()
            .filter(|b| b.doc.same(doc, ci) && b.is_live())
        {
            let (nl, shifted) = shift_line(b.anchor.line as usize, first, removed, inserted);
            b.anchor.line = nl as u32;
            if shifted {
                b.shifted = true;
            }
        }
        // C-1: 같은 줄로 모인 것은 하나로(먼저 것을 남긴다).
        let mut seen: Vec<(DocKey, u32)> = Vec::new();
        self.items.retain(|b| {
            if !b.doc.same(doc, ci) || !b.is_live() {
                return true;
            }
            let key = (b.doc.clone(), b.anchor.line);
            if seen.iter().any(|k| k.1 == key.1 && k.0.same(&key.0, ci)) {
                false
            } else {
                seen.push(key);
                true
            }
        });
    }

    /// 니모닉 지정(문서별 유일 · 이미 쓰면 그 항목에서 뺀다) — 뺏긴 항목 id.
    pub fn set_mnemonic(&mut self, id: u64, n: Option<u8>, ci: bool) -> Option<u64> {
        let doc = self.get(id)?.doc.clone();
        let mut taken = None;
        if let Some(n) = n {
            for b in self.items.iter_mut() {
                if b.id != id && b.doc.same(&doc, ci) && b.mnemonic == Some(n) {
                    b.mnemonic = None;
                    taken = Some(b.id);
                }
            }
        }
        if let Some(b) = self.get_mut(id) {
            b.mnemonic = n;
        }
        taken
    }

    #[must_use]
    pub fn by_mnemonic(&self, doc: &DocKey, n: u8, ci: bool) -> Option<&Bookmark> {
        self.items
            .iter()
            .find(|b| b.doc.same(doc, ci) && b.mnemonic == Some(n) && b.is_live())
    }

    /// 무효 정리(69 §5) — 지운 수.
    pub fn prune(&mut self, now: u64, stale_days: u32) -> usize {
        let before = self.items.len();
        self.items
            .retain(|b| !should_prune(b.state, now, stale_days));
        before - self.items.len()
    }

    /// 살아 있는 항목 수 / 무효 수.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        let live = self.items.iter().filter(|b| b.is_live()).count();
        (live, self.items.len() - live)
    }

    // ───────────── JSON(69 §7-2) ─────────────

    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n  \"version\": 1,\n  \"next_id\": ");
        out.push_str(&self.next_id.to_string());
        out.push_str(",\n  \"next_group\": ");
        out.push_str(&self.next_group.to_string());
        out.push_str(",\n  \"groups\": [");
        for (i, g) in self.groups.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "\n    {{ \"id\": {}, \"name\": {}, \"default\": {}, \"enabled\": {} }}",
                g.id,
                jstr(&g.name),
                g.default,
                g.enabled
            ));
        }
        out.push_str("\n  ],\n  \"items\": [");
        let mut items: Vec<&Bookmark> = self.items.iter().collect();
        items.sort_by_key(|b| b.id); // id 순 = git 충돌 최소(69 §14)
        for (i, b) in items.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("\n    { \"id\": ");
            out.push_str(&b.id.to_string());
            out.push_str(", \"doc\": ");
            match &b.doc {
                DocKey::File { path } => out.push_str(&format!("{{ \"file\": {} }}", jstr(path))),
                DocKey::Scratch { tab } => out.push_str(&format!("{{ \"tab\": {tab} }}")),
                DocKey::Object {
                    profile,
                    dialect,
                    schema,
                    kind,
                    name,
                    part,
                } => out.push_str(&format!(
                    "{{ \"obj\": {{ \"profile\": {}, \"dialect\": {}, \"schema\": {}, \"kind\": {}, \"name\": {}, \"part\": {} }} }}",
                    jstr(profile),
                    jstr(dialect),
                    jstr(schema),
                    jstr(kind),
                    jstr(name),
                    jstr(part)
                )),
            }
            let a = &b.anchor;
            out.push_str(&format!(
                ", \"line\": {}, \"col\": {}, \"text\": {}, \"before\": {}, \"after\": {}, \"hash\": \"{:016x}\", \"lines\": {}",
                a.line,
                a.col,
                jstr(&a.text),
                jstr(&a.before),
                jstr(&a.after),
                a.doc_hash,
                a.doc_lines
            ));
            if let Some(l) = &b.label {
                out.push_str(&format!(", \"label\": {}", jstr(l)));
            }
            if let Some(nt) = &b.note {
                out.push_str(&format!(", \"note\": {}", jstr(nt)));
            }
            if let Some(m) = b.mnemonic {
                out.push_str(&format!(", \"mn\": {m}"));
            }
            out.push_str(&format!(
                ", \"group\": {}, \"shared\": {}, \"created\": {}, \"visited\": {}",
                b.group, b.shared, b.created, b.visited
            ));
            if b.shifted {
                out.push_str(", \"shifted\": true");
            }
            if let State::Invalid { since, reason } = b.state {
                out.push_str(&format!(
                    ", \"state\": {{ \"invalid\": {{ \"since\": {}, \"reason\": \"{}\" }} }}",
                    since,
                    reason.as_str()
                ));
            }
            out.push_str(" }");
        }
        out.push_str("\n  ]\n}\n");
        out
    }

    /// JSON에서 읽는다 — 모르는 키는 무시 · 깨진 항목은 그 항목만 버린다.
    pub fn from_json(text: &str) -> Result<Store, String> {
        let json = parse(text)?;
        let Json::Obj(fields) = json else {
            return Err("bookmarks: not an object".into());
        };
        let mut st = Store {
            items: Vec::new(),
            groups: Vec::new(),
            next_id: 1,
            next_group: 2,
        };
        for (k, v) in &fields {
            match k.as_str() {
                "next_id" => st.next_id = num(v).unwrap_or(1.0) as u64,
                "next_group" => st.next_group = num(v).unwrap_or(2.0) as u32,
                "groups" => {
                    if let Json::Arr(gs) = v {
                        for g in gs {
                            if let Json::Obj(f) = g {
                                st.groups.push(Group {
                                    id: field_num(f, "id").unwrap_or(0.0) as u32,
                                    name: field_str(f, "name").unwrap_or_default(),
                                    default: field_bool(f, "default").unwrap_or(false),
                                    enabled: field_bool(f, "enabled").unwrap_or(true),
                                });
                            }
                        }
                    }
                }
                "items" => {
                    if let Json::Arr(items) = v {
                        for it in items {
                            if let Json::Obj(f) = it {
                                if let Some(b) = parse_item(f) {
                                    st.items.push(b);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        st.ensure_default_group();
        let max_id = st.items.iter().map(|b| b.id).max().unwrap_or(0);
        if st.next_id <= max_id {
            st.next_id = max_id + 1;
        }
        let max_g = st.groups.iter().map(|g| g.id).max().unwrap_or(1);
        if st.next_group <= max_g {
            st.next_group = max_g + 1;
        }
        Ok(st)
    }
}

fn parse_item(f: &[(String, Json)]) -> Option<Bookmark> {
    let id = field_num(f, "id")? as u64;
    let doc = match field(f, "doc")? {
        Json::Obj(d) => {
            if let Some(p) = field_str(d, "file") {
                DocKey::file(&p)
            } else if let Some(t) = field_num(d, "tab") {
                DocKey::Scratch { tab: t as u64 }
            } else if let Some(Json::Obj(o)) = field(d, "obj") {
                DocKey::Object {
                    profile: field_str(o, "profile").unwrap_or_default(),
                    dialect: field_str(o, "dialect").unwrap_or_default(),
                    schema: field_str(o, "schema").unwrap_or_default(),
                    kind: field_str(o, "kind").unwrap_or_default(),
                    name: field_str(o, "name").unwrap_or_default(),
                    part: field_str(o, "part").unwrap_or_default(),
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let hash = field_str(f, "hash")
        .and_then(|h| u64::from_str_radix(&h, 16).ok())
        .unwrap_or(0);
    let state = match field(f, "state") {
        Some(Json::Obj(s)) => match field(s, "invalid") {
            Some(Json::Obj(inv)) => State::Invalid {
                since: field_num(inv, "since").unwrap_or(0.0) as u64,
                reason: field_str(inv, "reason")
                    .and_then(|r| Reason::parse(&r))
                    .unwrap_or(Reason::TextGone),
            },
            _ => State::Live,
        },
        _ => State::Live,
    };
    Some(Bookmark {
        id,
        doc,
        anchor: Anchor {
            line: field_num(f, "line").unwrap_or(0.0) as u32,
            col: field_num(f, "col").unwrap_or(0.0) as u32,
            text: field_str(f, "text").unwrap_or_default(),
            before: field_str(f, "before").unwrap_or_default(),
            after: field_str(f, "after").unwrap_or_default(),
            doc_hash: hash,
            doc_lines: field_num(f, "lines").unwrap_or(0.0) as u32,
        },
        label: field_str(f, "label"),
        note: field_str(f, "note"),
        mnemonic: field_num(f, "mn").map(|n| n as u8),
        group: field_num(f, "group").unwrap_or(1.0) as u32,
        shared: field_bool(f, "shared").unwrap_or(false),
        state,
        created: field_num(f, "created").unwrap_or(0.0) as u64,
        visited: field_num(f, "visited").unwrap_or(0.0) as u64,
        shifted: field_bool(f, "shifted").unwrap_or(false),
    })
}

fn field<'a>(f: &'a [(String, Json)], k: &str) -> Option<&'a Json> {
    f.iter().find(|(n, _)| n == k).map(|(_, v)| v)
}
fn num(v: &Json) -> Option<f64> {
    match v {
        Json::Num(n) => Some(*n),
        _ => None,
    }
}
fn field_num(f: &[(String, Json)], k: &str) -> Option<f64> {
    field(f, k).and_then(num)
}
fn field_str(f: &[(String, Json)], k: &str) -> Option<String> {
    match field(f, k) {
        Some(Json::Str(s)) => Some(s.clone()),
        _ => None,
    }
}
fn field_bool(f: &[(String, Json)], k: &str) -> Option<bool> {
    match field(f, k) {
        Some(Json::Bool(b)) => Some(*b),
        _ => None,
    }
}

/// JSON 문자열 리터럴.
fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
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
    out.push('"');
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::redundant_clone)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    /// L0 줄 보정 = 단순 모델(전체 줄 배열을 실제로 바꿔 북마크 줄의 새 위치를 찾는다)과 난수 대조.
    #[test]
    fn shift_line_matches_naive_model() {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rnd = |m: usize| -> usize {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % (m as u64 + 1)) as usize
        };
        for _ in 0..2000 {
            let n = 1 + rnd(30);
            let mark = rnd(n - 1);
            let first = rnd(n);
            let removed = rnd(n - first);
            let inserted = rnd(5);
            // 단순 모델: 줄마다 id를 두고 편집을 실제로 적용한다.
            let mut ids: Vec<Option<usize>> = (0..n).map(Some).collect();
            let repl: Vec<Option<usize>> = (0..inserted).map(|_| None).collect();
            ids.splice(first..first + removed, repl);
            let expect = ids.iter().position(|&x| x == Some(mark));
            let (got, shifted) = shift_line(mark, first, removed, inserted);
            match expect {
                Some(e) => {
                    assert_eq!(
                        got, e,
                        "n={n} mark={mark} first={first} removed={removed} inserted={inserted}"
                    );
                    assert!(!shifted);
                }
                None => {
                    // 지워진 줄: 대치면 제자리(첫 삽입 줄들 안) · 아니면 경계.
                    assert!(
                        got >= first && got < first + inserted.max(1)
                            || (inserted == 0 && got == first)
                    );
                    assert_eq!(shifted, inserted == 0);
                }
            }
        }
    }

    #[test]
    fn relocate_seven_cases() {
        let o = RelocateOpts::default();
        let src = "SELECT 1\nFROM T\nWHERE A = 1\nAND B = 2\nORDER BY 1";
        let ls = lines(src);
        let a = make_anchor(&ls, 2, 0, &o); // WHERE A = 1
                                            // 같은 본문 = Same.
        assert_eq!(relocate(&a, &ls, None, &o), Relocated::Same);
        // 위 삽입 → 아래로.
        let s2 = "-- hdr\n-- hdr2\nSELECT 1\nFROM T\nWHERE A = 1\nAND B = 2\nORDER BY 1";
        assert_eq!(relocate(&a, &lines(s2), None, &o), Relocated::Moved(4));
        // 위 삭제 → 위로.
        let s3 = "FROM T\nWHERE A = 1\nAND B = 2";
        assert_eq!(relocate(&a, &lines(s3), None, &o), Relocated::Moved(1));
        // 그 줄 수정(유사) → 유사도로 잇는다.
        let s4 = "SELECT 1\nFROM T\nWHERE A = 10\nAND B = 2\nORDER BY 1";
        assert_eq!(relocate(&a, &lines(s4), None, &o), Relocated::Moved(2));
        // 그 줄 삭제 → 문맥(앞뒤)만으로는 문턱 미달이면 무효.
        let s5 = "SELECT 1\nFROM T\nAND B = 2\nORDER BY 1";
        assert!(matches!(
            relocate(&a, &lines(s5), None, &o),
            Relocated::Gone | Relocated::Moved(_)
        ));
        // 블록 이동(정합 표 제공) → 표를 따른다.
        let s6 = "ORDER BY 1\nSELECT 1\nFROM T\nWHERE A = 1\nAND B = 2";
        let map = [Some(1), Some(2), Some(3), Some(4), Some(0)];
        assert_eq!(
            relocate(&a, &lines(s6), Some(&map), &o),
            Relocated::Moved(3)
        );
        // 전체 재포맷(들여쓰기) → trim 비교로 잇는다.
        let s7 = "  SELECT 1\n    FROM T\n    WHERE A = 1\n    AND B = 2\n  ORDER BY 1";
        assert_eq!(
            relocate(&a, &lines(s7), None, &o),
            Relocated::Same,
            "trim 비교 = 같은 줄 번호"
        );
        // 중복 줄: 원래 자리에 가까운 쪽.
        let s8 = "WHERE A = 1\nx\nx\nWHERE A = 1\nx";
        assert_eq!(relocate(&a, &lines(s8), None, &o), Relocated::Moved(3));
        // 큰 문서 = TooBig.
        let big: Vec<&str> = (0..10).map(|_| "x").collect();
        let o2 = RelocateOpts {
            max_lines: 5,
            ..RelocateOpts::default()
        };
        assert_eq!(relocate(&a, &big, None, &o2), Relocated::TooBig);
    }

    #[test]
    fn dice_similarity_basics() {
        assert!((dice("night", "nacht") - 0.25).abs() < 1e-6);
        assert_eq!(dice("abc", "abc"), 1.0);
        assert_eq!(dice("", ""), 1.0);
        assert_eq!(dice("a", "b"), 0.0);
    }

    #[test]
    fn store_toggle_next_prev_and_json_round_trip() {
        let o = RelocateOpts::default();
        let ls = lines("a\nb\nc\nd");
        let mut st = Store::new();
        let doc = DocKey::file("D:\\x\\Q.sql");
        assert_eq!(
            doc,
            DocKey::File {
                path: "D:/x/Q.sql".into()
            }
        );
        let id1 = st
            .toggle(doc.clone(), make_anchor(&ls, 1, 0, &o), 10, true, 500, 5000)
            .unwrap()
            .unwrap();
        let id3 = st
            .toggle(doc.clone(), make_anchor(&ls, 3, 0, &o), 11, true, 500, 5000)
            .unwrap()
            .unwrap();
        assert_ne!(id1, id3);
        // 같은 줄 토글 = 제거.
        assert_eq!(
            st.toggle(doc.clone(), make_anchor(&ls, 1, 0, &o), 12, true, 500, 5000)
                .unwrap(),
            None
        );
        assert_eq!(st.for_doc(&doc, true).len(), 1);
        st.toggle(doc.clone(), make_anchor(&ls, 0, 0, &o), 13, true, 500, 5000)
            .unwrap();
        // 다음/이전 순환.
        assert_eq!(st.next_in_doc(&doc, 0, true, true).unwrap().anchor.line, 3);
        assert_eq!(
            st.next_in_doc(&doc, 3, true, true).unwrap().anchor.line,
            0,
            "끝 → 처음"
        );
        assert_eq!(
            st.next_in_doc(&doc, 0, false, true).unwrap().anchor.line,
            3,
            "처음 ← 끝"
        );
        // 대소문자 무시 열쇠.
        let same = DocKey::file("d:/X/q.SQL");
        assert_eq!(st.for_doc(&same, true).len(), 2);
        assert_eq!(st.for_doc(&same, false).len(), 0);
        // 니모닉 유일.
        st.set_mnemonic(id3, Some(3), true);
        let id0 = st.at_line(&doc, 0, true).unwrap().id;
        assert_eq!(st.set_mnemonic(id0, Some(3), true), Some(id3));
        assert_eq!(st.by_mnemonic(&doc, 3, true).unwrap().id, id0);
        // 상한.
        let mut small = Store::new();
        small
            .add(doc.clone(), make_anchor(&ls, 0, 0, &o), 1, true, 1, 0)
            .unwrap();
        assert_eq!(
            small.add(doc.clone(), make_anchor(&ls, 1, 0, &o), 1, true, 1, 0),
            Err("max_per_doc")
        );
        // JSON 왕복.
        if let Some(b) = st.get_mut(id0) {
            b.label = Some("이름 \"q\"".into());
            b.note = Some("메모\n둘째".into());
            b.state = State::Invalid {
                since: 5,
                reason: Reason::FileMissing,
            };
        }
        st.add_group("검증");
        let text = st.to_json();
        let back = Store::from_json(&text).unwrap();
        assert_eq!(back, st);
        // 모르는 키 무시 · 깨진 항목만 버림.
        let odd = "{\"version\":1,\"zzz\":1,\"items\":[{\"id\":9,\"doc\":{\"file\":\"a\"},\"line\":1},{\"nope\":true}]}";
        let s2 = Store::from_json(odd).unwrap();
        assert_eq!(s2.items.len(), 1);
        assert_eq!(s2.next_id, 10);
        assert_eq!(s2.groups.len(), 1, "기본 그룹은 만들어 둔다");
    }

    #[test]
    fn apply_change_and_prune_mcdc() {
        let o = RelocateOpts::default();
        let ls = lines("a\nb\nc\nd\ne");
        let mut st = Store::new();
        let doc = DocKey::file("f.sql");
        for l in [1, 3, 4] {
            st.add(doc.clone(), make_anchor(&ls, l, 0, &o), 1, true, 0, 0)
                .unwrap();
        }
        // 줄 0 앞에 두 줄 삽입 → 전부 +2.
        st.apply_change(&doc, 0, 0, 2, true);
        let ls_now: Vec<u32> = st
            .for_doc(&doc, true)
            .iter()
            .map(|b| b.anchor.line)
            .collect();
        assert_eq!(ls_now, vec![3, 5, 6]);
        // 줄 5~6 삭제(둘 다 안) → 둘 다 경계 5로 모이고 C-1로 하나만 남는다.
        st.apply_change(&doc, 5, 2, 0, true);
        let ls_now: Vec<u32> = st
            .for_doc(&doc, true)
            .iter()
            .map(|b| b.anchor.line)
            .collect();
        assert_eq!(ls_now, vec![3, 5]);
        assert!(st.for_doc(&doc, true)[1].shifted);
        // 정리 판정 MC/DC: (무효, stale_days>0, 경과≥N).
        let inv = State::Invalid {
            since: 0,
            reason: Reason::TextGone,
        };
        assert!(should_prune(inv, 31 * 86_400, 30));
        assert!(!should_prune(State::Live, 31 * 86_400, 30), "무효 아님");
        assert!(!should_prune(inv, 31 * 86_400, 0), "0 = 영원히");
        assert!(!should_prune(inv, 29 * 86_400, 30), "경과 미달");
        if let Some(b) = st.items.first_mut() {
            b.state = inv;
        }
        assert_eq!(st.prune(40 * 86_400, 30), 1);
        assert_eq!(st.counts(), (1, 0));
    }
}
