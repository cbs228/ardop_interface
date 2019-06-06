use std::convert::Into;
use std::fmt;
use std::string::String;

/// Connection direction
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum CallDirection {
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
///
/// `ConnectionInfo` are immutable once created.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ConnectionInfo {
    /// Connected peer callsign
    peer_call: String,

    /// Connected peer gridsquare, if known
    peer_grid: Option<String>,

    /// Connection bandwidth (Hz)
    bandwidth: u16,

    /// Connection direction
    direction: CallDirection,
}

impl ConnectionInfo {
    /// Record a new connection
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
        direction: CallDirection,
    ) -> ConnectionInfo
    where
        S: Into<String>,
    {
        ConnectionInfo {
            peer_call: peer_call.into(),
            peer_grid,
            bandwidth,
            direction,
        }
    }

    /// Peer callsign
    ///
    /// For outgoing connections, this is the dialed
    /// callsign and not how the peer identifies itself.
    /// For incoming connections, this is how the
    /// peer identifies itself.
    pub fn peer_call(&self) -> &String {
        &self.peer_call
    }

    /// Peer Maidenhead Grid Square, if reported
    pub fn peer_grid(&self) -> &Option<String> {
        &self.peer_grid
    }

    /// Connection bandwidth, in Hz
    pub fn bandwidth(&self) -> u16 {
        self.bandwidth
    }

    /// Connection direction (incoming vs outgoing)
    pub fn direction(&self) -> &CallDirection {
        &self.direction
    }
}

impl fmt::Display for ConnectionInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let fm = match &self.direction {
            CallDirection::Outgoing(c) => c.as_str(),
            CallDirection::Incoming(_t) => self.peer_call.as_str(),
        };
        let to = match &self.direction {
            CallDirection::Outgoing(_c) => self.peer_call.as_str(),
            CallDirection::Incoming(t) => t.as_str(),
        };
        match &self.peer_grid {
            Some(grid) => write!(f, "{}>{} [{}][{} Hz]", fm, to, grid, self.bandwidth),
            None => write!(f, "{}>{} [????][{} Hz]", fm, to, self.bandwidth),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_fmt() {
        let ct = ConnectionInfo::new("W9ABC", None, 500, CallDirection::Outgoing("W1AW".to_string()));
        let s = ct.to_string();
        assert!(s.starts_with("W1AW>W9ABC [????][500 Hz]"));

        let ct = ConnectionInfo::new(
            "W9ABC",
            Some("EM00".to_owned()),
            500,
            CallDirection::Incoming("W1AW-S".to_owned()),
        );
        let s = ct.to_string();
        assert!(s.starts_with("W9ABC>W1AW-S [EM00][500 Hz]"));
    }
}
