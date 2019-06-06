//! Data port connection and events stream
//!

use std::convert::Into;
use std::io;
use std::pin::Pin;
use std::string::String;

use futures::sink::Sink;
use futures::stream::Stream;
use futures::task::{Context, Poll};

use super::connevent::ConnEventParser;
use crate::protocol::response::{ConnectionStateChange, Event};
use crate::tncdata::{DataIn, DataOut};

/// Connection event or data
///
/// The output of the `DataEventStream`. This contains
/// either data from the remote peer or connection-relevant
/// events, such as disconnect notifications.
pub enum DataEvent {
    /// Data from the remote peer
    Data(DataIn),

    /// A connection-relevant event
    Event(ConnectionStateChange),
}

/// Sink and stream for data packets and events
///
/// This class implements a:
/// 1. Stream which multiplexes connection-relevant
///    events with data received from the remote peer.
///    This allows the caller to receive both by polling
///    a single Stream.
///
/// 2. Sink which delivers outgoing data for transmission
///    to the TNC.
pub struct DataEventStream<D, E>
where
    D: Stream<Item = DataIn> + Sink<DataOut, SinkError = io::Error> + Unpin,
    E: Stream<Item = Event> + Unpin,
{
    data_inout: D,
    event_in: E,
    conn_state: ConnEventParser,
}

impl<D, E> DataEventStream<D, E>
where
    D: Stream<Item = DataIn> + Sink<DataOut, SinkError = io::Error> + Unpin,
    E: Stream<Item = Event> + Unpin,
{
    /// Create new data event stream/sink
    pub fn new<S>(mycall: S, data: D, events: E) -> Self
    where
        S: Into<String>,
    {
        DataEventStream {
            data_inout: data,
            event_in: events,
            conn_state: ConnEventParser::new(mycall),
        }
    }

    /// Connection event parser
    ///
    /// This state machine combines raw events from the TNC
    /// into "combined events" suitable for consumption by
    /// API callers. For example, an incoming connection is
    /// announced using several events. This state machine
    /// combines this into one connection event.
    ///
    /// # Return
    /// Immutable reference to connection event state machine
    pub fn state(&self) -> &ConnEventParser {
        &self.conn_state
    }

    /// Connection event parser
    ///
    /// This state machine combines raw events from the TNC
    /// into "combined events" suitable for consumption by
    /// API callers. For example, an incoming connection is
    /// announced using several events. This state machine
    /// combines this into one connection event.
    ///
    /// # Return
    /// Mutable reference to connection event state machine
    pub fn state_mut(&mut self) -> &mut ConnEventParser {
        &mut self.conn_state
    }
}

impl<D, E> Unpin for DataEventStream<D, E>
where
    D: Stream<Item = DataIn> + Sink<DataOut, SinkError = io::Error> + Unpin,
    E: Stream<Item = Event> + Unpin,
{
}

impl<D, E> Stream for DataEventStream<D, E>
where
    D: Stream<Item = DataIn> + Sink<DataOut, SinkError = io::Error> + Unpin,
    E: Stream<Item = Event> + Unpin,
{
    type Item = DataEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut ctrl_eof = false;
        let mut data_eof = false;

        match Pin::new(&mut (*self).data_inout).poll_next(cx) {
            // got some peer data
            Poll::Ready(Some(data)) => return Poll::Ready(Some(DataEvent::Data(data))),

            // eof (that's bad!)
            Poll::Ready(None) => data_eof = true,

            // not yet (keep going)
            Poll::Pending => { /* no-op */ }
        }

        // try to obtain the next connection event
        loop {
            match Pin::new(&mut (*self).event_in).poll_next(cx) {
                // got an event, try to process it
                Poll::Ready(Some(evt)) => {
                    if let Some(conn_evt) = (*self).conn_state.process(evt) {
                        return Poll::Ready(Some(DataEvent::Event(conn_evt)));
                    }
                }

                // eof (that's bad!)
                Poll::Ready(None) => {
                    ctrl_eof = true;
                    break;
                }

                Poll::Pending => {
                    break;
                }
            }
        }

        if ctrl_eof && data_eof {
            // we have exhausted all our inputs
            Poll::Ready(None)
        } else {
            // one or more inputs still has data, but it's not ready yet
            Poll::Pending
        }
    }
}

impl<D, E> Sink<DataOut> for DataEventStream<D, E>
where
    D: Stream<Item = DataIn> + Sink<DataOut, SinkError = io::Error> + Unpin,
    E: Stream<Item = Event> + Unpin,
{
    type SinkError = io::Error;

    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::SinkError>> {
        // we are always ready to receive
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: DataOut) -> Result<(), Self::SinkError> {
        Pin::new(&mut (*self).data_inout).start_send(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::SinkError>> {
        Pin::new(&mut (*self).data_inout).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::SinkError>> {
        Pin::new(&mut (*self).data_inout).poll_close(cx)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    use futures::executor;
    use futures::stream;
    use futures::stream::StreamExt;

    use crate::framer::Framed;
    use crate::framing::data::TncDataFraming;
    use crate::protocol::response::ConnectionFailedReason;

    #[test]
    fn test_stream() {
        let stream_event = stream::iter(vec![Event::REJECTED(
            ConnectionFailedReason::IncompatibleBandwidth,
        )]);

        let io_data = Cursor::new(b"ARQ\x00\x05HELLO".to_vec());

        let mut dstream = DataEventStream::new(
            "W1AW",
            Framed::new(io_data, TncDataFraming::new()),
            stream_event,
        );

        executor::block_on(async {
            match dstream.next().await {
                Some(DataEvent::Data(_d)) => assert!(true),
                _ => assert!(false),
            }

            match dstream.next().await {
                Some(DataEvent::Event(ConnectionStateChange::Failed(
                    ConnectionFailedReason::IncompatibleBandwidth,
                ))) => assert!(true),
                _ => assert!(false),
            }

            assert!(dstream.next().await.is_none());
        });
    }
}
