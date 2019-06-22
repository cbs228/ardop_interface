//! Control streams
//!
//! This module exposes a `controlstream(AsyncRead)` method
//! which
//! 1. frames the TNC control port socket; and
//! 2. splits the output into events and command responses,
//!    and accepts Commands for transmission to the socket

use std::collections::vec_deque::VecDeque;
use std::io;
use std::marker::Unpin;
use std::pin::Pin;
use std::sync::Arc;

use futures::lock::Mutex;
use futures::prelude::*;
use futures::task::Context;
use futures::task::Poll;

use crate::framer::Framed;
use crate::framing::control::TncControlFraming;
use crate::protocol::response::{CommandResult, Event, Response};

/// Bind input streams to a TNC control socket
///
/// This method takes ownership of a TNC control socket's
/// read half and returns two futures `Stream`s: one for
/// `Event`s and one for `CommandResult`s.
///
/// # Parameters
/// - `control`: TNC control port (io type)
///
/// # Returns
/// 0. A `Stream` of asynchronous `Event`s from the TNC
/// 1. A `Stream` of good/bad responses to `Command`s sent
/// 2. A `Sink` which will accept stringified `Command`s for
///    transmission
pub fn controlstream<I>(
    control: I,
) -> (
    ControlStreamEvents<I>,
    ControlStreamResults<I>,
    ControlSink<I>,
)
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    let state = ControlState {
        io: Framed::new(control, TncControlFraming::new()),
        event_queue: VecDeque::with_capacity(32),
        result_queue: VecDeque::with_capacity(32),
        fused: false,
    };

    let stateref = Arc::new(Mutex::new(state));

    let ctrl = ControlStreamEvents {
        state: stateref.clone(),
    };

    let results = ControlStreamResults {
        state: stateref.clone(),
    };

    let sink = ControlSink {
        state: stateref,
        outqueue: VecDeque::with_capacity(32),
    };

    (ctrl, results, sink)
}

// Holds state for the control streams
struct ControlState<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    io: Framed<I, TncControlFraming>,
    event_queue: VecDeque<Event>,
    result_queue: VecDeque<CommandResult>,
    fused: bool,
}

/// Stream of TNC Connection Events
pub struct ControlStreamEvents<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    state: Arc<Mutex<ControlState<I>>>,
}

/// Stream of TNC Command Results
pub struct ControlStreamResults<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    state: Arc<Mutex<ControlState<I>>>,
}

/// Sink for outgoing TNC commands
pub struct ControlSink<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    state: Arc<Mutex<ControlState<I>>>,
    outqueue: VecDeque<String>,
}

impl<I> Unpin for ControlStreamEvents<I> where I: AsyncRead + AsyncWrite + Unpin {}

impl<I> Unpin for ControlStreamResults<I> where I: AsyncRead + AsyncWrite + Unpin {}

impl<I> Unpin for ControlSink<I> where I: AsyncRead + AsyncWrite + Unpin {}

impl<I> Stream for ControlStreamEvents<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    type Item = Event;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut state = ready!(Pin::new(&mut (*self).state.lock()).poll(cx));

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

impl<I> Stream for ControlStreamResults<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    type Item = CommandResult;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut state = ready!(Pin::new(&mut (*self).state.lock()).poll(cx));

        loop {
            // anything in the Result queue?
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

impl<I> Stream for ControlState<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if (*self).fused {
            return Poll::Ready(None);
        }

        match ready!(Pin::new(&mut (*self).io).poll_next(cx)) {
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

impl<I> Sink<String> for ControlSink<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    type SinkError = io::Error;

    fn poll_ready(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::SinkError>> {
        // we are ready if the outgoing I/O is ready
        let mut state = ready!(Pin::new(&mut (*self).state.lock()).poll(cx));
        Pin::new(&mut state.io).poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: String) -> Result<(), Self::SinkError> {
        // accept into internal buffer
        Ok((*self).outqueue.push_back(item))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::SinkError>> {
        let this = &mut (*self);
        let state_mutex = &mut this.state;
        let outqueue = &mut this.outqueue;

        let mut state = ready!(Pin::new(&mut state_mutex.lock()).poll(cx));

        // foreach item in the queue, try to submit it to the stream
        // every call to start_send() needs a call to poll_ready()
        while !outqueue.is_empty() {
            ready!(Pin::new(&mut state.io).poll_ready(cx))?;
            Pin::new(&mut state.io).start_send(outqueue.pop_front().unwrap())?;
        }
        Pin::new(&mut state.io).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::SinkError>> {
        let mut state = ready!(Pin::new(&mut (*self).state.lock()).poll(cx));
        Pin::new(&mut state.io).poll_close(cx)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    use futures::executor;

    use crate::protocol::constants::CommandID;

    #[test]
    fn test_streams() {
        let curs = Cursor::new(b"MYCALL\rBUSY TRUE\rARQBW\r".to_vec());
        let (mut evt, mut res, _out) = controlstream(curs);

        executor::block_on(async {
            let e1 = evt.next().await.unwrap();
            assert_eq!(Event::BUSY(true), e1);

            let r1 = res.next().await.unwrap();
            assert_eq!(Ok((CommandID::MYCALL, None)), r1);

            let r2 = res.next().await.unwrap();
            assert_eq!(Ok((CommandID::ARQBW, None)), r2);

            assert!(res.next().await.is_none());
            assert!(evt.next().await.is_none());
        });
    }

    #[test]
    fn test_sink() {
        // since there's no way to get the io back, this test
        // mostly just ensures that a send can complete
        let curs = Cursor::new(vec![0u8; 24]);
        let (_evt, _res, mut sink) = controlstream(curs);
        let _ = executor::block_on(sink.send("ABORT\r".to_owned()));
    }
}
