//! Control streams
//!
//! This module exposes a `datastream(AsyncRead)` method
//! which
//! 1. frames the TNC data port socket; and
//! 2. splits the output into ARQ and FEC data

use std::collections::vec_deque::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use tokio::codec::FramedRead;
use tokio::prelude::*;

use crate::framing::data::TncDataFraming;
use crate::tncdata::DataIn;

/// Bind input streams to a TNC data socket
///
/// This method takes ownership of a TNC data socket's
/// read half and returns two futures `Stream`s: one for
/// ARQ data and another for FEC data.
///
/// # Parameters
/// - `data_in`: TNC control port (read half)
///
/// # Returns
/// 0. A `Stream` of ARQ data received from remote stations
/// 1. A `Stream` of FEC data received from remote stations
pub fn datastream<R>(data_in: R) -> (DataStreamArq<R>, DataStreamFec<R>)
where
    R: AsyncRead,
{
    let state = DataState {
        data_in: FramedRead::new(data_in, TncDataFraming::new()),
        arq_queue: VecDeque::with_capacity(32),
        fec_queue: VecDeque::with_capacity(32),
    };

    let stateref = Arc::new(Mutex::new(state));

    let arq = DataStreamArq {
        state: stateref.clone(),
    };

    let fec = DataStreamFec { state: stateref };

    (arq, fec)
}

// Holds state for the control streams
struct DataState<R>
where
    R: AsyncRead,
{
    data_in: FramedRead<R, TncDataFraming>,
    arq_queue: VecDeque<Bytes>,
    fec_queue: VecDeque<Bytes>,
}

/// Stream of TNC Connection Events
pub struct DataStreamArq<R>
where
    R: AsyncRead,
{
    state: Arc<Mutex<DataState<R>>>,
}

/// Stream of TNC Command Results
pub struct DataStreamFec<R>
where
    R: AsyncRead,
{
    state: Arc<Mutex<DataState<R>>>,
}

impl<R> Stream for DataStreamArq<R>
where
    R: AsyncRead,
{
    type Item = Bytes;
    type Error = io::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        let mut state = self.state.lock().unwrap();

        loop {
            // Any in the ARQ queue?
            if let Some(out) = state.arq_queue.pop_front() {
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

impl<R> Stream for DataStreamFec<R>
where
    R: AsyncRead,
{
    type Item = Bytes;
    type Error = io::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        let mut state = self.state.lock().unwrap();

        loop {
            // Any in the FEC queue?
            if let Some(out) = state.fec_queue.pop_front() {
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

impl<R> DataState<R>
where
    R: AsyncRead,
{
    // Polls for next item in the incoming stream
    pub fn poll_next(&mut self) -> Result<Async<Option<()>>, io::Error> {
        match self.data_in.poll()? {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(None) => Ok(Async::Ready(None)),
            Async::Ready(Some(DataIn::ARQ(res))) => {
                // enqueue it
                self.arq_queue.push_back(res);
                Ok(Async::Ready(Some(())))
            }
            Async::Ready(Some(DataIn::FEC(evt))) => {
                // enqueue it
                self.fec_queue.push_back(evt);
                Ok(Async::Ready(Some(())))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    #[test]
    fn test_data_stream() {
        let words = b"ARQ\x00\x05HELLOFEC\x00\x05WORLDERR".to_vec();
        let curs = Cursor::new(words);
        let (arq, fec) = datastream(curs);

        let mut arqwaiter = arq.wait();
        let mut fecwaiter = fec.wait();

        let f = fecwaiter.next().unwrap().unwrap();
        assert_eq!(f, "WORLD");

        let a = arqwaiter.next().unwrap().unwrap();
        assert_eq!(a, "HELLO");
    }
}
