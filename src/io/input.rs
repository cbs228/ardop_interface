//! Receiver futures
//!
//! Contains futures for receiving control and data
//! messages from the TNC.

use std::convert::Into;
use std::io;
use std::string::String;
use std::sync::Arc;

use futures::prelude::*;

use futures::sync::mpsc;
use tokio::codec::FramedRead;
use tokio::io::AsyncRead;

use super::super::framing::control::TncControlFraming;
use super::super::framing::data::TncDataFraming;
use super::super::response::{CommandResult, ConnectionStateChange, Event, Response};
use super::super::tncdata::DataIn;
use super::busylock;
use super::connevent::ConnEventParser;

/// Creates futures for reading control messages
///
/// This method binds a framer to the read half of the TNC
/// control socket, consuming it. It produces futures for
/// reading from the control socket.
///
/// # Parameters
/// - `sock`: Read half of the TNC socket
/// - `mycall`: Your station's formally-assigned callsign.
///   This is used to populate `ConnectionStateChange`
///   messages.
///
/// # Returns
/// 0. A task which will run the framer to completion. You
///    should probably use `tokio::executor::spawn()` to
///    run it.
/// 1. A stream which contains only `CommandResult`, which
///    describes the success or failure of a command sent to
///    the TNC.
/// 2. A stream which contains only `ConnectionStateChange`
///    events, received asynchronously from the TNC
/// 3. A `BusyLock` which can be used to block a thread until
///    the radio channel becomes clear.
pub fn control<S, C>(
    sock: S,
    mycall: C,
) -> (
    impl Future<Item = (), Error = ()>,
    impl Stream<Item = CommandResult, Error = io::Error>,
    impl Stream<Item = ConnectionStateChange, Error = io::Error>,
    Arc<busylock::BusyLock>,
)
where
    S: AsyncRead,
    C: Into<String>,
{
    let mut conn_evt = ConnEventParser::new(mycall);
    let (busylock_send, busylock_recv) = busylock::new();

    let framer = FramedRead::new(sock, TncControlFraming::new());

    // connect as follows:
    //
    // framer --+--> response_in0 -> result_out
    //          |
    //          +--> response_in1 -> event_out -> conn_evt_out
    //
    // by cloning the output of framer and putting it through some filters
    let (response_in0, result_out) = filter_command_results();
    let (response_in1, event_out) = filter_events();

    let cloner = framer
        .forward(response_in0.fanout(response_in1))
        .map(|_i| {})
        .map_err(|_e| ());

    // now bolt on the ConnEventParser to the event stream
    let conn_evt_out = event_out
        .inspect(move |evt| match evt {
            Event::BUSY(bus) => busylock_send.set_busy(*bus),
            _ => { /* no-op */ }
        })
        .filter_map(move |evt| conn_evt.process(evt));

    // return all these futures
    (cloner, result_out, conn_evt_out, busylock_recv)
}

/// Creates future for reading data messages
///
/// This method binds a framer to the read half of the TNC's
/// data socket. This returns a stream which reads payload
/// data (i.e., from over the air) as it becomes available.
///
/// # Parameters
/// - `sock`: Read half of the TNC data socket
///
/// # Returns
/// A stream which contains byte fragments received from
/// ARQ connections and FEC bursts.
pub fn data<S>(sock: S) -> impl Stream<Item = DataIn, Error = io::Error>
where
    S: AsyncRead,
{
    FramedRead::new(sock, TncDataFraming::new())
}

// filters a stream of Response to only Event
fn filter_command_results() -> (
    impl Sink<SinkItem = Response, SinkError = io::Error> + Clone,
    impl Stream<Item = CommandResult, Error = io::Error>,
) {
    let (sink, stream) = sink_to_stream::<Response>();
    let filter_out = stream.filter_map(|resp| match resp {
        Response::CommandResult(res) => Some(res),
        _ => None,
    });
    (sink, filter_out)
}
// filters a stream of Response to only Event
fn filter_events() -> (
    impl Sink<SinkItem = Response, SinkError = io::Error> + Clone,
    impl Stream<Item = Event, Error = io::Error>,
) {
    let (sink, stream) = sink_to_stream::<Response>();
    let filter_out = stream.filter_map(|resp| match resp {
        Response::Event(evt) => Some(evt),
        _ => None,
    });
    (sink, filter_out)
}

// a channel which connects a sink to a stream
fn sink_to_stream<T>() -> (
    impl Sink<SinkItem = T, SinkError = io::Error> + Clone,
    impl Stream<Item = T, Error = io::Error>,
) {
    let (sink, stream) = mpsc::unbounded::<T>();
    (
        sink.sink_map_err(|_e| broken_pipe_error()),
        stream.map_err(|_e| broken_pipe_error()),
    )
}

// returns a "broken pipe" error for closed channels
fn broken_pipe_error() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "channel closed")
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    use super::super::super::constants::CommandID;
    use super::super::super::response::ConnectionFailedReason;

    #[test]
    fn test_control() {
        let curs = Cursor::new("BUSY TRUE\rMYCALL\rREJECTEDBW\r".as_bytes().to_owned());

        let (task, res, conn_state, busy) = control(curs, "W1AW");

        tokio::run(task);
        let responses: Vec<CommandResult> = res.wait().map(|resp| resp.unwrap()).collect();
        assert_eq!(responses[0], Ok((CommandID::MYCALL, None)));

        let conns: Vec<ConnectionStateChange> = conn_state.wait().map(|csc| csc.unwrap()).collect();
        assert_eq!(
            conns[0],
            ConnectionStateChange::Failed(ConnectionFailedReason::IncompatibleBandwidth)
        );

        assert!(busy.busy());
    }
}
