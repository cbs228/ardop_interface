//! TNC errors

use std::convert::From;
use std::fmt;
use std::io;
use std::string::String;

use futures::channel::mpsc::{SendError, TrySendError};

use crate::protocol::response::CommandResult;

/// Errors raised by the `Tnc` interface
#[derive(Debug)]
pub enum TncError {
    /// TNC rejected a command with a `FAULT` message
    CommandFailed(String),

    /// The TNC sent a response that does not belong to our command
    CommandResponseInvalid,

    /// TNC failed to respond in a timely manner
    CommandTimeout,

    /// Socket connectivity problem
    IoError(io::Error),
}

/// Composite `Ok`/`Err` return type
pub type TncResult<T> = Result<T, TncError>;

impl fmt::Display for TncError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            TncError::CommandFailed(s) => write!(f, "TNC command failed: \"{}\"", s),
            TncError::CommandResponseInvalid => {
                write!(f, "TNC sent an unsolicited or invalid command response",)
            }
            TncError::CommandTimeout => write!(f, "TNC command timed out"),
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
            io::ErrorKind::TimedOut => TncError::CommandTimeout,
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

fn connection_reset_err() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionReset,
        "Lost connection to ARDOP TNC",
    )
}
