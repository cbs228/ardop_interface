//! Data port connection and events stream
//!

use std::convert::{From, Into};
use std::io;
use std::pin::Pin;
use std::string::String;

use futures::future::{select, Either, Future};
use futures::sink::Sink;
use futures::stream::{Stream, StreamExt};
use futures::task::{Context, Poll};

use super::connevent::ConnEventParser;
use super::data::{DataIn, DataOut};

use crate::protocol::response::{ConnectionStateChange, Event};

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
    D: Stream<Item = io::Result<DataIn>> + Sink<DataOut, Error = io::Error> + Unpin,
    E: Stream<Item = Event> + Unpin,
{
    data_inout: D,
    event_in: E,
    conn_state: ConnEventParser,
    data_eof: bool,
    event_eof: bool,
}

impl<D, E> DataEventStream<D, E>
where
    D: Stream<Item = io::Result<DataIn>> + Sink<DataOut, Error = io::Error> + Unpin,
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
            data_eof: false,
            event_eof: false,
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
    D: Stream<Item = io::Result<DataIn>> + Sink<DataOut, Error = io::Error> + Unpin,
    E: Stream<Item = Event> + Unpin,
{
}

impl<D, E> Stream for DataEventStream<D, E>
where
    D: Stream<Item = io::Result<DataIn>> + Sink<DataOut, Error = io::Error> + Unpin,
    E: Stream<Item = Event> + Unpin,
{
    type Item = DataEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut (*self);

        if !this.data_eof && !this.event_eof {
            // both streams can be read
            while !this.data_eof && !this.event_eof {
                let mut either = select(this.data_inout.next(), this.event_in.next());
                match ready!(Pin::new(&mut either).poll(cx)) {
                    Either::Left((Some(Ok(data)), _f)) => return Poll::Ready(Some(data.into())),
                    Either::Left((_e, _f)) => this.data_eof = true,
                    Either::Right((Some(evt), _f)) => {
                        // try to process this event
                        if let Some(conn_evt) = this.conn_state.process(evt) {
                            return Poll::Ready(Some(conn_evt.into()));
                        }
                    }
                    Either::Right((None, _f)) => this.event_eof = true,
                }
            }
        }

        if !this.data_eof {
            while !this.data_eof {
                match ready!(Pin::new(&mut this.data_inout).poll_next(cx)) {
                    Some(Ok(data)) => return Poll::Ready(Some(data.into())),
                    _ => this.data_eof = true,
                }
            }
        } else if !this.event_eof {
            while !this.event_eof {
                match ready!(Pin::new(&mut this.event_in).poll_next(cx)) {
                    Some(evt) =>
                    // try to process this event
                    {
                        if let Some(conn_evt) = this.conn_state.process(evt) {
                            return Poll::Ready(Some(conn_evt.into()));
                        }
                    }
                    None => this.event_eof = true,
                }
            }
        }

        // all have EOF
        Poll::Ready(None)
    }
}

impl<D, E> Sink<DataOut> for DataEventStream<D, E>
where
    D: Stream<Item = io::Result<DataIn>> + Sink<DataOut, Error = io::Error> + Unpin,
    E: Stream<Item = Event> + Unpin,
{
    type Error = io::Error;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // we are always ready to receive
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: DataOut) -> Result<(), Self::Error> {
        Pin::new(&mut (*self).data_inout).start_send(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut (*self).data_inout).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut (*self).data_inout).poll_close(cx)
    }
}

impl From<DataIn> for DataEvent {
    fn from(data: DataIn) -> Self {
        DataEvent::Data(data)
    }
}

impl From<ConnectionStateChange> for DataEvent {
    fn from(conn: ConnectionStateChange) -> Self {
        DataEvent::Event(conn)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    use futures::executor;
    use futures::stream;
    use futures::stream::StreamExt;
    use futures_codec::Framed;

    use crate::arq::ConnectionFailedReason;
    use crate::framing::data::TncDataFraming;

    #[test]
    fn test_stream() {
        let stream_event = stream::iter(vec![Event::REJECTED(
            ConnectionFailedReason::IncompatibleBandwidth,
        )]);

        let io_data = Cursor::new(b"\x00\x08ARQHELLO".to_vec());

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
