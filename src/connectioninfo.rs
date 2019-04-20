use std::convert::Into;
use std::fmt;
use std::string::String;

use chrono::prelude::*;

/// Connection direction
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Direction {
    /// An outgoing connection, via `ARQCALL`
    ///
    /// Parameters:
    /// - My callsign (`MYCALL`)
    Outgoing(String),

    /// An incoming connection, via `LISTEN`
    ///
    /// Parameters:
    /// - Callsign "dialed" by peer. Either `MYCALL` or one of
    ///   `MYAUX`.
    Incoming(String),
}

/// Represents an ARQ connection
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ConnectionInfo {
    /// Connected peer callsign
    pub peer_call: String,

    /// Connected peer gridsquare, if known
    pub peer_grid: Option<String>,

    /// Connection bandwidth (Hz)
    pub bandwidth: u16,

    /// Connection direction
    pub direction: Direction,

    /// Time connection established
    pub established: DateTime<Utc>,
}

impl ConnectionInfo {
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
    ) -> ConnectionInfo
    where
        S: Into<String>,
    {
        ConnectionInfo {
            peer_call: peer_call.into(),
            peer_grid,
            bandwidth,
            direction,
            established: Utc::now(),
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
        established: DateTime<Utc>,
    ) -> ConnectionInfo
    where
        S: Into<String>,
    {
        ConnectionInfo {
            peer_call: peer_call.into(),
            peer_grid,
            bandwidth,
            direction,
            established,
        }
    }
}

impl fmt::Display for ConnectionInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let fm = match &self.direction {
            Direction::Outgoing(c) => c.as_str(),
            Direction::Incoming(_t) => self.peer_call.as_str(),
        };
        let to = match &self.direction {
            Direction::Outgoing(_c) => self.peer_call.as_str(),
            Direction::Incoming(t) => t.as_str(),
        };
        match &self.peer_grid {
            Some(grid) => write!(
                f,
                "{}>{} [{}][{} Hz][{}]",
                fm,
                to,
                grid,
                self.bandwidth,
                self.established.format("%Y-%m-%d %H:%M:%SZ")
            ),
            None => write!(
                f,
                "{}>{} [????][{} Hz][{}]",
                fm,
                to,
                self.bandwidth,
                self.established.format("%Y-%m-%d %H:%M:%SZ")
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_fmt() {
        let ct = ConnectionInfo::new("W9ABC", None, 500, Direction::Outgoing("W1AW".to_string()));
        let s = ct.to_string();
        assert!(s.starts_with("W1AW>W9ABC [????][500 Hz]"));

        let ct = ConnectionInfo::new(
            "W9ABC",
            Some("EM00".to_owned()),
            500,
            Direction::Incoming("W1AW-S".to_owned()),
        );
        let s = ct.to_string();
        assert!(s.starts_with("W9ABC>W1AW-S [EM00][500 Hz]"));
    }
}
