//! Busy channel waiting
//!
//! ARDOP provides busy channel detection which will prevent
//! outgoing transmissions when `BUSY TRUE` is received. At
//! present, tested implementations won't always do this,
//! and they might not buffer transmissions for later sending
//! at that.
//!
//! Implement a thread-safe mechanism for blocking until
//! `BUSY FALSE` is received. The client thread should use
//! `BusyLockReceive::wait_clear_for()` to wait for a clear
//! channel.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Create new busy lock send/receive pair
///
/// The busy lock is used to provide inter-thread notifications
/// of the channel busy state. The sender is intended to be
/// paired with an `EventHandler`. The receiver may be cloned to
/// any number of threads which are interested in the busy state.
///
/// # Returns
/// 0. The sending side of the busy channel notifier
/// 1. The receiving side of the busy channel notifier
pub fn new() -> (Arc<BusyLock>, Arc<BusyLock>) {
    let bls = BusyLock::new();
    let bls2 = bls.clone();
    (bls, bls2)
}

/// Waits for a busy channel to become clear
#[derive(Debug)]
pub struct BusyLock {
    // busy true/false and time of last state transition
    busy: Mutex<(bool, Instant)>,
    notify: Condvar,
}

impl BusyLock {
    fn new() -> Arc<BusyLock> {
        Arc::new(BusyLock {
            busy: Mutex::new((false, Instant::now())),
            notify: Condvar::new(),
        })
    }

    /// Sets the current busy state
    ///
    /// Updates the current busy/clear state from the TNC.
    /// For use only by the I/O thread.
    ///
    /// # Parameters
    /// - `busy`: True if channel is busy, or false if it is
    ///   clear
    pub fn set_busy(&self, is_busy: bool) {
        let mut busy = self.busy.lock().unwrap();
        *busy = (is_busy, Instant::now());

        // notify any state transition
        self.notify.notify_all();
    }

    /// Waits for a clear channel
    ///
    /// Waits until the TNC has declared the channel to be clear
    /// (i.e., `BUSY FALSE`) for a time interval of at least
    /// `clear`. This method *may* block for up to (about) `timeout`
    /// seconds and will return false if no clear channel is detected
    /// during that time frame.
    ///
    /// # Parameters
    /// - `clear`: Required *contiguous* duration during which
    ///   the channel must be clear
    /// - `timeout`: Total time to wait before giving up
    /// - `poll`: Poll for a clear channel at this interval.
    ///   For best results, `poll` < `timeout`.
    ///
    /// # Return
    /// `true` if the channel has become clear for at least `clear`
    /// time units, or `false` if the channel did not become clear
    /// during the specified `timeout` period.
    pub fn wait_clear_for(&self, clear: Duration, timeout: Duration, poll: Duration) -> bool {
        // try an immediate check first
        {
            let mtx = self.busy.lock().unwrap();
            let busyat = *mtx;
            if !busyat.0 && busyat.1.elapsed() >= clear {
                // have been clear for required period of time
                return true;
            }
        }

        // failed, so we must wait
        let start = Instant::now();
        loop {
            let result = self
                .notify
                .wait_timeout(self.busy.lock().unwrap(), poll)
                .unwrap();

            let busyat = *result.0;
            if !busyat.0 && busyat.1.elapsed() >= clear {
                return true;
            }

            // timeout elapsed with negative result
            if start.elapsed() > timeout {
                return false;
            }
        }
    }

    /// Gets the current busy state
    ///
    /// Atomically obtains the current busy state. To wait on
    /// a clear channel, use `BusyLock::wait()` instead.
    pub fn busy(&self) -> bool {
        (*self.busy.lock().unwrap()).0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::thread;

    #[test]
    fn test_wait_clear_for() {
        let bl1 = BusyLock::new();
        let bl2 = bl1.clone();

        // times out
        bl1.set_busy(true);
        let t2 = thread::spawn(move || {
            let res = bl2.wait_clear_for(
                Duration::from_millis(0),
                Duration::from_millis(1),
                Duration::from_millis(1),
            );
            assert_eq!(res, false);
        });
        t2.join().unwrap();

        // becomes clear before timeout
        let bl3 = bl1.clone();
        let t2 = thread::spawn(move || {
            let res = bl3.wait_clear_for(
                Duration::from_millis(0),
                Duration::from_millis(1000),
                Duration::from_millis(20),
            );
            assert_eq!(res, true);
        });
        bl1.set_busy(false);
        t2.join().unwrap();

        // clear, but not for long enough
        bl1.set_busy(false);
        let bl4 = bl1.clone();
        let t2 = thread::spawn(move || {
            let res = bl4.wait_clear_for(
                Duration::from_millis(100000),
                Duration::from_millis(1),
                Duration::from_millis(20),
            );
            assert_eq!(res, false);
        });
        t2.join().unwrap();

        // clear right away
        let bl5 = bl1.clone();
        let t2 = thread::spawn(move || {
            let res = bl5.wait_clear_for(
                Duration::from_millis(0),
                Duration::from_millis(0),
                Duration::from_millis(1),
            );
            assert_eq!(res, true);
        });
        t2.join().unwrap();
    }
}
