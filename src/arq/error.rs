use std::fmt;

/// Reasons a remote peer will reject a connection
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConnectionFailedReason {
    /// Peer is busy with an existing connection, or channel busy?
    Busy,

    /// Bandwidth negotiation failed
    IncompatibleBandwidth,

    /// No answer from peer
    NoAnswer,
}

impl fmt::Display for ConnectionFailedReason {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            ConnectionFailedReason::Busy => write!(f, "busy channel"),
            ConnectionFailedReason::IncompatibleBandwidth => {
                write!(f, "rejected by peer: incompatible bandwidth")
            }
            ConnectionFailedReason::NoAnswer => write!(f, "no answer from peer"),
        }
    }
}
