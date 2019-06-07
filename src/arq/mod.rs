//! ARQ Connection Types
//!
//! This module contains `ArqStream`, which is similar to
//! the asynchronous `TcpStream`... but for radio.

mod arqstream;
mod connectioninfo;
mod error;

pub use arqstream::ArqStream;
pub use connectioninfo::{CallDirection, ConnectionInfo};
pub use error::ConnectionFailedReason;
