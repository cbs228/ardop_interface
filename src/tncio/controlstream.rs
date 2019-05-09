//! Control streams
//!
//! This module exposes a `controlstream(AsyncRead)` method
//! which
//! 1. frames the TNC control port socket; and
//! 2. splits the output into events and command responses

use std::collections::vec_deque::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};

use tokio::codec::FramedRead;
use tokio::prelude::*;

use crate::framing::control::TncControlFraming;
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
    R: AsyncRead,
{
    let state = ControlState {
        control_in: FramedRead::new(control_in, TncControlFraming::new()),
        event_queue: VecDeque::with_capacity(32),
        result_queue: VecDeque::with_capacity(32),
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
    R: AsyncRead,
{
    control_in: FramedRead<R, TncControlFraming>,
    event_queue: VecDeque<Event>,
    result_queue: VecDeque<CommandResult>,
}

/// Stream of TNC Connection Events
pub struct ControlStreamEvents<R>
where
    R: AsyncRead,
{
    state: Arc<Mutex<ControlState<R>>>,
}

/// Stream of TNC Command Results
pub struct ControlStreamResults<R>
where
    R: AsyncRead,
{
    state: Arc<Mutex<ControlState<R>>>,
}

impl<R> Stream for ControlStreamEvents<R>
where
    R: AsyncRead,
{
    type Item = Event;
    type Error = io::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        let mut state = self.state.lock().unwrap();

        loop {
            // Any in the Events queue?
            if let Some(out) = state.event_queue.pop_front() {
                return Ok(Async::Ready(Some(out)));
            }

            // None, so try and fetch from the stream. Errors will propagate.
            match state.poll_next()? {
                // must block
                Async::NotReady => return Ok(Async::NotReady),

                // EOF
                Async::Ready(None) => return Ok(Async::Ready(None)),

                // Something read -- keep going
                Async::Ready(Some(())) => continue,
            }
        }
    }
}

impl<R> Stream for ControlStreamResults<R>
where
    R: AsyncRead,
{
    type Item = CommandResult;
    type Error = io::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        let mut state = self.state.lock().unwrap();

        loop {
            // Any in the Results queue?
            if let Some(out) = state.result_queue.pop_front() {
                return Ok(Async::Ready(Some(out)));
            }

            // None, so try and fetch from the stream. Errors will propagate.
            match state.poll_next()? {
                // must block
                Async::NotReady => return Ok(Async::NotReady),

                // EOF
                Async::Ready(None) => return Ok(Async::Ready(None)),

                // Something read -- keep going
                Async::Ready(Some(())) => continue,
            }
        }
    }
}

impl<R> ControlState<R>
where
    R: AsyncRead,
{
    // Polls for next item in the incoming stream
    pub fn poll_next(&mut self) -> Result<Async<Option<()>>, io::Error> {
        match self.control_in.poll()? {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(None) => Ok(Async::Ready(None)),
            Async::Ready(Some(Response::CommandResult(res))) => {
                // enqueue it
                self.result_queue.push_back(res);
                Ok(Async::Ready(Some(())))
            }
            Async::Ready(Some(Response::Event(evt))) => {
                // enqueue it
                self.event_queue.push_back(evt);
                Ok(Async::Ready(Some(())))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    use crate::protocol::constants::CommandID;

    #[test]
    fn test_stream_events() {
        let curs = Cursor::new(b"MYCALL\rBUSY TRUE\rARQBW\r".to_vec());
        let (evt, res) = controlstream(curs);

        let mut res_waiter = res.wait();
        let mut evt_waiter = evt.wait();

        let e1 = evt_waiter.next().unwrap().unwrap();
        assert_eq!(Event::BUSY(true), e1);

        let r1 = res_waiter.next().unwrap().unwrap();
        assert_eq!(Ok((CommandID::MYCALL, None)), r1);

        let r2 = res_waiter.next().unwrap().unwrap();
        assert_eq!(Ok((CommandID::ARQBW, None)), r2);

        assert!(res_waiter.next().is_none());
    }
}
