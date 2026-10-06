//! Lets a worker sleep until any of its producers has queued work.
//!
//! Each worker owns one doorbell and still receives on its ordinary channels.
//! Producers send first and ring second; the worker drains every channel after
//! each wake, so a ring that lands between draining and waiting is never lost.
use std::{
    sync::{Arc, Condvar, Mutex, PoisonError},
    time::Instant,
};

#[derive(Clone, Default)]
pub struct Doorbell(Arc<(Mutex<bool>, Condvar)>);

impl Doorbell {
    pub fn ring(&self) {
        let (rung, wake) = &*self.0;
        *rung.lock().unwrap_or_else(PoisonError::into_inner) = true;
        wake.notify_all();
    }

    /// Sleeps until rung or until `deadline` (forever without one), then
    /// clears the bell. Returns false when the deadline passed unrung.
    pub fn wait(&self, deadline: Option<Instant>) -> bool {
        let (rung, wake) = &*self.0;
        let mut guard = rung.lock().unwrap_or_else(PoisonError::into_inner);
        while !*guard {
            guard = match deadline {
                None => wake.wait(guard).unwrap_or_else(PoisonError::into_inner),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return false;
                    }
                    wake.wait_timeout(guard, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
        *guard = false;
        true
    }
}

/// Rings when dropped. As the last field of a handle it fires after the
/// handle's channel senders are gone, so the woken worker sees the disconnect.
#[derive(Default)]
pub struct RingOnDrop(pub Doorbell);

impl Drop for RingOnDrop {
    fn drop(&mut self) {
        self.0.ring();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    #[test]
    fn rings_are_kept_until_waited_and_deadlines_end_quiet_waits() {
        let bell = Doorbell::default();
        bell.ring();
        bell.ring();
        assert!(bell.wait(None), "a ring before waiting is not lost");
        let start = Instant::now();
        assert!(!bell.wait(Some(start + Duration::from_millis(30))));
        assert!(start.elapsed() >= Duration::from_millis(30));
        assert!(!bell.wait(Some(start)), "a past deadline returns at once");
    }

    #[test]
    fn another_thread_wakes_a_waiter_and_drop_rings_last() {
        let bell = Doorbell::default();
        let ringer = bell.clone();
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            ringer.ring();
        });
        assert!(bell.wait(Some(Instant::now() + Duration::from_secs(10))));
        worker.join().unwrap();

        struct Handle {
            _sender: std::sync::mpsc::Sender<()>,
            _bell: RingOnDrop,
        }
        let (sender, receiver) = std::sync::mpsc::channel::<()>();
        let handle = Handle {
            _sender: sender,
            _bell: RingOnDrop(bell.clone()),
        };
        drop(handle);
        assert!(bell.wait(None));
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        ));
    }
}
