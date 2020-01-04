//! TNC errors

use std::convert::From;
use std::fmt;
use std::io;
use std::string::String;

use async_std::future::TimeoutError;

use futures::channel::mpsc::{SendError, TrySendError};

use crate::protocol::response::CommandResult;

/// Errors raised by the `Tnc` interface
#[derive(Debug)]
pub enum TncError {
    /// TNC rejected a command with a `FAULT` message
    CommandFailed(String),

    /// Event timed out
    ///
    /// This error indicates that an expected TNC event did
    /// not occur within the allotted time. The operation
    /// should probably be retried.
    TimedOut,

    /// Socket connectivity problem
    ///
    /// These errors are generally fatal and indicate serious,
    /// uncorrectable problems with the local ARDOP TNC
    /// connection.
    ///
    /// - `io::ErrorKind::ConnectionReset`: lost connection
    ///    to TNC
    /// - `io::ErrorKind::TimedOut`: TNC did not respond to
    ///    a command
    /// - `io::ErrorKind::InvalidData`: TNC sent a malformed
    ///    or unsolicited command response
    IoError(io::Error),
}

/// Composite `Ok`/`Err` return type
pub type TncResult<T> = Result<T, TncError>;

impl fmt::Display for TncError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            TncError::CommandFailed(s) => write!(f, "TNC command failed: \"{}\"", s),
            TncError::TimedOut => write!(f, "Timed out"),
            TncError::IoError(e) => write!(f, "IO Error: {}", e),
        }
    }
}

impl From<CommandResult> for TncError {
    fn from(r: CommandResult) -> Self {
        match r {
            Ok(_a) => unreachable!(),
            Err(x) => TncError::CommandFailed(x),
        }
    }
}

impl From<io::Error> for TncError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            // these errors might come from a runtime timeout
            io::ErrorKind::TimedOut => TncError::TimedOut,
            _ => TncError::IoError(e),
        }
    }
}

impl From<SendError> for TncError {
    fn from(_e: SendError) -> Self {
        TncError::IoError(connection_reset_err())
    }
}

impl<T> From<TrySendError<T>> for TncError {
    fn from(_e: TrySendError<T>) -> Self {
        TncError::IoError(connection_reset_err())
    }
}

impl From<TimeoutError> for TncError {
    fn from(_e: TimeoutError) -> Self {
        TncError::TimedOut
    }
}

fn connection_reset_err() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionReset,
        "Lost connection to ARDOP TNC",
    )
}
