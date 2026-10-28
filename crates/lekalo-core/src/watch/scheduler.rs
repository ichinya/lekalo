//! Pure bounded scheduler: one active run, one pending dirty union,
//! debounce/max-wait, and explicit teardown before replacement. Clock
//! values are monotonic milliseconds injected by the host or tests.
use super::request::{limits, WatchRequest};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Decision {
    None,
    CancelStale {
        run_id: u64,
        superseded_by_generation: u64,
    },
    Start {
        run_id: u64,
        generation: u64,
        paths: BTreeSet<String>,
        full_rescan: bool,
    },
}
#[derive(Debug)]
struct Active {
    id: u64,
    generation: u64,
    paths: BTreeSet<String>,
    full: bool,
    stale: bool,
    cancel_requested: bool,
}
#[derive(Debug)]
pub struct Scheduler {
    debounce: u64,
    max_wait: u64,
    generation: u64,
    next_run: u64,
    dirty: BTreeSet<String>,
    first: Option<u64>,
    last: Option<u64>,
    active: Option<Active>,
    full: bool,
}
impl Scheduler {
    pub fn new(request: &WatchRequest) -> Self {
        Self {
            debounce: request.debounce_ms,
            max_wait: request.max_wait_ms,
            generation: 0,
            next_run: 1,
            dirty: BTreeSet::new(),
            first: None,
            last: None,
            active: None,
            full: false,
        }
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn active_generation(&self) -> Option<u64> {
        self.active.as_ref().map(|a| a.generation)
    }
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }
    pub fn active_run(&self) -> Option<u64> {
        self.active.as_ref().map(|a| a.id)
    }
    pub fn note_path(&mut self, path: &str, now: u64) -> bool {
        self.generation = self.generation.saturating_add(1);
        if !self.full {
            if self.dirty.len() >= limits::MAX_PENDING_PATHS && !self.dirty.contains(path) {
                self.full = true;
                self.dirty.clear();
            } else {
                self.dirty.insert(path.to_owned());
            }
        }
        self.first.get_or_insert(now);
        self.last = Some(now);
        if let Some(active) = &mut self.active {
            active.stale = true;
        }
        self.active.is_some()
    }
    pub fn require_full_rescan(&mut self, now: u64) {
        self.note_path("lekalo", now);
        self.full = true;
        self.dirty.clear();
    }
    pub fn baseline_started(&mut self, id: u64) {
        assert!(self.active.is_none());
        self.next_run = id + 1;
        self.active = Some(Active {
            id,
            generation: self.generation,
            paths: BTreeSet::new(),
            full: true,
            stale: false,
            cancel_requested: false,
        });
    }
    pub fn poll(&mut self, now: u64) -> Decision {
        if let Some(active) = &mut self.active {
            if active.stale && !active.cancel_requested {
                active.cancel_requested = true;
                return Decision::CancelStale {
                    run_id: active.id,
                    superseded_by_generation: self.generation,
                };
            }
            return Decision::None;
        }
        let (Some(first), Some(last)) = (self.first, self.last) else {
            return Decision::None;
        };
        if now.saturating_sub(last) < self.debounce && now.saturating_sub(first) < self.max_wait {
            return Decision::None;
        }
        let paths = std::mem::take(&mut self.dirty);
        let full = std::mem::take(&mut self.full);
        let id = self.next_run;
        self.next_run += 1;
        self.first = None;
        self.last = None;
        self.active = Some(Active {
            id,
            generation: self.generation,
            paths: paths.clone(),
            full,
            stale: false,
            cancel_requested: false,
        });
        Decision::Start {
            run_id: id,
            generation: self.generation,
            paths,
            full_rescan: full,
        }
    }
    /// Only the host's positive proof of completion/teardown unlocks
    /// another start. Dirty inputs of a stale run survive cancellation.
    pub fn run_finished(&mut self, id: u64) {
        if self.active.as_ref().is_none_or(|a| a.id != id) {
            return;
        }
        let active = self.active.take().expect("active matched");
        if active.stale {
            self.full |= active.full;
            if !self.full {
                self.dirty.extend(active.paths);
                if self.dirty.len() > limits::MAX_PENDING_PATHS {
                    self.dirty.clear();
                    self.full = true;
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn scheduler() -> Scheduler {
        Scheduler::new(&WatchRequest::default())
    }
    #[test]
    fn rapid_edits_wait_for_quiet_then_publish_latest_generation() {
        let mut s = scheduler();
        for t in [0, 40, 80, 120] {
            s.note_path("lekalo/modules/planner/entities.yaml", t);
        }
        assert_eq!(s.poll(269), Decision::None);
        let Decision::Start {
            generation, paths, ..
        } = s.poll(270)
        else {
            panic!("quiet run");
        };
        assert_eq!(generation, 4);
        assert_eq!(paths.len(), 1);
    }
    #[test]
    fn max_wait_prevents_starvation() {
        let mut s = scheduler();
        for t in [0, 100, 300, 600, 900] {
            s.note_path("x", t);
        }
        assert!(matches!(s.poll(1000), Decision::Start { .. }));
    }
    #[test]
    fn cancellation_preserves_union_and_blocks_until_teardown() {
        let mut s = scheduler();
        s.note_path("a", 0);
        assert!(matches!(s.poll(150), Decision::Start { run_id: 1, .. }));
        s.note_path("b", 200);
        assert!(matches!(
            s.poll(201),
            Decision::CancelStale {
                run_id: 1,
                superseded_by_generation: 2
            }
        ));
        assert_eq!(s.poll(1000), Decision::None);
        s.run_finished(1);
        let Decision::Start { paths, .. } = s.poll(1001) else {
            panic!("replacement");
        };
        assert_eq!(paths, ["a".into(), "b".into()].into());
    }
    #[test]
    fn overflow_is_bounded_and_explicit() {
        let mut s = scheduler();
        for i in 0..4097 {
            s.note_path(&format!("p{i}"), 0);
        }
        let Decision::Start {
            paths, full_rescan, ..
        } = s.poll(150)
        else {
            panic!("run");
        };
        assert!(paths.is_empty());
        assert!(full_rescan);
    }
}
