use std::convert::Into;
use std::string::String;
use std::time::SystemTime;

/// Connection direction
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Direction {
    /// An outgoing connection, via `ARQCALL`
    Outgoing,

    /// An incoming connection, via `LISTEN`
    ///
    /// Parameters:
    /// - Callsign "dialed" by peer. Either `MYCALL` or one of
    ///   `MYAUX`.
    Incoming(String),
}

/// Represents an ARQ connection
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Connection {
    /// Connected peer callsign
    pub peer_call: String,

    /// Connected peer gridsquare, if known
    pub peer_grid: Option<String>,

    /// Connection bandwidth (Hz)
    pub bandwidth: u16,

    /// Connection direction
    pub direction: Direction,

    /// Time connection established
    pub established: SystemTime,
}

impl Connection {
    /// Record a new connection, opened now
    ///
    /// Parameters
    /// - `peer_call`: Peer callsign
    /// - `peer_grid`: Peer gridsquare, if known
    /// - `bandwidth`: Connection bandwidth (Hz)
    /// - `direction`: Connection direction
    pub fn new<S>(
        peer_call: S,
        peer_grid: Option<String>,
        bandwidth: u16,
        direction: Direction,
    ) -> Connection
    where
        S: Into<String>,
    {
        Connection {
            peer_call: peer_call.into(),
            peer_grid,
            bandwidth,
            direction,
            established: SystemTime::now(),
        }
    }

    /// Record a new connection, opened at some arbitrary time
    ///
    /// Parameters
    /// - `peer_call`: Peer callsign
    /// - `peer_grid`: Peer gridsquare, if known
    /// - `bandwidth`: Connection bandwidth (Hz)
    /// - `direction`: Connection direction
    /// - `established`: Time that connection was opened
    pub fn new_at<S>(
        peer_call: S,
        peer_grid: Option<String>,
        bandwidth: u16,
        direction: Direction,
        established: SystemTime,
    ) -> Connection
    where
        S: Into<String>,
    {
        Connection {
            peer_call: peer_call.into(),
            peer_grid,
            bandwidth,
            direction,
            established,
        }
    }
}
