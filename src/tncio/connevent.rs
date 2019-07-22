//! Connection Event Parser
//!
//! The `ConnEventParser` is a state machine which consumes
//! a stream of `Event`s from the TNC. Events which are relevant
//! to ARQ connections are composited into `ConnectionStateChange`
//! messages. The remaining events are discarded.

use std::convert::Into;
use std::string::String;
use std::time::{Duration, Instant};

use crate::arq::{CallDirection, ConnectionFailedReason, ConnectionInfo};
use crate::protocol::response::{ConnectionStateChange, Event, State};

/// Handles TNC events
pub struct ConnEventParser {
    mycall: String,
    last_target: Option<String>,
    last_arq_state: State,
    is_connected: bool,
    buffer: u64,
    clear_since: Option<Instant>,
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
            clear_since: Some(Instant::now()),
        }
    }

    /// Reset to zero initial conditions
    ///
    /// Reset the event parser to zero initial conditions. This
    /// method should be invoked before sending an `INITIALIZE`
    /// command to the TNC.
    pub fn reset(&mut self) {
        self.last_target = None;
        self.last_arq_state = State::DISC;
        self.is_connected = false;
        self.buffer = 0;
        self.clear_since = Some(Instant::now());
    }

    /// Get this station's callsign
    ///
    /// # Returns
    /// The formally assigned callsign for this station.
    pub fn mycall(&self) -> &String {
        return &self.mycall;
    }

    /// True if the RF channel is detected busy
    ///
    /// True if the busy detector has reported that the
    /// RF channel is currently busy.
    ///
    /// The accuracy of this time stamp depends on the
    /// `ConnEventParser` remaining up-to-date with TNC
    /// events.
    pub fn busy(&self) -> bool {
        self.clear_since.is_none()
    }

    /// Time since the channel has been clear
    ///
    /// Returns the monotonic time which has elapsed since
    /// the RF channel has become clear. If the channel is
    /// currently busy, returns zero.
    ///
    /// The accuracy of this time stamp depends on the
    /// `ConnEventParser` remaining up-to-date with TNC
    /// events.
    pub fn clear_time(&self) -> Duration {
        match &self.clear_since {
            None => Duration::from_micros(0),
            Some(clear_at) => clear_at.elapsed(),
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
            Event::BUSY(busy) => {
                self.clear_since = if busy { None } else { Some(Instant::now()) };

                Some(ConnectionStateChange::Busy(busy))
            }
            Event::CONNECTED(peer, bw, grid) => {
                // connection established... which way?
                let dir = match &self.last_target {
                    None => CallDirection::Outgoing(self.mycall.to_owned()),
                    Some(myalt) => CallDirection::Incoming(myalt.to_owned()),
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
                let was_connecting = !self.is_connected && self.last_arq_state == State::ISS;
                let is_connected = State::is_connected(&st);
                self.last_arq_state = st;

                if was_connecting && !is_connected {
                    // connection failed
                    Some(ConnectionStateChange::Failed(
                        ConnectionFailedReason::NoAnswer,
                    ))
                } else if self.is_connected {
                    match &self.last_arq_state {
                        &State::ISS => Some(ConnectionStateChange::Sending),
                        &State::IRS => Some(ConnectionStateChange::Receiving),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            Event::PING(sender, target, snr, qual) => {
                Some(ConnectionStateChange::Ping(sender, target, snr, qual))
            }
            Event::PINGACK(snr, qual) => Some(ConnectionStateChange::PingAck(snr, qual)),
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
                assert_eq!(500, conn.bandwidth());
                assert_eq!(
                    &CallDirection::Outgoing("W0EME".to_owned()),
                    conn.direction()
                );
                assert_eq!("W1AW", conn.peer_call());
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
                assert_eq!(500, conn.bandwidth());
                assert_eq!(
                    &CallDirection::Incoming("W0EME-S".to_owned()),
                    conn.direction()
                );
                assert_eq!("W1AW", conn.peer_call());
            }
            _ => assert!(false),
        };
    }

    #[test]
    fn test_disconnect_vs_failure() {
        let mut evh = ConnEventParser::new("W0EME");
        evh.process(Event::CONNECTED("W1AW".to_owned(), 500, None));
        let e1 = evh.process(Event::NEWSTATE(State::IRS));
        match e1 {
            Some(ConnectionStateChange::Receiving) => assert!(true),
            _ => assert!(false),
        }
        let e1 = evh.process(Event::NEWSTATE(State::ISS));
        match e1 {
            Some(ConnectionStateChange::Sending) => assert!(true),
            _ => assert!(false),
        }

        let e1 = evh.process(Event::NEWSTATE(State::DISC));
        match e1 {
            None => assert!(true),
            _ => assert!(false),
        }
        let e1 = evh.process(Event::DISCONNECTED);
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

    #[test]
    fn test_ping_ack() {
        let mut evh = ConnEventParser::new("W0EME");
        assert_eq!(
            ConnectionStateChange::PingAck(5, 20),
            evh.process(Event::PINGACK(5, 20)).unwrap()
        );
    }

    #[test]
    fn test_busy() {
        let mut evh = ConnEventParser::new("W0EME");
        assert_eq!(false, evh.busy());

        evh.process(Event::BUSY(true)).unwrap();
        assert_eq!(true, evh.busy());
        evh.process(Event::BUSY(false)).unwrap();
        assert_eq!(false, evh.busy());
    }
}
