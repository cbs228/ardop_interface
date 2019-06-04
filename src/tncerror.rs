use std::convert::From;
use std::fmt;
use std::io;
use std::string::String;

use futures::channel::mpsc::{SendError, TrySendError};
use futures::prelude::*;

use async_timer::timed::Expired;
use async_timer::Oneshot;

use crate::protocol::constants::CommandID;
use crate::protocol::response::CommandResult;

/// Errors raised by the `Tnc` interface
#[derive(Debug)]
pub enum TncError {
    /// TNC rejected a command with a `FAULT` message
    CommandFailed(String),

    /// The TNC sent a response that does not belong to our command
    CommandResponseInvalid(CommandID, CommandID),

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
            TncError::CommandResponseInvalid(expect, got) => write!(
                f,
                "TNC sent a malformed command response: expected \"{}\" but got \"{}\"",
                expect, got
            ),
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
        TncError::IoError(e)
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

impl<F, T> From<Expired<F, T>> for TncError
where
    F: Future + Unpin,
    T: Oneshot,
{
    fn from(_e: Expired<F, T>) -> Self {
        TncError::CommandTimeout
    }
}

fn connection_reset_err() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionReset,
        "Lost connection to ARDOP TNC",
    )
}
