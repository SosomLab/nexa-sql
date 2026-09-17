//! **트랜잭션 로그**(docs/44 · T-107 · 사용자 09-17) — DBeaver Query Manager의 골격(수집기 → 메모리 링 → 필터 → 표)에
//! **트랜잭션 결과를 문장 행의 열로** 명시한다(DBeaver는 색만 · Result 열은 늘 Success → 롤백이 안 보였다).
//!
//! - [`TxEntry`] = 문장 실행 하나(DBeaver `QMMStatementExecuteInfo`) · [`TxRecord`] = 수동 커밋 구간(`QMMTransactionSavepointInfo`).
//! - 문장 행은 `tx → TxRecord.outcome`을 **표시 시점**에 읽으므로 커밋/롤백 뒤 행을 갱신할 필요가 없다.
//! - UI 무의존(호스트 GUI·CLI 공용) · 링 상한 `cap`(설정 `txlog.max_entries`) · 오류 문장은 트랜잭션에 넣지 않는다(DBeaver `QMUtils`와 동일).

use std::collections::VecDeque;
use std::time::Duration;

/// 문장의 목적(DBeaver `DBCExecutionPurpose`의 축약).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// 사용자가 편집기/CLI에서 실행한 문장.
    User,
    /// 탐색기·카탈로그 질의(메타 세션).
    Meta,
    /// 추가 페치·COUNT·PRINT 같은 보조 질의.
    Util,
}

/// 실행 결과.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecOutcome {
    Running,
    Ok,
    Err {
        code: Option<i64>,
        message: String,
    },
    /// 사용자가 중지(받은 행 수).
    Stopped {
        rows: u64,
    },
}

/// 트랜잭션 결과 — 문장 행의 **Tx 열**(DBeaver에 없던 것).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxOutcome {
    /// 열린 수동 트랜잭션(미커밋).
    Pending,
    /// 문장마다 커밋(자동 커밋 모드 · `TxEntry.tx == None`).
    Auto,
    Committed,
    RolledBack,
    /// DDL 등 암묵 커밋(무엇이 일으켰는가).
    ImplicitCommit(String),
    /// 자동 커밋으로 전환되며 정리됨.
    Switched,
    /// 접속 끊김·재접속·세션 소실(서버가 롤백).
    Lost,
}

/// 문장 실행 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxEntry {
    pub id: u64,
    /// 시작 시각(`YYYY-MM-DD HH:MM:SS.mmm` — 호스트가 준다 · UI 무의존).
    pub at: String,
    /// 편집기 탭 id(0 = CLI/알 수 없음).
    pub editor: u64,
    pub purpose: Purpose,
    /// 원문(표는 한 줄로 접어 보여 준다).
    pub text: String,
    pub duration: Option<Duration>,
    /// 페치 행 또는 영향 행.
    pub rows: Option<u64>,
    pub result: ExecOutcome,
    /// 속한 [`TxRecord`](자동 커밋이면 `None`).
    pub tx: Option<u64>,
    /// 같은 실행 배치 안의 문장 번호(호스트의 `RunEvent.index`).
    pub index: usize,
}

/// 수동 커밋 구간.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxRecord {
    pub id: u64,
    pub started: String,
    pub ended: Option<String>,
    pub outcome: TxOutcome,
    /// 영향 행 > 0인 문장 수(툴바 배지와 같은 수).
    pub updates: usize,
}

/// 표시 필터.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TxFilter {
    /// 문장 부분 일치(대소문자 무시 · 비면 전부).
    pub text: String,
    /// `false` = 사용자 문장만(Meta·Util 제외).
    pub all_purposes: bool,
    /// `false` = 열린 트랜잭션(대기)과 자동 커밋 최근 문장만 · `true` = 닫힌 트랜잭션도.
    pub previous: bool,
    /// `Some(id)` = 그 편집기 탭의 문장만.
    pub editor: Option<u64>,
}

/// 수집기 + 링.
#[derive(Debug)]
pub struct TxLog {
    entries: VecDeque<TxEntry>,
    records: VecDeque<TxRecord>,
    next_id: u64,
    next_tx: u64,
    open_tx: Option<u64>,
    cap: usize,
    /// 현재 실행 배치의 (index → entry id) — 결과/오류 이벤트를 문장에 붙인다.
    current: Vec<(usize, u64)>,
}

impl Default for TxLog {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl TxLog {
    #[must_use]
    pub fn new(cap: usize) -> Self {
        TxLog {
            entries: VecDeque::new(),
            records: VecDeque::new(),
            next_id: 1,
            next_tx: 1,
            open_tx: None,
            cap: cap.max(16),
            current: Vec::new(),
        }
    }

    /// 링 상한(설정 변경) — 넘치면 오래된 것부터 버린다.
    pub fn set_cap(&mut self, cap: usize) {
        self.cap = cap.max(16);
        self.trim();
    }

    fn trim(&mut self) {
        while self.entries.len() > self.cap {
            self.entries.pop_front();
        }
        // 어떤 문장도 가리키지 않는 닫힌 기록은 버린다(열린 것은 유지).
        let used: std::collections::HashSet<u64> =
            self.entries.iter().filter_map(|e| e.tx).collect();
        let open = self.open_tx;
        self.records
            .retain(|r| used.contains(&r.id) || Some(r.id) == open);
    }

    /// 새 실행 배치 시작(앞 배치의 index 매핑을 버린다).
    pub fn begin_batch(&mut self) {
        self.current.clear();
    }

    /// 문장 실행 시작 → entry id.
    pub fn begin(
        &mut self,
        at: String,
        editor: u64,
        purpose: Purpose,
        index: usize,
        text: &str,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push_back(TxEntry {
            id,
            at,
            editor,
            purpose,
            text: text.to_string(),
            duration: None,
            rows: None,
            result: ExecOutcome::Running,
            tx: None,
            index,
        });
        self.current.retain(|(i, _)| *i != index);
        self.current.push((index, id));
        self.trim();
        id
    }

    fn entry_mut(&mut self, index: usize) -> Option<&mut TxEntry> {
        let id = self
            .current
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, id)| *id)?;
        self.entries.iter_mut().rev().find(|e| e.id == id)
    }

    /// 조회 결과(첫 세그먼트) 도착.
    pub fn result(&mut self, index: usize, rows: u64, elapsed: Duration) {
        if let Some(e) = self.entry_mut(index) {
            e.rows = Some(rows);
            e.duration = Some(elapsed);
            e.result = ExecOutcome::Ok;
        }
    }

    /// DML/DDL 완료.
    pub fn done(&mut self, index: usize, rows_affected: Option<u64>, elapsed: Duration) {
        if let Some(e) = self.entry_mut(index) {
            e.rows = rows_affected;
            e.duration = Some(elapsed);
            e.result = ExecOutcome::Ok;
        }
    }

    /// 단계별 소요가 뒤따라오면 총합으로 갱신(26 §2 Timeline).
    pub fn timing(&mut self, index: usize, total: Duration) {
        if let Some(e) = self.entry_mut(index) {
            e.duration = Some(total);
        }
    }

    /// 오류 — 트랜잭션에는 넣지 않는다(이미 붙어 있었다면 뗀다 · DBeaver 규칙).
    pub fn error(&mut self, index: usize, code: Option<i64>, message: &str) {
        if let Some(e) = self.entry_mut(index) {
            e.result = ExecOutcome::Err {
                code,
                message: message.to_string(),
            };
            e.tx = None;
        }
    }

    /// 중지(부분 행).
    pub fn stopped(&mut self, index: usize, rows: u64) {
        if let Some(e) = self.entry_mut(index) {
            e.result = ExecOutcome::Stopped { rows };
            e.rows = Some(rows);
        }
    }

    /// 이 문장을 **열린 수동 트랜잭션**에 붙인다(없으면 연다 · `counts` = 영향 행이 있는 갱신 문장인가).
    pub fn attach_tx(&mut self, index: usize, at: String, counts: bool) {
        let tx = match self.open_tx {
            Some(id) => id,
            None => {
                let id = self.next_tx;
                self.next_tx += 1;
                self.records.push_back(TxRecord {
                    id,
                    started: at,
                    ended: None,
                    outcome: TxOutcome::Pending,
                    updates: 0,
                });
                self.open_tx = Some(id);
                id
            }
        };
        if counts {
            if let Some(r) = self.records.iter_mut().find(|r| r.id == tx) {
                r.updates += 1;
            }
        }
        if let Some(e) = self.entry_mut(index) {
            e.tx = Some(tx);
        }
    }

    /// 열린 트랜잭션을 닫는다(커밋·롤백·암묵·전환·끊김). 열린 것이 없으면 아무 일도 없다.
    pub fn close_tx(&mut self, outcome: TxOutcome, at: String) -> bool {
        let Some(id) = self.open_tx.take() else {
            return false;
        };
        if let Some(r) = self.records.iter_mut().find(|r| r.id == id) {
            r.outcome = outcome;
            r.ended = Some(at);
        }
        true
    }

    /// 열린 트랜잭션(있으면).
    #[must_use]
    pub fn open_record(&self) -> Option<&TxRecord> {
        let id = self.open_tx?;
        self.records.iter().find(|r| r.id == id)
    }

    #[must_use]
    pub fn record(&self, id: u64) -> Option<&TxRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    /// id로 문장 하나(우클릭 메뉴 · 복사/편집기로).
    #[must_use]
    pub fn entry(&self, id: u64) -> Option<&TxEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// 문장의 트랜잭션 결과(표시 시점에 읽는다).
    #[must_use]
    pub fn outcome_of(&self, e: &TxEntry) -> TxOutcome {
        match e.tx.and_then(|id| self.record(id)) {
            Some(r) => r.outcome.clone(),
            None => TxOutcome::Auto,
        }
    }

    /// 전체 문장 수.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 필터에 맞는 문장(최신 우선 · 복사 0).
    pub fn entries<'a>(&'a self, f: &'a TxFilter) -> impl Iterator<Item = &'a TxEntry> + 'a {
        let needle = f.text.trim().to_lowercase();
        self.entries.iter().rev().filter(move |e| {
            if !f.all_purposes && e.purpose != Purpose::User {
                return false;
            }
            if let Some(ed) = f.editor {
                if e.editor != ed {
                    return false;
                }
            }
            if !f.previous {
                let current = match e.tx {
                    Some(id) => Some(id) == self.open_tx,
                    None => true,
                };
                if !current {
                    return false;
                }
            }
            needle.is_empty() || e.text.to_lowercase().contains(&needle)
        })
    }

    /// 전부 지우기(호스트 "로그 비우기").
    pub fn clear(&mut self) {
        self.entries.clear();
        self.records.clear();
        self.current.clear();
        self.open_tx = None;
    }
}

/// 문장 한 줄 표시(줄바꿈 → `¶` · 연속 공백 접기 · `max` 글자).
#[must_use]
pub fn one_line(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in text.trim().chars() {
        let c = match ch {
            '\n' => '¶',
            '\r' => continue,
            '\t' | ' ' => {
                if last_space {
                    continue;
                }
                last_space = true;
                ' '
            }
            c => c,
        };
        if c != ' ' {
            last_space = false;
        }
        out.push(c);
        if out.chars().count() >= max {
            out.push('…');
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(n: u32) -> String {
        format!("2026-09-17 10:00:{n:02}.000")
    }

    /// 대기 → 롤백: 문장 행의 Tx 열이 표시 시점에 롤백으로 바뀐다(DBeaver에 없던 것) · 오류 문장은 트랜잭션 밖.
    #[test]
    fn rollback_marks_statements_of_the_closed_transaction() {
        let mut l = TxLog::new(100);
        l.begin_batch();
        l.begin(at(1), 7, Purpose::User, 0, "UPDATE t SET a=1");
        l.done(0, Some(1), Duration::from_millis(15));
        l.attach_tx(0, at(1), true);
        l.begin(at(2), 7, Purpose::User, 1, "SELECT * FROM t");
        l.result(1, 3, Duration::from_millis(9));
        l.attach_tx(1, at(2), false);
        l.begin(at(3), 7, Purpose::User, 2, "SELECT * FROM missing");
        l.error(2, Some(942), "table or view does not exist");
        let f = TxFilter {
            previous: true,
            ..TxFilter::default()
        };
        let rows: Vec<TxEntry> = l.entries(&f).cloned().collect();
        assert_eq!(rows.len(), 3, "최신 우선 3");
        assert_eq!(l.outcome_of(&rows[2]), TxOutcome::Pending);
        assert_eq!(l.open_record().map(|r| r.updates), Some(1));
        assert!(l.close_tx(TxOutcome::RolledBack, at(4)));
        assert_eq!(
            l.outcome_of(&rows[2]),
            TxOutcome::RolledBack,
            "UPDATE 행 = 롤백됨"
        );
        assert_eq!(
            l.outcome_of(&rows[1]),
            TxOutcome::RolledBack,
            "같은 트랜잭션의 조회도"
        );
        assert_eq!(
            l.outcome_of(&rows[0]),
            TxOutcome::Auto,
            "오류 문장은 트랜잭션 밖"
        );
        assert!(l.open_record().is_none());
        assert!(
            !l.close_tx(TxOutcome::Committed, at(5)),
            "열린 것이 없으면 false"
        );
        // 이전 트랜잭션 숨기기 = 닫힌 것은 빠지고 자동(오류)만.
        let cur_f = TxFilter::default();
        let cur: Vec<&TxEntry> = l.entries(&cur_f).collect();
        assert_eq!(cur.len(), 1);
        assert_eq!(cur[0].index, 2);
    }

    /// 검색·탭 필터·링 상한.
    #[test]
    fn filters_and_ring_cap() {
        let mut l = TxLog::new(16);
        for i in 0..40 {
            l.begin(
                at(i % 60),
                (i % 2) as u64,
                Purpose::User,
                i as usize,
                &format!("SELECT {i} FROM dual"),
            );
            l.result(i as usize, 1, Duration::from_millis(1));
        }
        assert_eq!(l.len(), 16, "링 상한");
        let f = TxFilter {
            text: "select 3".into(),
            previous: true,
            editor: Some(1),
            ..TxFilter::default()
        };
        let got: Vec<usize> = l.entries(&f).map(|e| e.index).collect();
        assert_eq!(got, vec![39, 37, 35, 33, 31]);
        assert_eq!(one_line("SELECT *\n  FROM   t\r\n", 9), "SELECT *¶…");
    }
}
