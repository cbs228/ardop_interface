//! Connection Event Parser
//!
//! The `ConnEventParser` is a state machine which consumes
//! a stream of `Event`s from the TNC. Events which are relevant
//! to ARQ connections are composited into `ConnectionStateChange`
//! messages. The remaining events are discarded.

use std::convert::Into;
use std::string::String;

use crate::connectioninfo::{ConnectionInfo, Direction};
use crate::protocol::response::{ConnectionFailedReason, ConnectionStateChange, Event, State};

/// Handles TNC events
pub struct ConnEventParser {
    mycall: String,
    last_target: Option<String>,
    last_arq_state: State,
    is_connected: bool,
    buffer: u64,
}

impl ConnEventParser {
    /// Create an `ConnectionEventParser`
    ///
    /// # Parameters
    /// - `mycall`: Your formally-assigned callsign
    ///
    /// # Returns
    /// An `EventHandler`, ready for use
    pub fn new<S>(mycall: S) -> ConnEventParser
    where
        S: Into<String>,
    {
        ConnEventParser {
            mycall: mycall.into(),
            last_target: None,
            last_arq_state: State::DISC,
            is_connected: false,
            buffer: 0,
        }
    }

    /// Process an event
    ///
    /// Processes an event notification received asynchronously
    /// from the TNC. Returns either a `ConnectionStateChange`,
    /// an unhandled `Event` for possible further processing, or
    /// `None` if the event was consumed.
    ///
    /// # Parameters
    /// - `event`: Event to process
    ///
    /// # Returns
    /// A composited event
    pub fn process(&mut self, event: Event) -> Option<ConnectionStateChange> {
        match event {
            Event::BUFFER(buf) => {
                let oldbuffer = self.buffer;
                self.buffer = buf;

                if buf > 0 || oldbuffer > 0 {
                    Some(ConnectionStateChange::SendBuffer(buf))
                } else {
                    None
                }
            }
            Event::CONNECTED(peer, bw, grid) => {
                // connection established... which way?
                let dir = match &self.last_target {
                    None => Direction::Outgoing(self.mycall.to_owned()),
                    Some(myalt) => Direction::Incoming(myalt.to_owned()),
                };

                self.is_connected = true;
                self.last_target = None;

                // notify of event
                Some(ConnectionStateChange::Connected(ConnectionInfo::new(
                    peer, grid, bw, dir,
                )))
            }
            Event::DISCONNECTED => {
                if self.is_connected {
                    self.is_connected = false;
                    Some(ConnectionStateChange::Closed)
                } else {
                    None
                }
            }
            Event::NEWSTATE(st) => {
                let was_connecting = State::is_connected(&self.last_arq_state);
                let is_connected = State::is_connected(&st);
                self.last_arq_state = st;

                if self.is_connected && !is_connected {
                    // connection closed
                    self.is_connected = false;
                    Some(ConnectionStateChange::Closed)
                } else if was_connecting && !is_connected {
                    // connection failed
                    Some(ConnectionStateChange::Failed(
                        ConnectionFailedReason::NoAnswer,
                    ))
                } else {
                    None
                }
            }
            Event::REJECTED(cfr) => Some(ConnectionStateChange::Failed(cfr)),
            Event::TARGET(tgtcall) => {
                self.last_target = Some(tgtcall);
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::protocol::response::ConnectionStateChange;

    #[test]
    fn test_outgoing_connection() {
        let mut evh = ConnEventParser::new("W0EME");
        evh.process(Event::NEWSTATE(State::ISS));
        let e1 = evh.process(Event::CONNECTED("W1AW".to_owned(), 500, None));
        match e1 {
            Some(ConnectionStateChange::Connected(conn)) => {
                assert_eq!(500, conn.bandwidth);
                assert_eq!(Direction::Outgoing("W0EME".to_owned()), conn.direction);
                assert_eq!("W1AW", conn.peer_call);
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn test_incoming_connection() {
        let mut evh = ConnEventParser::new("W0EME");
        evh.process(Event::TARGET("W0EME-S".to_owned()));
        evh.process(Event::NEWSTATE(State::IRS));
        let e1 = evh.process(Event::CONNECTED("W1AW".to_owned(), 500, None));

        match e1 {
            Some(ConnectionStateChange::Connected(conn)) => {
                assert_eq!(500, conn.bandwidth);
                assert_eq!(Direction::Incoming("W0EME-S".to_owned()), conn.direction);
                assert_eq!("W1AW", conn.peer_call);
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn test_disconnect_vs_failure() {
        let mut evh = ConnEventParser::new("W0EME");
        evh.process(Event::CONNECTED("W1AW".to_owned(), 500, None));
        let e1 = evh.process(Event::NEWSTATE(State::DISC));
        match e1 {
            Some(ConnectionStateChange::Closed) => assert!(true),
            _ => assert!(false),
        }

        evh.process(Event::NEWSTATE(State::ISS));
        let e2 = evh.process(Event::NEWSTATE(State::DISC));
        match e2 {
            Some(ConnectionStateChange::Failed(ConnectionFailedReason::NoAnswer)) => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_buffer() {
        let mut evh = ConnEventParser::new("W0EME");
        assert!(evh.process(Event::BUFFER(0)).is_none());
        assert!(evh.process(Event::BUFFER(0)).is_none());
        assert_eq!(
            ConnectionStateChange::SendBuffer(10),
            evh.process(Event::BUFFER(10)).unwrap()
        );
        assert_eq!(
            ConnectionStateChange::SendBuffer(5),
            evh.process(Event::BUFFER(5)).unwrap()
        );
        assert_eq!(
            ConnectionStateChange::SendBuffer(0),
            evh.process(Event::BUFFER(0)).unwrap()
        );
        assert!(evh.process(Event::BUFFER(0)).is_none());
    }
}
