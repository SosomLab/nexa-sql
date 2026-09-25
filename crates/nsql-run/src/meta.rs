//! **메타 저장소 한 벌**(docs/47 · T-57 ①) — 탐색기 트리 · 자동 완성 · hover 툴팁 · 팔레트 · CLI가 **같은 데이터**를 읽는다.
//!
//! - 이름은 [`Interner`]로 인터닝(`Sym`) · 소문자 키를 함께 둔다(대소문자 무시 조회 · 표시는 원문).
//! - 객체 목록은 **버킷(스키마 × 종류)** 단위로 채우고 갱신한다(점증 수집 · 디프 · 부분 서비스 = `Coverage`).
//! - 조회: 정확 = 해시 O(1) · 접두 = 버킷/전역 **소문자 정렬 배열 + `partition_point`** O(log n + k).
//! - 읽기는 [`MetaStore::snapshot`]이 돌려주는 `Arc<Snapshot>`에서(락 0 · 갱신은 새 스냅샷 교체 · 버킷은 `Arc` 구조 공유).
//! - UI·드라이버 무의존 — 카탈로그 결과(`ObjEntry`/`ColEntry`)를 호스트가 넣는다. 47 §3-1 유니버설 모델(채울 수 없는 값 = `None`).

use nsql_catalog::ObjectKind;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// 인터닝된 이름(원문 보존).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sym(pub u32);

/// 이름 인터너 — 원문 + 소문자 키. 같은 원문은 같은 `Sym`.
#[derive(Debug, Default, Clone)]
pub struct Interner {
    names: Vec<Arc<str>>,
    lower: Vec<Arc<str>>,
    by_name: HashMap<Arc<str>, Sym>,
}

impl Interner {
    pub fn intern(&mut self, s: &str) -> Sym {
        if let Some(&id) = self.by_name.get(s) {
            return id;
        }
        let id = Sym(self.names.len() as u32);
        let a: Arc<str> = Arc::from(s);
        self.names.push(a.clone());
        self.lower.push(Arc::from(s.to_lowercase().as_str()));
        self.by_name.insert(a, id);
        id
    }
    #[must_use]
    pub fn get(&self, s: Sym) -> &str {
        &self.names[s.0 as usize]
    }
    #[must_use]
    pub fn lower(&self, s: Sym) -> &str {
        &self.lower[s.0 as usize]
    }
    /// 원문 → 이미 인터닝된 Sym(없으면 None · 읽기 전용 조회용).
    #[must_use]
    pub fn find(&self, s: &str) -> Option<Sym> {
        self.by_name.get(s).copied()
    }
    /// 소문자 키 Sym(원문이 이미 소문자면 그 Sym) — `exact`/`by_name`의 키.
    pub fn intern_lower(&mut self, s: Sym) -> Sym {
        let lower = self.lower[s.0 as usize].clone();
        if *lower == *self.names[s.0 as usize] {
            return s;
        }
        self.intern(&lower)
    }
    /// 읽기 전용 소문자 키(인터닝돼 있어야 함 · 아니면 원문 Sym).
    #[must_use]
    pub fn lower_key(&self, s: Sym) -> Sym {
        self.find(self.lower(s)).unwrap_or(s)
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
    /// 대략 바이트(문자열 2벌 + 표).
    #[must_use]
    pub fn approx_bytes(&self) -> usize {
        self.names.iter().map(|n| n.len() * 2 + 48).sum::<usize>() + self.by_name.len() * 24
    }
}

/// 객체 상태(47 §3-1 · 모르면 `Unknown`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ObjStatus {
    #[default]
    Unknown,
    Valid,
    Invalid,
}

/// 객체 하나(유니버설 상위집합 · 47 §3-1) — 채울 수 없는 항목은 `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjEntry {
    pub schema: Sym,
    pub name: Sym,
    pub kind: ObjectKind,
    pub status: ObjStatus,
    /// 수정 시각(epoch 초 · 없으면 None).
    pub modified: Option<u64>,
    pub comment: Option<Sym>,
    /// 부가(PG 오버로드 서명 · 시노님 대상 등).
    pub extra: Option<Sym>,
}

/// 컬럼 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColEntry {
    pub name: Sym,
    pub data_type: Sym,
    pub nullable: Option<bool>,
    pub position: u16,
    pub default: Option<Sym>,
    pub comment: Option<Sym>,
    /// PK · FK · UQ 비트.
    pub key: u8,
}

pub const KEY_PK: u8 = 1;
pub const KEY_FK: u8 = 2;
pub const KEY_UQ: u8 = 4;

/// 테이블 상세(제약·인덱스 — 완성 상세 카드·객체 정보 · 09-24). 종류 = `P`/`U`/`R`/`C`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEntry {
    pub name: Sym,
    pub kind: char,
    pub cols: Vec<Sym>,
    pub ref_table: Option<Sym>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    pub name: Sym,
    pub unique: bool,
    pub cols: Vec<Sym>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableDetail {
    pub keys: Vec<KeyEntry>,
    pub indexes: Vec<IndexEntry>,
    /// 테이블 코멘트(Description · 09-24).
    pub comment: Option<Sym>,
    /// 컬럼 코멘트(이름 심볼, 코멘트 심볼).
    pub col_comments: Vec<(Sym, Sym)>,
}

/// 테이블 상세 채움 상태(컬럼과 별개 — 필요할 때 한 번).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum DetailState {
    #[default]
    Unknown,
    Loading,
    Loaded {
        at: u64,
        detail: Arc<TableDetail>,
    },
}

/// 카탈로그 → 상세 입력.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewDetail {
    pub keys: Vec<(String, char, Vec<String>, Option<String>)>,
    pub indexes: Vec<(String, bool, Vec<String>)>,
    pub comment: Option<String>,
    pub col_comments: Vec<(String, String)>,
}

/// 버킷(스키마 × 종류)의 채움 상태 — 부분 서비스의 근거.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Coverage {
    Missing,
    Loading,
    Loaded {
        at: u64,
        n: usize,
    },
    /// ★ 낡음(79 §3 · 명시 갱신 뒤): 목록은 살아 있어 소비자는 `Loaded`처럼 쓰고, 다음 요청 때 다시 읽는다(깜빡임 0).
    Stale {
        at: u64,
        n: usize,
    },
    /// ★ L1 이름 층(85 §2): 이름 인덱스가 채운 이름만(상태·부가 없음) — 소비자(완성·검색)는 `Loaded`처럼 후보를 내고,
    /// 상태가 필요한 곳(카드·트리)이 L2 승격(`upgrading`)을 청한다. 빈 버킷도 `n = 0`으로 남는다(부정 캐시 = 다시 묻지 않음).
    Names {
        at: u64,
        n: usize,
        upgrading: bool,
    },
    Error {
        message: String,
        at: u64,
    },
}

impl Coverage {
    /// 후보를 낼 수 있는 상태(목록이 있다) — Loaded · Stale · Names.
    #[must_use]
    pub fn has_list(&self) -> bool {
        matches!(
            self,
            Coverage::Loaded { .. } | Coverage::Stale { .. } | Coverage::Names { .. }
        )
    }
}

/// 버킷 — 객체 id 목록은 **소문자 이름 정렬**(접두 이진 탐색).
#[derive(Clone, Debug)]
pub struct Bucket {
    pub schema: Sym,
    pub kind: ObjectKind,
    pub coverage: Coverage,
    pub objs: Vec<ObjId>,
}

/// 스냅샷 안 객체 인덱스.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjId(pub u32);

/// 컬럼 채움 상태.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColState {
    Unknown,
    Loading,
    Loaded { at: u64, cols: Arc<Vec<ColEntry>> },
    Error(String),
}

/// 읽기 전용 스냅샷 — 갱신은 새 스냅샷으로 교체(버킷·컬럼은 `Arc` 공유 · 바뀐 것만 새로).
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub stamp: u64,
    pub current_schema: Option<Sym>,
    pub schemas: Vec<Sym>,
    pub objs: Vec<ObjEntry>,
    pub cols: Vec<ColState>,
    /// 테이블 상세(제약·인덱스) — 요청한 것만(09-24).
    pub details: HashMap<ObjId, DetailState>,
    pub buckets: HashMap<(Sym, ObjectKind), Arc<Bucket>>,
    /// (스키마, 소문자 이름) → 객체(같은 이름이 종류별로 있으면 첫 것 · 종류 지정 조회는 버킷으로).
    exact: HashMap<(Sym, Sym), ObjId>,
    /// 전 스키마 소문자 이름 정렬(스키마 모를 때 접두) — ★ **지연 계산**(85 §6 · 09-25 프레임 끊김): 버킷이 바뀔 때 비우고
    /// `prefix_any`가 처음 부를 때 만든다(L1이 스키마 30개를 채우는 동안 20k 정렬을 30번 하지 않게).
    by_name: OnceLock<Vec<(Sym, ObjId)>>,
}

/// 메타 저장소 — 쓰기는 이 구조체, 읽기는 [`MetaStore::snapshot`].
#[derive(Debug, Default)]
pub struct MetaStore {
    pub names: Interner,
    snap: Arc<Snapshot>,
    /// 컬럼 LRU(스키마별 마지막 사용 스탬프 · 메모리 상한 초과 시 오래된 스키마 컬럼부터 내림).
    col_used: HashMap<Sym, u64>,
    max_bytes: usize,
}

/// 접두 조회 결과 한 줄.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub id: ObjId,
    pub schema: Sym,
    pub name: Sym,
    pub kind: ObjectKind,
}

impl MetaStore {
    #[must_use]
    pub fn new(max_bytes: usize) -> Self {
        MetaStore {
            names: Interner::default(),
            snap: Arc::new(Snapshot::default()),
            col_used: HashMap::new(),
            max_bytes: max_bytes.max(1 << 20),
        }
    }

    /// 현재 스냅샷(락 없음 · 소비자는 이것만 읽는다).
    #[must_use]
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snap.clone()
    }

    fn edit(&mut self) -> Snapshot {
        let mut s = (*self.snap).clone();
        s.stamp += 1;
        s
    }

    fn commit(&mut self, s: Snapshot) {
        self.snap = Arc::new(s);
    }

    /// 스키마 목록 + 현재 스키마.
    pub fn set_schemas(&mut self, schemas: &[String], current: Option<&str>) {
        let mut s = self.edit();
        s.schemas = schemas.iter().map(|n| self.names.intern(n)).collect();
        s.schemas
            .sort_by(|a, b| self.names.lower(*a).cmp(self.names.lower(*b)));
        s.current_schema = current.map(|c| self.names.intern(c));
        self.commit(s);
    }

    /// 버킷을 "불러오는 중"으로(부분 서비스 표시).
    pub fn mark_loading(&mut self, schema: &str, kind: ObjectKind) {
        let sc = self.names.intern(schema);
        let mut s = self.edit();
        let b = s.buckets.entry((sc, kind)).or_insert_with(|| {
            Arc::new(Bucket {
                schema: sc,
                kind,
                coverage: Coverage::Missing,
                objs: Vec::new(),
            })
        });
        match b.coverage {
            Coverage::Loaded { .. } => {}
            // L1 이름 층은 승격 중에도 후보를 계속 낸다(깜빡임 0).
            Coverage::Names { at, n, .. } => {
                let mut nb = (**b).clone();
                nb.coverage = Coverage::Names {
                    at,
                    n,
                    upgrading: true,
                };
                *b = Arc::new(nb);
            }
            _ => {
                let mut nb = (**b).clone();
                nb.coverage = Coverage::Loading;
                *b = Arc::new(nb);
            }
        }
        self.commit(s);
    }

    /// ★ L1 이름 층 채움(85 §2): 스키마 하나의 (종류, 이름) 전부를 **한 번의 편집**으로 — `kinds`의 모든 종류에 버킷을 만들고
    /// (없는 종류 = 빈 버킷 `Names{n:0}` = 부정 캐시), 이미 `Loaded`/`Stale`/`Loading`인 버킷은 건드리지 않는다(내려가지 않음).
    /// 같은 이름의 기존 객체는 id를 유지(컬럼 그대로). 반환 = 새로 만든 객체 수.
    pub fn load_names(
        &mut self,
        schema: &str,
        kinds: &[ObjectKind],
        names: &[(ObjectKind, String)],
        at: u64,
    ) -> usize {
        let sc = self.names.intern(schema);
        let mut s = self.edit();
        let mut added = 0usize;
        for &kind in kinds {
            let keep = s.buckets.get(&(sc, kind)).is_some_and(|b| {
                matches!(
                    b.coverage,
                    Coverage::Loaded { .. } | Coverage::Stale { .. } | Coverage::Loading
                )
            });
            if keep {
                continue;
            }
            let old: Vec<ObjId> = s
                .buckets
                .get(&(sc, kind))
                .map(|b| b.objs.clone())
                .unwrap_or_default();
            let mut old_by_lower: HashMap<Sym, ObjId> = HashMap::with_capacity(old.len());
            for id in &old {
                let n = s.objs[id.0 as usize].name;
                old_by_lower.insert(self.names.intern_lower(n), *id);
            }
            let mut objs: Vec<ObjId> = Vec::new();
            let mut seen: std::collections::HashSet<ObjId> = std::collections::HashSet::new();
            for (k, n) in names.iter().filter(|(k, _)| *k == kind) {
                let name = self.names.intern(n);
                let lower = self.names.intern_lower(name);
                let id = match old_by_lower.get(&lower) {
                    Some(&id) => id,
                    None => {
                        added += 1;
                        let id = ObjId(s.objs.len() as u32);
                        s.objs.push(ObjEntry {
                            schema: sc,
                            name,
                            kind: *k,
                            status: ObjStatus::Unknown,
                            modified: None,
                            comment: None,
                            extra: None,
                        });
                        s.cols.push(ColState::Unknown);
                        id
                    }
                };
                if seen.insert(id) {
                    objs.push(id);
                }
            }
            for id in &old {
                if !seen.contains(id) {
                    let key = self.names.lower_key(s.objs[id.0 as usize].name);
                    if s.exact.get(&(sc, key)) == Some(id) {
                        s.exact.remove(&(sc, key));
                    }
                }
            }
            {
                let names = &self.names;
                let objs_ref = &s.objs;
                objs.sort_by(|a, b| {
                    names
                        .lower(objs_ref[a.0 as usize].name)
                        .cmp(names.lower(objs_ref[b.0 as usize].name))
                });
            }
            for id in &objs {
                let lower = self.names.lower_key(s.objs[id.0 as usize].name);
                s.exact.entry((sc, lower)).or_insert(*id);
            }
            s.buckets.insert(
                (sc, kind),
                Arc::new(Bucket {
                    schema: sc,
                    kind,
                    coverage: Coverage::Names {
                        at,
                        n: objs.len(),
                        upgrading: false,
                    },
                    objs,
                }),
            );
        }
        // 전역 이름 정렬은 지연(다음 `prefix_any`가 만든다).
        s.by_name = OnceLock::new();
        self.commit(s);
        added
    }

    /// ★ L3 회수(85 §4 · "불필요시 최대한 빠르게"): 상세(제약·인덱스)는 `detail_ttl`을 넘긴 것과 `detail_max`를 넘는 오래된 것부터 ·
    /// 컬럼은 현재 스키마(`keep`) 밖에서 `cols_ttl`을 넘긴 것을 `Unknown`으로(목록은 그대로 · 다음 요청 때 다시). 반환 = (상세, 컬럼) 비운 수.
    pub fn reclaim(
        &mut self,
        now: u64,
        keep: Option<Sym>,
        detail_max: usize,
        detail_ttl: u64,
        cols_ttl: u64,
    ) -> (usize, usize) {
        let snap = &self.snap;
        let mut loaded: Vec<(ObjId, u64)> = snap
            .details
            .iter()
            .filter_map(|(id, d)| match d {
                DetailState::Loaded { at, .. } => Some((*id, *at)),
                _ => None,
            })
            .collect();
        loaded.sort_by_key(|(_, at)| *at);
        let mut drop_details: Vec<ObjId> = loaded
            .iter()
            .filter(|(_, at)| detail_ttl > 0 && now.saturating_sub(*at) > detail_ttl)
            .map(|(id, _)| *id)
            .collect();
        let remaining = loaded.len().saturating_sub(drop_details.len());
        if detail_max > 0 && remaining > detail_max {
            let survivors: Vec<ObjId> = loaded
                .iter()
                .filter(|(id, _)| !drop_details.contains(id))
                .map(|(id, _)| *id)
                .collect();
            let extra = survivors.len() - detail_max;
            drop_details.extend(survivors.into_iter().take(extra));
        }
        let drop_cols: Vec<usize> = if cols_ttl == 0 {
            Vec::new()
        } else {
            snap.cols
                .iter()
                .enumerate()
                .filter(|(i, c)| {
                    matches!(c, ColState::Loaded { at, .. } if now.saturating_sub(*at) > cols_ttl)
                        && keep.is_none_or(|k| snap.objs[*i].schema != k)
                })
                .map(|(i, _)| i)
                .collect()
        };
        if drop_details.is_empty() && drop_cols.is_empty() {
            return (0, 0);
        }
        let mut s = self.edit();
        for id in &drop_details {
            s.details.remove(id);
        }
        for i in &drop_cols {
            s.cols[*i] = ColState::Unknown;
        }
        self.commit(s);
        (drop_details.len(), drop_cols.len())
    }

    /// 버킷 오류.
    pub fn mark_error(&mut self, schema: &str, kind: ObjectKind, message: &str, at: u64) {
        let sc = self.names.intern(schema);
        let mut s = self.edit();
        let b = s.buckets.entry((sc, kind)).or_insert_with(|| {
            Arc::new(Bucket {
                schema: sc,
                kind,
                coverage: Coverage::Missing,
                objs: Vec::new(),
            })
        });
        let mut nb = (**b).clone();
        nb.coverage = Coverage::Error {
            message: message.to_string(),
            at,
        };
        *b = Arc::new(nb);
        self.commit(s);
    }

    /// ★ 버킷 채우기/갱신 = **디프**(47 §5): 같은 이름은 `ObjId`를 유지(컬럼 그대로) · 수정 시각·상태·코멘트가 바뀐 것만 교체
    /// (수정 시각이 바뀐 테이블은 컬럼을 `Unknown`으로) · 없어진 것은 버킷에서 뺀다(tombstone · 컴팩션은 `compact`).
    /// 반환 = (추가, 변경, 삭제) 수.
    pub fn load_bucket(
        &mut self,
        schema: &str,
        kind: ObjectKind,
        items: &[NewObj],
        at: u64,
    ) -> (usize, usize, usize) {
        let sc = self.names.intern(schema);
        let mut s = self.edit();
        let old: Vec<ObjId> = s
            .buckets
            .get(&(sc, kind))
            .map(|b| b.objs.clone())
            .unwrap_or_default();
        let mut old_by_lower: HashMap<Sym, ObjId> = HashMap::with_capacity(old.len());
        for id in &old {
            let n = s.objs[id.0 as usize].name;
            old_by_lower.insert(self.names.intern_lower(n), *id);
        }
        let (mut added, mut changed, mut removed) = (0usize, 0usize, 0usize);
        let mut objs: Vec<ObjId> = Vec::with_capacity(items.len());
        let mut seen: std::collections::HashSet<ObjId> = std::collections::HashSet::new();
        for it in items {
            let name = self.names.intern(&it.name);
            let lower = self.names.intern_lower(name);
            let entry = ObjEntry {
                schema: sc,
                name,
                kind,
                status: it.status,
                modified: it.modified,
                comment: it.comment.as_deref().map(|c| self.names.intern(c)),
                extra: it.extra.as_deref().map(|c| self.names.intern(c)),
            };
            let id = match old_by_lower.get(&lower) {
                Some(&id) => {
                    let cur = &s.objs[id.0 as usize];
                    if *cur != entry {
                        changed += 1;
                        let modified_changed = cur.modified != entry.modified;
                        s.objs[id.0 as usize] = entry;
                        if modified_changed {
                            s.cols[id.0 as usize] = ColState::Unknown;
                        }
                    }
                    id
                }
                None => {
                    added += 1;
                    let id = ObjId(s.objs.len() as u32);
                    s.objs.push(entry);
                    s.cols.push(ColState::Unknown);
                    id
                }
            };
            seen.insert(id);
            objs.push(id);
        }
        for id in &old {
            if !seen.contains(id) {
                removed += 1;
                let key = self.names.lower_key(s.objs[id.0 as usize].name);
                s.exact.remove(&(sc, key));
            }
        }
        // 소문자 정렬(접두 이진 탐색).
        {
            let names = &self.names;
            let objs_ref = &s.objs;
            objs.sort_by(|a, b| {
                names
                    .lower(objs_ref[a.0 as usize].name)
                    .cmp(names.lower(objs_ref[b.0 as usize].name))
            });
        }
        for id in &objs {
            let lower = self.names.lower_key(s.objs[id.0 as usize].name);
            s.exact.entry((sc, lower)).or_insert(*id);
        }
        s.buckets.insert(
            (sc, kind),
            Arc::new(Bucket {
                schema: sc,
                kind,
                coverage: Coverage::Loaded { at, n: objs.len() },
                objs,
            }),
        );
        // 전역 이름 정렬은 지연(다음 `prefix_any`가 만든다).
        s.by_name = OnceLock::new();
        self.commit(s);
        (added, changed, removed)
    }

    /// 컬럼 채우기(테이블 하나).
    pub fn set_columns(&mut self, id: ObjId, cols: &[NewCol], at: u64) {
        let mut s = self.edit();
        if let Some(slot) = s.cols.get_mut(id.0 as usize) {
            let list: Vec<ColEntry> = cols
                .iter()
                .map(|c| ColEntry {
                    name: self.names.intern(&c.name),
                    data_type: self.names.intern(&c.data_type),
                    nullable: c.nullable,
                    position: c.position,
                    default: c.default.as_deref().map(|d| self.names.intern(d)),
                    comment: c.comment.as_deref().map(|d| self.names.intern(d)),
                    key: c.key,
                })
                .collect();
            *slot = ColState::Loaded {
                at,
                cols: Arc::new(list),
            };
            let schema = s.objs[id.0 as usize].schema;
            self.col_used.insert(schema, at);
        }
        self.commit(s);
        self.evict_if_needed(at);
    }

    /// 테이블 상세 채우기(제약·인덱스).
    pub fn set_detail(&mut self, id: ObjId, d: &NewDetail, at: u64) {
        let mut s = self.edit();
        let keys = d
            .keys
            .iter()
            .map(|(n, k, cols, rf)| KeyEntry {
                name: self.names.intern(n),
                kind: *k,
                cols: cols.iter().map(|c| self.names.intern(c)).collect(),
                ref_table: rf.as_deref().map(|r| self.names.intern(r)),
            })
            .collect();
        let indexes = d
            .indexes
            .iter()
            .map(|(n, u, cols)| IndexEntry {
                name: self.names.intern(n),
                unique: *u,
                cols: cols.iter().map(|c| self.names.intern(c)).collect(),
            })
            .collect();
        let comment = d.comment.as_deref().map(|c| self.names.intern(c));
        let col_comments = d
            .col_comments
            .iter()
            .map(|(n, c)| (self.names.intern(n), self.names.intern(c)))
            .collect();
        s.details.insert(
            id,
            DetailState::Loaded {
                at,
                detail: Arc::new(TableDetail {
                    keys,
                    indexes,
                    comment,
                    col_comments,
                }),
            },
        );
        self.commit(s);
    }

    pub fn mark_detail_loading(&mut self, id: ObjId) {
        let mut s = self.edit();
        if !matches!(s.details.get(&id), Some(DetailState::Loaded { .. })) {
            s.details.insert(id, DetailState::Loading);
        }
        self.commit(s);
    }

    /// 상세 되돌리기(실패 · 컬럼이 바뀜 → 다음 요청 때 다시).
    pub fn reset_detail(&mut self, id: ObjId) {
        let mut s = self.edit();
        s.details.remove(&id);
        self.commit(s);
    }

    /// 컬럼 상태 되돌리기(요청 실패 · 다음 요청 때 다시).
    pub fn reset_columns(&mut self, id: ObjId) {
        let mut s = self.edit();
        if let Some(slot) = s.cols.get_mut(id.0 as usize) {
            if matches!(slot, ColState::Loading) {
                *slot = ColState::Unknown;
            }
        }
        self.commit(s);
    }

    pub fn mark_columns_loading(&mut self, id: ObjId) {
        let mut s = self.edit();
        if let Some(slot) = s.cols.get_mut(id.0 as usize) {
            if !matches!(slot, ColState::Loaded { .. }) {
                *slot = ColState::Loading;
            }
        }
        self.commit(s);
    }

    /// 메모리 상한(47 §3): 넘치면 가장 오래 안 쓴 스키마의 컬럼부터 내린다(객체 목록은 유지).
    fn evict_if_needed(&mut self, now: u64) {
        let _ = now;
        while self.approx_bytes() > self.max_bytes {
            let Some((&victim, _)) = self.col_used.iter().min_by_key(|(_, &t)| t) else {
                break;
            };
            let mut s = self.edit();
            let mut dropped = 0;
            for (i, o) in s.objs.iter().enumerate() {
                if o.schema == victim && matches!(s.cols[i], ColState::Loaded { .. }) {
                    s.cols[i] = ColState::Unknown;
                    dropped += 1;
                }
            }
            self.commit(s);
            self.col_used.remove(&victim);
            if dropped == 0 {
                break;
            }
        }
    }

    /// 대략 메모리(바이트).
    #[must_use]
    pub fn approx_bytes(&self) -> usize {
        let s = &self.snap;
        let cols: usize = s
            .cols
            .iter()
            .map(|c| match c {
                ColState::Loaded { cols, .. } => cols.len() * 32,
                _ => 0,
            })
            .sum();
        let details: usize = s
            .details
            .values()
            .map(|d| match d {
                DetailState::Loaded { detail, .. } => {
                    64 + detail.keys.len() * 48
                        + detail.indexes.len() * 40
                        + detail.col_comments.len() * 16
                }
                _ => 8,
            })
            .sum();
        self.names.approx_bytes()
            + s.objs.len() * 40
            + cols
            + details
            + s.by_name.get().map_or(0, Vec::len) * 12
            + s.exact.len() * 24
    }

    /// 층별 대략 메모리(85 §5 · 메모리 창): (L1 이름·목록, L2 컬럼, L3 상세) 바이트.
    #[must_use]
    pub fn layer_bytes(&self) -> (usize, usize, usize) {
        let s = &self.snap;
        let l2: usize = s
            .cols
            .iter()
            .map(|c| match c {
                ColState::Loaded { cols, .. } => cols.len() * 32,
                _ => 0,
            })
            .sum();
        let l3: usize = s
            .details
            .values()
            .map(|d| match d {
                DetailState::Loaded { detail, .. } => {
                    64 + detail.keys.len() * 48
                        + detail.indexes.len() * 40
                        + detail.col_comments.len() * 16
                }
                _ => 8,
            })
            .sum();
        let l1 = self.names.approx_bytes()
            + s.objs.len() * 40
            + s.by_name.get().map_or(0, Vec::len) * 12
            + s.exact.len() * 24;
        (l1, l2, l3)
    }

    /// ★ 버킷 해제(미사용 판정 즉시 회수 · 사용자 09-23): 그 (스키마, 종류)의 객체 목록·정확 조회 열쇠·전역 이름 정렬·컬럼을
    /// 버리고 버킷을 없앤다(커버리지 = `Missing` · 다음 요청 때 다시 읽는다). 객체 행 자체는 id 안정성 때문에 남긴다(행당 40 B ·
    /// 컬럼이 무거운 쪽이고 그것은 비운다). 반환 = 실제로 비웠는가.
    pub fn drop_bucket(&mut self, schema: &str, kind: ObjectKind) -> bool {
        let Some(sc) = self.names.find(schema) else {
            return false;
        };
        let Some(b) = self.snap.buckets.get(&(sc, kind)).cloned() else {
            return false;
        };
        let mut s = self.edit();
        for id in &b.objs {
            let key = self.names.lower_key(s.objs[id.0 as usize].name);
            if s.exact.get(&(sc, key)) == Some(id) {
                s.exact.remove(&(sc, key));
            }
            s.cols[id.0 as usize] = ColState::Unknown;
            s.details.remove(id);
        }
        s.buckets.remove(&(sc, kind));
        s.by_name = OnceLock::new();
        self.commit(s);
        true
    }

    /// ★ 명시 갱신(79 §3 · T-187): `schema`(None = 전부)의 `Loaded` 버킷을 `Stale`로 — 목록은 살아 있어 완성은 후보를 내고,
    /// 다음 요청(`request_objects`)이 다시 읽는다. 반환 = 표시한 버킷 수.
    pub fn mark_stale(&mut self, schema: Option<&str>) -> usize {
        let want = schema.and_then(|s| self.names.find(s));
        if schema.is_some() && want.is_none() {
            return 0;
        }
        let mut s = self.edit();
        let mut n = 0;
        for ((sc, _), b) in s.buckets.iter_mut() {
            if want.is_some_and(|w| w != *sc) {
                continue;
            }
            if let Coverage::Loaded { at, n: cnt } = b.coverage {
                let mut nb = (**b).clone();
                nb.coverage = Coverage::Stale { at, n: cnt };
                *b = Arc::new(nb);
                n += 1;
            }
        }
        self.commit(s);
        n
    }

    /// ★ 명시 갱신: `schema`(None = 전부) 객체의 컬럼·상세를 `Unknown`으로(목록은 유지 · 다음 요청 때 다시). 반환 = 비운 객체 수.
    pub fn mark_columns_unknown(&mut self, schema: Option<&str>) -> usize {
        let want = schema.and_then(|s| self.names.find(s));
        if schema.is_some() && want.is_none() {
            return 0;
        }
        let mut s = self.edit();
        let mut n = 0;
        for i in 0..s.cols.len() {
            let sc = s.objs[i].schema;
            if want.is_some_and(|w| w != sc) {
                continue;
            }
            if matches!(s.cols[i], ColState::Loaded { .. }) {
                s.cols[i] = ColState::Unknown;
                s.details.remove(&ObjId(i as u32));
                n += 1;
            }
        }
        self.commit(s);
        n
    }

    /// 상한 변경.
    pub fn set_max_bytes(&mut self, n: usize) {
        self.max_bytes = n.max(1 << 20);
        self.evict_if_needed(0);
    }

    /// 전부 비움(접속 해제).
    pub fn clear(&mut self) {
        self.names = Interner::default();
        self.snap = Arc::new(Snapshot::default());
        self.col_used.clear();
    }
}

/// 카탈로그 → 저장소 입력(호스트가 `nsql_catalog::ObjectInfo`에서 변환).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewObj {
    pub name: String,
    pub status: ObjStatus,
    pub modified: Option<u64>,
    pub comment: Option<String>,
    pub extra: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewCol {
    pub name: String,
    pub data_type: String,
    pub nullable: Option<bool>,
    pub position: u16,
    pub default: Option<String>,
    pub comment: Option<String>,
    pub key: u8,
}

impl Snapshot {
    /// 정확 조회(대소문자 무시) — `schema`가 None이면 현재 스키마 → 전 스키마 순.
    #[must_use]
    pub fn lookup(&self, names: &Interner, schema: Option<&str>, name: &str) -> Option<ObjId> {
        let lname = name.to_lowercase();
        // O(1): 소문자 키 Sym이 인터닝돼 있지 않으면 그런 이름은 없다.
        let lkey = names.find(&lname)?;
        let find_in = |sc: Sym| -> Option<ObjId> { self.exact.get(&(sc, lkey)).copied() };
        match schema {
            Some(sc) => {
                let lsc = sc.to_lowercase();
                // 접속 스키마 목록에 없는 버킷(사전 `$dict` · 09-24 "ALL_TABLES 컬럼이 안 온다")도 인터닝된 이름으로 찾는다.
                let sym = self
                    .schemas
                    .iter()
                    .copied()
                    .find(|s| names.lower(*s) == lsc)
                    .or_else(|| names.find(sc))?;
                find_in(sym)
            }
            None => {
                if let Some(cur) = self.current_schema {
                    if let Some(id) = find_in(cur) {
                        return Some(id);
                    }
                }
                // 전 스키마: 정렬 배열 이진 탐색.
                let by_name = self.by_name_sorted(names);
                let i = by_name.partition_point(|(s, _)| names.lower(*s) < lname.as_str());
                by_name
                    .get(i)
                    .filter(|(s, _)| names.lower(*s) == lname)
                    .map(|(_, id)| *id)
            }
        }
    }

    /// 접두 조회(버킷 · 대소문자 무시 · `limit`) — O(log n + k).
    #[must_use]
    pub fn prefix(
        &self,
        names: &Interner,
        schema: Sym,
        kind: ObjectKind,
        prefix: &str,
        limit: usize,
    ) -> Vec<Hit> {
        let p = prefix.to_lowercase();
        let Some(b) = self.buckets.get(&(schema, kind)) else {
            return Vec::new();
        };
        let lo = b
            .objs
            .partition_point(|id| names.lower(self.objs[id.0 as usize].name) < p.as_str());
        b.objs[lo..]
            .iter()
            .take_while(|id| names.lower(self.objs[id.0 as usize].name).starts_with(&p))
            .take(limit)
            .map(|id| {
                let o = &self.objs[id.0 as usize];
                Hit {
                    id: *id,
                    schema: o.schema,
                    name: o.name,
                    kind: o.kind,
                }
            })
            .collect()
    }

    /// 전 스키마 접두 조회(스키마를 모를 때).
    #[must_use]
    /// 전 스키마 소문자 이름 정렬(지연 · 처음 부를 때 한 번 · 85 §6).
    fn by_name_sorted(&self, names: &Interner) -> &Vec<(Sym, ObjId)> {
        self.by_name.get_or_init(|| {
            let mut v: Vec<(Sym, ObjId)> = self
                .buckets
                .values()
                .flat_map(|b| b.objs.iter().copied())
                .map(|id| (names.lower_key(self.objs[id.0 as usize].name), id))
                .collect();
            v.sort_by(|a, b| names.get(a.0).cmp(names.get(b.0)));
            v
        })
    }

    pub fn prefix_any(&self, names: &Interner, prefix: &str, limit: usize) -> Vec<Hit> {
        let p = prefix.to_lowercase();
        let by_name = self.by_name_sorted(names);
        let lo = by_name.partition_point(|(s, _)| names.lower(*s) < p.as_str());
        by_name[lo..]
            .iter()
            .take_while(|(s, _)| names.lower(*s).starts_with(&p))
            .take(limit)
            .map(|(_, id)| {
                let o = &self.objs[id.0 as usize];
                Hit {
                    id: *id,
                    schema: o.schema,
                    name: o.name,
                    kind: o.kind,
                }
            })
            .collect()
    }

    /// 테이블 상세 상태(없으면 Unknown).
    #[must_use]
    pub fn detail(&self, id: ObjId) -> DetailState {
        self.details.get(&id).cloned().unwrap_or_default()
    }

    /// 컬럼(없으면 상태만).
    #[must_use]
    pub fn columns(&self, id: ObjId) -> &ColState {
        self.cols.get(id.0 as usize).unwrap_or(&ColState::Unknown)
    }

    /// 컬럼 접두(대소문자 무시).
    #[must_use]
    pub fn column_prefix<'a>(
        &'a self,
        names: &'a Interner,
        id: ObjId,
        prefix: &str,
        limit: usize,
    ) -> Vec<&'a ColEntry> {
        let p = prefix.to_lowercase();
        match self.columns(id) {
            ColState::Loaded { cols, .. } => cols
                .iter()
                .filter(|c| names.lower(c.name).starts_with(&p))
                .take(limit)
                .collect(),
            _ => Vec::new(),
        }
    }

    #[must_use]
    pub fn coverage(&self, schema: Sym, kind: ObjectKind) -> Coverage {
        self.buckets
            .get(&(schema, kind))
            .map_or(Coverage::Missing, |b| b.coverage.clone())
    }

    #[must_use]
    pub fn object(&self, id: ObjId) -> Option<&ObjEntry> {
        self.objs.get(id.0 as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 명시 갱신(79 §3 · T-187): `mark_stale`는 Loaded → Stale(목록·접두 조회는 그대로) · `mark_columns_unknown`은 컬럼·상세를 비움 ·
    /// 스키마를 주면 그 스키마만 · 모르는 스키마 = 0.
    #[test]
    fn mark_stale_and_columns_unknown_keep_lists() {
        let mut m = MetaStore::new(1 << 20);
        m.set_schemas(&["hr".into(), "fin".into()], Some("hr"));
        m.load_bucket("hr", ObjectKind::Table, &[obj("EMP", 1)], 1);
        m.load_bucket("fin", ObjectKind::Table, &[obj("ACCT", 1)], 1);
        let emp = m
            .snapshot()
            .lookup(&m.names, Some("hr"), "EMP")
            .expect("emp");
        m.set_columns(
            emp,
            &[NewCol {
                name: "EMPNO".into(),
                data_type: "NUMBER".into(),
                ..Default::default()
            }],
            1,
        );
        assert_eq!(m.mark_stale(Some("nope")), 0);
        assert_eq!(m.mark_stale(Some("hr")), 1, "hr 버킷 하나");
        let s = m.snapshot();
        let hr = m.names.find("hr").expect("hr");
        let fin = m.names.find("fin").expect("fin");
        assert!(matches!(
            s.coverage(hr, ObjectKind::Table),
            Coverage::Stale { n: 1, .. }
        ));
        assert!(matches!(
            s.coverage(fin, ObjectKind::Table),
            Coverage::Loaded { .. }
        ));
        assert_eq!(
            s.prefix(&m.names, hr, ObjectKind::Table, "e", 10).len(),
            1,
            "목록은 그대로"
        );
        assert!(matches!(s.columns(emp), ColState::Loaded { .. }));
        assert_eq!(m.mark_columns_unknown(Some("fin")), 0, "fin은 컬럼 없음");
        assert_eq!(m.mark_columns_unknown(None), 1);
        assert!(matches!(m.snapshot().columns(emp), ColState::Unknown));
        // 다시 읽으면 Loaded로.
        m.mark_loading("hr", ObjectKind::Table);
        m.load_bucket("hr", ObjectKind::Table, &[obj("EMP", 1)], 2);
        assert!(matches!(
            m.snapshot().coverage(hr, ObjectKind::Table),
            Coverage::Loaded { .. }
        ));
    }

    fn obj(name: &str, modified: u64) -> NewObj {
        NewObj {
            name: name.into(),
            modified: Some(modified),
            ..NewObj::default()
        }
    }

    /// 버킷 채움 → 접두/정확 조회 → 디프(변경·추가·삭제 · ObjId 유지 · 수정 시각 바뀌면 컬럼 Unknown).
    #[test]
    fn bucket_lookup_prefix_and_diff_keeps_ids() {
        let mut m = MetaStore::new(64 << 20);
        m.set_schemas(&["HR".into(), "SALES".into()], Some("HR"));
        let hr = m.snapshot().schemas[0];
        let items = vec![
            obj("EMP", 1),
            obj("Dept", 1),
            obj("emp_hist", 1),
            obj("JOBS", 1),
        ];
        let (a, c, r) = m.load_bucket("HR", ObjectKind::Table, &items, 10);
        assert_eq!((a, c, r), (4, 0, 0));
        let s = m.snapshot();
        let hits = s.prefix(&m.names, hr, ObjectKind::Table, "em", 10);
        let names: Vec<&str> = hits.iter().map(|h| m.names.get(h.name)).collect();
        assert_eq!(names, vec!["EMP", "emp_hist"], "소문자 정렬 · 접두");
        let id = s.lookup(&m.names, None, "dept").expect("현재 스키마에서");
        assert_eq!(m.names.get(s.object(id).unwrap().name), "Dept");
        assert!(s.lookup(&m.names, Some("hr"), "jobs").is_some());
        assert!(s.lookup(&m.names, Some("SALES"), "EMP").is_none());
        // 컬럼
        m.set_columns(
            id,
            &[
                NewCol {
                    name: "DEPT_ID".into(),
                    data_type: "NUMBER".into(),
                    position: 1,
                    key: KEY_PK,
                    ..NewCol::default()
                },
                NewCol {
                    name: "DNAME".into(),
                    data_type: "VARCHAR2".into(),
                    position: 2,
                    ..NewCol::default()
                },
            ],
            11,
        );
        let s = m.snapshot();
        assert_eq!(s.column_prefix(&m.names, id, "d", 10).len(), 2);
        assert_eq!(s.column_prefix(&m.names, id, "dn", 10).len(), 1);
        // 디프: Dept 수정(컬럼 Unknown) · JOBS 삭제 · NEW 추가 · EMP 그대로(id 유지)
        let emp_id = s.lookup(&m.names, Some("HR"), "EMP").unwrap();
        let items2 = vec![
            obj("EMP", 1),
            obj("Dept", 2),
            obj("emp_hist", 1),
            obj("NEW_T", 1),
        ];
        let (a, c, r) = m.load_bucket("HR", ObjectKind::Table, &items2, 20);
        assert_eq!((a, c, r), (1, 1, 1));
        let s = m.snapshot();
        assert_eq!(
            s.lookup(&m.names, Some("HR"), "EMP"),
            Some(emp_id),
            "ObjId 유지"
        );
        assert!(
            matches!(s.columns(id), ColState::Unknown),
            "수정 시각 바뀜 → 컬럼 Unknown"
        );
        assert!(s.lookup(&m.names, Some("HR"), "JOBS").is_none(), "삭제");
        assert!(matches!(
            s.coverage(hr, ObjectKind::Table),
            Coverage::Loaded { n: 4, .. }
        ));
        // 전 스키마 접두
        m.load_bucket("SALES", ObjectKind::View, &[obj("EMP_V", 1)], 21);
        let s = m.snapshot();
        let any = s.prefix_any(&m.names, "emp", 10);
        assert_eq!(any.len(), 3, "EMP · emp_hist · EMP_V");
        // 부분 서비스 표시
        m.mark_loading("SALES", ObjectKind::Table);
        assert_eq!(
            m.snapshot()
                .coverage(m.snapshot().schemas[1], ObjectKind::Table),
            Coverage::Loading
        );
    }

    /// 메모리 상한: 오래 안 쓴 스키마 컬럼부터 내린다(객체 목록 유지).
    #[test]
    fn eviction_drops_old_schema_columns_first() {
        let mut m = MetaStore::new(1 << 20);
        m.set_schemas(&["A".into(), "B".into()], Some("A"));
        m.load_bucket("A", ObjectKind::Table, &[obj("T1", 1)], 1);
        m.load_bucket("B", ObjectKind::Table, &[obj("T2", 1)], 1);
        let s = m.snapshot();
        let t1 = s.lookup(&m.names, Some("A"), "T1").unwrap();
        let t2 = s.lookup(&m.names, Some("B"), "T2").unwrap();
        let cols: Vec<NewCol> = (0..2000)
            .map(|i| NewCol {
                name: format!("C{i}"),
                data_type: "INT".into(),
                position: i as u16,
                ..NewCol::default()
            })
            .collect();
        m.set_columns(t1, &cols, 1);
        m.set_columns(t2, &cols, 2);
        m.set_max_bytes(1 << 20); // 낮은 상한 → 오래된 A부터
        let s = m.snapshot();
        assert!(
            matches!(s.columns(t1), ColState::Unknown)
                || matches!(s.columns(t2), ColState::Loaded { .. })
        );
        assert!(
            s.object(t1).is_some() && s.object(t2).is_some(),
            "객체 목록은 유지"
        );
    }

    /// 버킷 해제(미사용 즉시 회수 · 09-23): 목록·정확 조회·전역 정렬·컬럼이 비고 커버리지는 Missing · 다른 버킷은 그대로.
    #[test]
    fn drop_bucket_clears_index_and_columns_but_keeps_others() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["A".into(), "B".into()], Some("A"));
        m.load_bucket(
            "A",
            ObjectKind::Table,
            &[NewObj {
                name: "T1".into(),
                ..Default::default()
            }],
            1,
        );
        m.load_bucket(
            "B",
            ObjectKind::Table,
            &[NewObj {
                name: "T2".into(),
                ..Default::default()
            }],
            1,
        );
        let id = m.snapshot().lookup(&m.names, Some("B"), "t2").unwrap();
        m.set_columns(
            id,
            &[NewCol {
                name: "C".into(),
                ..Default::default()
            }],
            1,
        );
        assert!(m.drop_bucket("B", ObjectKind::Table));
        let s = m.snapshot();
        assert!(matches!(
            s.coverage(m.names.find("B").unwrap(), ObjectKind::Table),
            Coverage::Missing
        ));
        assert!(s.lookup(&m.names, Some("B"), "T2").is_none());
        assert!(matches!(s.columns(id), ColState::Unknown));
        assert!(s
            .prefix_any(&m.names, "t", 10)
            .iter()
            .all(|h| m.names.get(h.name) != "T2"));
        assert!(
            s.lookup(&m.names, Some("A"), "T1").is_some(),
            "다른 버킷은 그대로"
        );
        assert!(
            !m.drop_bucket("B", ObjectKind::Table),
            "두 번째는 비울 것이 없다"
        );
        assert!(!m.drop_bucket("ZZ", ObjectKind::Table));
    }

    /// 스키마 목록에 없는 버킷(사전 `$dict`)도 스키마를 지정한 조회로 찾는다(09-24 `ALL_TABLES` 컬럼 요청 누락의 원인).
    #[test]
    fn lookup_finds_objects_in_buckets_outside_schema_list() {
        let mut m = MetaStore::new(1 << 24);
        m.set_schemas(&["BISCM".into()], Some("BISCM"));
        m.load_bucket(
            "$dict",
            ObjectKind::View,
            &[NewObj {
                name: "ALL_TABLES".into(),
                ..Default::default()
            }],
            1,
        );
        let s = m.snapshot();
        assert!(s.lookup(&m.names, Some("$dict"), "all_tables").is_some());
        assert!(s.lookup(&m.names, Some("BISCM"), "ALL_TABLES").is_none());
        assert!(
            s.lookup(&m.names, None, "ALL_TABLES").is_some(),
            "전 스키마 조회에도 든다"
        );
    }
}

#[cfg(test)]
mod layer_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn kinds() -> Vec<ObjectKind> {
        vec![ObjectKind::Table, ObjectKind::View]
    }

    /// L1: 이름만으로 버킷(빈 종류도 `Names{n:0}`) · 소비자는 `has_list` · L2 승격(`load_bucket`)은 같은 이름의 id를 유지 ·
    /// 이미 Loaded인 버킷은 L1이 내리지 않는다.
    #[test]
    fn names_layer_then_upgrade_keeps_ids() {
        let mut m = MetaStore::new(1 << 20);
        let names = vec![
            (ObjectKind::Table, "EMP".to_string()),
            (ObjectKind::Table, "DEPT".to_string()),
        ];
        assert_eq!(m.load_names("HR", &kinds(), &names, 10), 2);
        let snap = m.snapshot();
        let hr = m.names.find("HR").unwrap();
        assert!(snap.coverage(hr, ObjectKind::Table).has_list());
        assert!(
            matches!(
                snap.coverage(hr, ObjectKind::View),
                Coverage::Names { n: 0, .. }
            ),
            "빈 종류 = 부정 캐시"
        );
        let emp = snap.lookup(&m.names, Some("HR"), "emp").unwrap();
        // 승격 중에도 목록은 산다.
        m.mark_loading("HR", ObjectKind::Table);
        assert!(matches!(
            m.snapshot().coverage(hr, ObjectKind::Table),
            Coverage::Names {
                upgrading: true,
                ..
            }
        ));
        let objs = vec![
            NewObj {
                name: "EMP".into(),
                status: ObjStatus::Valid,
                ..Default::default()
            },
            NewObj {
                name: "DEPT".into(),
                status: ObjStatus::Valid,
                ..Default::default()
            },
            NewObj {
                name: "NEW_T".into(),
                status: ObjStatus::Valid,
                ..Default::default()
            },
        ];
        let (added, _, removed) = m.load_bucket("HR", ObjectKind::Table, &objs, 20);
        assert_eq!((added, removed), (1, 0));
        let snap = m.snapshot();
        assert_eq!(
            snap.lookup(&m.names, Some("HR"), "emp"),
            Some(emp),
            "id 유지"
        );
        assert!(matches!(
            snap.coverage(hr, ObjectKind::Table),
            Coverage::Loaded { n: 3, .. }
        ));
        // L1을 다시 채워도 Loaded는 그대로.
        m.load_names("HR", &kinds(), &names, 30);
        assert!(matches!(
            m.snapshot().coverage(hr, ObjectKind::Table),
            Coverage::Loaded { n: 3, .. }
        ));
    }

    /// L3 회수: 상세는 TTL·상한(오래된 것부터) · 컬럼은 현재 스키마 밖 + TTL.
    #[test]
    fn reclaim_details_by_ttl_and_cap_and_cold_columns() {
        let mut m = MetaStore::new(1 << 20);
        let t = |n: &str| NewObj {
            name: n.into(),
            ..Default::default()
        };
        m.load_bucket("HR", ObjectKind::Table, &[t("A"), t("B"), t("C")], 1);
        m.load_bucket("SALES", ObjectKind::Table, &[t("S1")], 1);
        let snap = m.snapshot();
        let ids: Vec<ObjId> = ["A", "B", "C"]
            .iter()
            .map(|n| snap.lookup(&m.names, Some("HR"), n).unwrap())
            .collect();
        let s1 = snap.lookup(&m.names, Some("SALES"), "S1").unwrap();
        let col = NewCol {
            name: "X".into(),
            data_type: "INT".into(),
            ..Default::default()
        };
        for (i, id) in ids.iter().enumerate() {
            m.set_detail(*id, &NewDetail::default(), 100 + i as u64 * 10);
            m.set_columns(*id, std::slice::from_ref(&col), 100);
        }
        m.set_columns(s1, std::slice::from_ref(&col), 100);
        let hr = m.names.find("HR");
        // TTL 25 → at 100·110 만료(now 140) · 상한 1 → 남은 120 중 하나만 → 셋 다? 아니: 만료 둘 + 상한 안 = 셋 중 둘 비움.
        let (d, c) = m.reclaim(140, hr, 1, 25, 30);
        assert_eq!(d, 2, "TTL 만료 둘 · 남은 하나는 상한 1 안");
        assert_eq!(c, 1, "현재 스키마(HR) 컬럼은 남고 SALES만 비움");
        assert!(matches!(m.snapshot().columns(s1), ColState::Unknown));
        assert!(matches!(
            m.snapshot().columns(ids[0]),
            ColState::Loaded { .. }
        ));
        // 상한만(TTL 0) → 남은 상세 1개는 상한 1 안 → 0.
        assert_eq!(m.reclaim(1000, None, 1, 0, 0), (0, 0));
    }
}
