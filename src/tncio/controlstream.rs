//! Control streams
//!
//! This module exposes a `controlstream(AsyncRead)` method
//! which
//! 1. frames the TNC control port socket; and
//! 2. splits the output into events and command responses

use std::collections::vec_deque::VecDeque;
use std::marker::Unpin;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use futures::prelude::*;
use futures::task::Context;
use futures::task::Poll;

use crate::framing::control::TncControlFraming;
use crate::framing::framer::FramedRead;
use crate::protocol::response::{CommandResult, Event, Response};

/// Bind input streams to a TNC control socket
///
/// This method takes ownership of a TNC control socket's
/// read half and returns two futures `Stream`s: one for
/// `Event`s and one for `CommandResult`s.
///
/// # Parameters
/// - `control_in`: TNC control port (read half)
///
/// # Returns
/// 0. A `Stream` of asynchronous `Event`s from the TNC
/// 1. A `Stream` of good/bad responses to `Commands` sent
///    by the caller.
pub fn controlstream<R>(control_in: R) -> (ControlStreamEvents<R>, ControlStreamResults<R>)
where
    R: AsyncRead + Unpin,
{
    let state = ControlState {
        control_in: FramedRead::new(control_in, TncControlFraming::new()),
        event_queue: VecDeque::with_capacity(32),
        result_queue: VecDeque::with_capacity(32),
        fused: false,
    };

    let stateref = Arc::new(Mutex::new(state));

    let ctrl = ControlStreamEvents {
        state: stateref.clone(),
    };

    let results = ControlStreamResults { state: stateref };

    (ctrl, results)
}

// Holds state for the control streams
struct ControlState<R>
where
    R: AsyncRead + Unpin,
{
    control_in: FramedRead<R, TncControlFraming>,
    event_queue: VecDeque<Event>,
    result_queue: VecDeque<CommandResult>,
    fused: bool,
}

/// Stream of TNC Connection Events
pub struct ControlStreamEvents<R>
where
    R: AsyncRead + Unpin,
{
    state: Arc<Mutex<ControlState<R>>>,
}

/// Stream of TNC Command Results
pub struct ControlStreamResults<R>
where
    R: AsyncRead + Unpin,
{
    state: Arc<Mutex<ControlState<R>>>,
}

impl<R> Unpin for ControlStreamEvents<R> where R: AsyncRead + Unpin {}

impl<R> Unpin for ControlStreamResults<R> where R: AsyncRead + Unpin {}

impl<R> Stream for ControlStreamEvents<R>
where
    R: AsyncRead + Unpin,
{
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut state = (*self).state.lock().unwrap();

        loop {
            // anything in the Event queue?
            if let Some(out) = state.event_queue.pop_front() {
                return Poll::Ready(Some(out));
            }

            // none, so read from the stream
            match ready!(Pin::new(&mut *state).poll_next(cx)) {
                // EOF
                None => return Poll::Ready(None),

                // found something; the next loop will get it
                Some(()) => continue,
            }
        }
    }
}

impl<R> Stream for ControlStreamResults<R>
where
    R: AsyncRead + Unpin,
{
    type Item = CommandResult;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut state = (*self).state.lock().unwrap();

        loop {
            // anything in the Event queue?
            if let Some(out) = state.result_queue.pop_front() {
                return Poll::Ready(Some(out));
            }

            // none, so read from the stream
            match ready!(Pin::new(&mut *state).poll_next(cx)) {
                // EOF
                None => return Poll::Ready(None),

                // found something; the next loop will get it
                Some(()) => continue,
            }
        }
    }
}

impl<R> Stream for ControlState<R>
where
    R: AsyncRead + Unpin,
{
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if (*self).fused {
            return Poll::Ready(None);
        }

        match ready!(Pin::new(&mut (*self).control_in).poll_next(cx)) {
            None => {
                (*self).fused = true;
                Poll::Ready(None)
            }
            Some(Response::CommandResult(res)) => {
                (*self).result_queue.push_back(res);
                Poll::Ready(Some(()))
            }
            Some(Response::Event(evt)) => {
                (*self).event_queue.push_back(evt);
                Poll::Ready(Some(()))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    use futures::executor::ThreadPool;

    use crate::protocol::constants::CommandID;

    #[test]
    fn test_stream_events() {
        let curs = Cursor::new(b"MYCALL\rBUSY TRUE\rARQBW\r".to_vec());
        let (mut evt, mut res) = controlstream(curs);

        let mut exec = ThreadPool::new().expect("Failed to create threadpool");
        exec.run(async {
            let e1 = await!(evt.next()).unwrap();
            assert_eq!(Event::BUSY(true), e1);

            let r1 = await!(res.next()).unwrap();
            assert_eq!(Ok((CommandID::MYCALL, None)), r1);

            let r2 = await!(res.next()).unwrap();
            assert_eq!(Ok((CommandID::ARQBW, None)), r2);

            assert!(await!(res.next()).is_none());
            assert!(await!(evt.next()).is_none());
        });
    }
}
