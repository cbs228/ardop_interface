//! Clear/busy channel detection
//!
//! While the ARDOP TNC has a free/busy channel detector, the
//! timing of when to submit a new outgoing transmission
//! (FEC or ARQ) is up to the client. Clients must tightly time
//! free / busy events (`BUSY FALSE` / `BUSY TRUE`) and wait
//! "long enough" for a clear channel.
//!
//! This method provides a task which will grab a mutex whenever
//! the channel is busy. The mutex is dropped whenever the channel
//! has become clear for long enough. "Long enough" is controlled
//! by the caller.

use std::sync::Arc;
use std::time::Duration;

use futures::prelude::*;

use futures::channel::oneshot;
use futures::lock::{Mutex, MutexGuard};
use futures::stream::Stream;

use async_timer::Timed;

use crate::protocol::response::Event;

/// Busy/clear channel detector task
///
/// This method should be run as a task. It will run until the `src`
/// stream is exhausted. TNC `BUSY` events will cause this task to
/// take the `busy_lock` mutex and hold it until the channel becomes
/// clear for at least `clear_time`. Other events will be passed
/// through to the `dst` queue immediately.
///
/// When `busy_lock` is held by this method, the channel is busy. When
/// `busy_lock` is not held, the channel is clear. Callers *MUST AVOID*
/// holding on to the `busy_lock` for too long, as doing this may block
/// additional events from being processed.
///
/// # Parameters
/// - `src`: Stream source of events
/// - `dst`: Non-busy events will be forwarded here
/// - `busy_lock`: A futures-aware mutex for busy channel indication
/// - `clear_time`: Minimum time that channel must be clear. If zero,
///   the channel will be declared clear immediately on receipt of
///   a `BUSY FALSE` event.
/// - `run_notify`: If given, will write a single empty value to
///   this channel when the first Event is processed from `src`.
///   This can be used to synchronize the startup of this task,
///   which is mainly useful for tests.
#[allow(unused_variables)]
#[allow(unused_assignments)]
pub async fn busy_lock_task<S, K>(
    mut src: S,
    mut dst: K,
    busy_lock: Arc<Mutex<()>>,
    clear_time: Duration,
    mut run_notify: Option<oneshot::Sender<()>>,
) where
    S: Stream<Item = Event> + Unpin,
    K: Sink<Event> + Unpin,
{
    // take the lock on startup
    let mut busy_when_held: Option<MutexGuard<()>> = Some(busy_lock.lock().await);
    let mut will_be_clear = true;

    // if the clear time is zero, we will not timeout while
    // waiting for events
    let immediately_clear = clear_time == Duration::from_secs(0);
    if immediately_clear {
        busy_when_held = None;
    }

    loop {
        // if clear_time is zero, or we are already clear, don't timeout
        let res = if immediately_clear || busy_when_held.is_none() {
            Ok(src.next().await)
        } else {
            Timed::platform_new(src.next(), clear_time.clone()).await
        };

        match res {
            Ok(Some(Event::BUSY(true))) => {
                // grab the lock, if we don't have it
                if busy_when_held.is_none() {
                    busy_when_held = Some(busy_lock.lock().await);
                }
                will_be_clear = false;
            }
            Ok(Some(Event::BUSY(false))) => {
                // we will be clear on next timeout
                will_be_clear = true;
                if immediately_clear {
                    // channel declared clear immediately
                    busy_when_held = None;
                }
            }
            Ok(Some(evt)) => {
                // Other events go to dst. Send errors cause
                // task termination.
                match dst.send(evt).await {
                    Ok(()) => { /* no-op */ }
                    Err(_e) => break,
                }
            }
            Err(_timeout) => {
                if will_be_clear {
                    // drop the lock
                    busy_when_held = None;
                }
            }
            Ok(None) => {
                // stream exhausted -> terminate
                break;
            }
        }

        // we are now running
        if run_notify.is_some() {
            run_notify.take().unwrap().send(()).unwrap();
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use futures::channel::mpsc;
    use futures::executor;
    use futures::executor::ThreadPool;
    use futures::task::SpawnExt;

    #[test]
    fn test_exit() {
        let busy = Arc::new(Mutex::new(()));
        let (mut evt_in_snd, evt_in_rx) = mpsc::unbounded();
        let (evt_out_snd, _evt_out_rx) = mpsc::unbounded();
        let (start_snd, start_rx) = oneshot::channel();

        let mut pool = ThreadPool::new().unwrap();
        pool.spawn(busy_lock_task(
            evt_in_rx,
            evt_out_snd,
            busy.clone(),
            Duration::from_secs(120),
            Some(start_snd),
        ))
        .unwrap();

        executor::block_on(async {
            // send the first event (we're busy)
            evt_in_snd.send(Event::BUSY(true)).await.unwrap();

            // once running...
            let _ = start_rx.await.unwrap();
            // ... we can't get the lock
            assert!(busy.try_lock().is_none());

            // close channel -> kills task
            evt_in_snd.close_channel();

            // and now we can get the lock
            busy.lock().await;
        });
    }

    #[test]
    fn test_becomes_clear() {
        let busy = Arc::new(Mutex::new(()));
        let (mut evt_in_snd, evt_in_rx) = mpsc::unbounded();
        let (evt_out_snd, _evt_out_rx) = mpsc::unbounded();
        let (start_snd, start_rx) = oneshot::channel();

        let mut pool = ThreadPool::new().unwrap();
        pool.spawn(busy_lock_task(
            evt_in_rx,
            evt_out_snd,
            busy.clone(),
            Duration::from_millis(2),
            Some(start_snd),
        ))
        .unwrap();

        executor::block_on(async {
            // initially busy
            evt_in_snd.send(Event::BUSY(true)).await.unwrap();
            let _ = start_rx.await.unwrap();

            // becomes clear
            evt_in_snd.send(Event::BUSY(false)).await.unwrap();

            // and now we can get the lock, eventually
            busy.lock().await;

            // close channel -> kills task
            evt_in_snd.close_channel();
        });
    }
}
