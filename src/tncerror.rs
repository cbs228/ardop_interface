use std::convert::From;
use std::fmt;
use std::io;
use std::string::String;

use crate::protocol::response::CommandResult;

/// Errors raised by the `Tnc` interface
#[derive(Debug)]
pub enum TncError {
    /// TNC rejected a command with a `FAULT` message
    CommandFailed(String),

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
