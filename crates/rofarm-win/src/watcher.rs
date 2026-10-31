use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub struct AdaptivePoller {
    idle_ms: u64,
    active_ms: u64,
    current_ms: AtomicU32,
    idle_ticks: AtomicU32,
}

impl AdaptivePoller {
    pub fn new(idle: Duration, active: Duration) -> Self {
        let idle_ms = idle.as_millis().max(100) as u64;
        Self {
            idle_ms,
            active_ms: active.as_millis().max(50) as u64,
            current_ms: AtomicU32::new(active.as_millis().max(50) as u32),
            idle_ticks: AtomicU32::new(0),
        }
    }

    pub fn interval(&self) -> Duration {
        Duration::from_millis(self.current_ms.load(Ordering::Relaxed) as u64)
    }

    pub fn observe(&self, changed: bool) {
        if changed {
            self.idle_ticks.store(0, Ordering::Relaxed);
            self.current_ms.store(self.active_ms as u32, Ordering::Relaxed);
        } else {
            let ticks = self.idle_ticks.fetch_add(1, Ordering::Relaxed) + 1;
            if ticks >= 10 {
                let next = (self.current_ms.load(Ordering::Relaxed) as u64 * 2).min(self.idle_ms);
                self.current_ms.store(next as u32, Ordering::Relaxed);
                self.idle_ticks.store(0, Ordering::Relaxed);
            }
        }
    }
}

pub fn watch_once(pids: &[u32], running: impl Fn(u32) -> bool, mut on_exit: impl FnMut(u32)) {
    for pid in pids {
        if !running(*pid) {
            on_exit(*pid);
        }
    }
}

pub type SharedPoller = Arc<AdaptivePoller>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poller_backs_off_when_idle_and_speeds_up_on_change() {
        let poller = AdaptivePoller::new(Duration::from_secs(5), Duration::from_millis(400));
        assert_eq!(poller.interval(), Duration::from_millis(400));
        for _ in 0..20 {
            poller.observe(false);
        }
        assert!(poller.interval() > Duration::from_millis(400));
        poller.observe(true);
        assert_eq!(poller.interval(), Duration::from_millis(400));
    }

    #[test]
    fn exits_are_reported() {
        let mut exited = Vec::new();
        watch_once(&[1, 2, 3], |pid| pid != 2, |pid| exited.push(pid));
        assert_eq!(exited, vec![2]);
    }
}
