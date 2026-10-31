//! ★ 저장 큐 `SaveQueue`([docs/70 §3](../../../docs/70-autosave-and-restore.md) · T-165 P3 ① · 10-31 · 30 §2 부품) — **틱 + 같은 열쇠 병합 +
//! 대기 재설정 + 최대 대기**. 의존 0 · 순수(시각은 인자 `now`) · 호스트는 무엇이든 열쇠(`K`)와 페이로드(`P`)로 쓴다.
//!
//! - `push`: 같은 열쇠가 있으면 페이로드를 **바꾸고 `due`만 뒤로**(`now + delay` · trailing debounce) · 처음 온 시각 + `max_wait` =
//!   `deadline`은 그대로(계속 요청이 와도 `max_wait`마다 한 번은 수행 = throttle 상한 · "0.5초 간격으로 2시간 저장 버튼" 사용자 09-22).
//! - `take_due`: `now >= min(due, deadline)`인 것만 **들어온 순서대로** 꺼낸다(한 틱에 필요한 저장만) · `take_due_limit`은 최대 N개(프레임 예산).
//! - `flush`: 전부(종료 · 전환) · `cancel`: 열쇠를 버린다(저장했다 · 닫았다) · `next_due`: 가장 이른 깨움 시각(잠자기 친화 · 39 §2).
//! - 페이로드는 본문이 아니라 "무엇을 쓸지"(경로 · 세대) — 실제 본문은 수행 시점에 한 번만 뜬다.
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Pending<K, P> {
    key: K,
    payload: P,
    due: Instant,
    deadline: Instant,
}

/// 열쇠당 최대 1개.
#[derive(Debug)]
pub struct SaveQueue<K, P> {
    items: Vec<Pending<K, P>>,
}

impl<K: PartialEq, P> Default for SaveQueue<K, P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: PartialEq, P> SaveQueue<K, P> {
    #[must_use]
    pub fn new() -> Self {
        SaveQueue { items: Vec::new() }
    }

    /// 같은 열쇠가 있으면 페이로드 교체 + `due = now + delay`(`deadline` 유지) · 없으면 새로(`deadline = now + max_wait`).
    pub fn push(&mut self, key: K, payload: P, now: Instant, delay: Duration, max_wait: Duration) {
        if let Some(it) = self.items.iter_mut().find(|it| it.key == key) {
            it.payload = payload;
            it.due = now + delay;
            return;
        }
        self.items.push(Pending {
            key,
            payload,
            due: now + delay,
            deadline: now + max_wait,
        });
    }

    fn is_due(it: &Pending<K, P>, now: Instant) -> bool {
        now >= it.due || now >= it.deadline
    }

    /// 만료된 것 전부(들어온 순서).
    pub fn take_due(&mut self, now: Instant) -> Vec<(K, P)> {
        self.take_due_limit(now, usize::MAX)
    }

    /// 만료된 것 중 앞에서 최대 `max`개(나머지는 남는다 · 다음 틱).
    pub fn take_due_limit(&mut self, now: Instant, max: usize) -> Vec<(K, P)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.items.len() && out.len() < max {
            if Self::is_due(&self.items[i], now) {
                let it = self.items.remove(i);
                out.push((it.key, it.payload));
            } else {
                i += 1;
            }
        }
        out
    }

    /// 전부 꺼낸다(종료 · 프로젝트 닫기 · 전환).
    pub fn flush(&mut self) -> Vec<(K, P)> {
        self.items
            .drain(..)
            .map(|it| (it.key, it.payload))
            .collect()
    }

    /// 열쇠를 버린다 — 있었으면 true.
    pub fn cancel(&mut self, key: &K) -> bool {
        let n = self.items.len();
        self.items.retain(|it| it.key != *key);
        self.items.len() != n
    }

    /// 다음 틱이 필요한 가장 이른 시각(없으면 None — 매 프레임 돌지 않는다).
    #[must_use]
    pub fn next_due(&self) -> Option<Instant> {
        self.items.iter().map(|it| it.due.min(it.deadline)).min()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn contains(&self, key: &K) -> bool {
        self.items.iter().any(|it| it.key == *key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    /// MC/DC(70 §3): 같은 열쇠 재요청 = 페이로드 교체 + due 재설정(deadline 유지) · 다른 열쇠 = 따로 · delay 만료 · 계속 재요청해도
    /// deadline에 한 번은 수행 · cancel 뒤 없음 · flush = 전부 · take_due_limit = 앞에서 N개만 · next_due = 가장 이른 min(due, deadline).
    #[test]
    fn debounce_with_deadline_mcdc() {
        let t0 = Instant::now();
        let mut q: SaveQueue<u32, &str> = SaveQueue::new();
        assert!(q.is_empty() && q.next_due().is_none());
        q.push(1, "a1", t0, S(1), S(5));
        assert_eq!(q.next_due(), Some(t0 + S(1)));
        // delay 전 = 없음 · 재요청 = due가 밀린다(페이로드는 마지막 것).
        assert!(q.take_due(t0 + Duration::from_millis(900)).is_empty());
        q.push(1, "a2", t0 + Duration::from_millis(900), S(1), S(5));
        assert!(
            q.take_due(t0 + S(1)).is_empty(),
            "재요청으로 due가 1.9s로 밀림"
        );
        assert_eq!(
            q.take_due(t0 + Duration::from_millis(1900)),
            vec![(1, "a2")]
        );
        assert!(q.is_empty());
        // 계속 재요청(0.5s마다) → deadline(first + 5s)에 한 번은 나온다.
        let mut now = t0 + S(10);
        let first = now;
        q.push(2, "b", now, S(1), S(5));
        while now + Duration::from_millis(500) < first + S(5) {
            now += Duration::from_millis(500);
            assert!(
                q.take_due(now).is_empty(),
                "deadline 전에는 나오지 않음: {:?}",
                now - first
            );
            q.push(2, "b", now, S(1), S(5));
        }
        assert_eq!(
            q.next_due(),
            Some(first + S(5)),
            "due는 밀렸지만 deadline이 먼저"
        );
        assert_eq!(q.take_due(first + S(5)), vec![(2, "b")]);
        now = first + S(5);
        // 다른 열쇠는 따로 · cancel · flush · limit.
        q.push(3, "c", now, S(1), S(5));
        q.push(4, "d", now, S(1), S(5));
        q.push(5, "e", now, S(2), S(5));
        assert_eq!(q.len(), 3);
        assert!(q.cancel(&4));
        assert!(!q.cancel(&4));
        assert!(!q.contains(&4) && q.contains(&3));
        assert_eq!(q.next_due(), Some(now + S(1)));
        assert_eq!(
            q.take_due_limit(now + S(3), 1),
            vec![(3, "c")],
            "앞에서 1개만"
        );
        assert_eq!(q.len(), 1);
        q.push(6, "f", now, S(9), S(60));
        let all = q.flush();
        assert_eq!(all, vec![(5, "e"), (6, "f")]);
        assert!(q.is_empty());
    }
}
