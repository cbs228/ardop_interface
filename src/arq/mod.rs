//! Async I/O for ARQ connections
//!
//! This module contains:
//!
//! * The [ArqStream](struct.ArqStream.html) struct, which provides
//!   I/O operations on open ARQ connections
//! * An immutable connection [information](struct.ConnectionInfo.html) struct
//! * Reasons why a connection will [fail](enum.ConnectionFailedReason.html)
//!   to complete
//!
//! # Example
//!
//! ```no_run
//! #![feature(async_await)]
//! use std::str;
//!
//! use futures::prelude::*;
//!
//! use ardop_interface::tnc::*;
//! use ardop_interface::arq::ArqStream;
//!
//! // Provide this method with an open ArqStream
//! async fn handle_connection(mut conn: ArqStream) {
//!     let mut inp = &mut [0u8; 256];
//!     conn.write_all(b"hello world!\n").await.expect("I/O Error");
//!     let num_read = conn.read(inp).await.expect("I/O Error");
//!     if num_read == 0 {
//!         println!("Connection closed by peer (EOF).");
//!     } else {
//!         println!("Received: {}", str::from_utf8(&inp[0..num_read]).unwrap());
//!     }
//!     println!("Connection info: {}", &conn);
//! }
//! ```
//!
//! # Async I/O Interface
//!
//! The [ArqStream](struct.ArqStream.html) implements the
//! `futures::io::AsyncRead` and `futures::io::AsyncWrite` traits.
//! This makes the `ArqStream` behave like an asynchronous
//! `TcpStream`. Bytes written to the object will be sent as
//! payload data to the remote peer. Data sent from the peer will
//! be available for reading.
//!
//! `io::Error`s returned by the I/O methods include:
//!
//! * `std::io::ErrorKind::ConnectionReset`: Failure to communicate
//!   with the local ARDOP TNC—perhaps its TCP connection has
//!   closed.
//! * `std::io::ErrorKind::UnexpectedEof`: For writes to a closed
//!   connection.
//!
//! Reads of a closed connection will return *zero* bytes of data.
//!
//! The `async close()` trait method will initiate a disconnect with
//! the remote peer. Any un-flushed data will be lost. You may also
//! initiate a disconnect by letting the `ArqStream` drop out of scope.
//! **Beware**, however, that this will block the calling thread
//! until the disconnect completes.
//!
//! # Half-Duplex Operation
//!
//! Radio links on a single frequency are *half-duplex*: Only one
//! station can transmit at a time. The ARDOP protocol prevents packet
//! collisions between two peers by designating one station as the
//! *sending* side (ISS) and the other as the *receiving* side (IRS).
//! The IRS can attempt to become the ISS, but this takes time.
//!
//! `ArqStream` relies on the `AUTOBREAK` behavior in the ARDOP TNC.
//! When the link becomes idle, and your station has data to send,
//! the TNC will automatically attempt to become the ISS and send
//! the data. This has important design implications for radio-friendly
//! protocols:
//!
//! * The remote peer will not see any of the data you enqueue
//!   for transmission until it has finished sending.
//!
//! * Link turnovers take time. To maximize throughput, avoid protocol
//!   designs which require lots of "overs."
//!
//! # Framing Devices
//!
//! In most non-trivial applications, it is desirable to turn streams
//! of bytes into higher-level *messages* for your application to
//! consume. Messages are always read atomically, regardless of how
//! many "overs" are required to deliver the data. Never rely on the
//! receiving an entire message in one `read()`—that's not how
//! streaming connections work.
//!
//! For higher-level protocols, you should implement a framer around
//! the raw `ArqStream`. Framers slice the incoming byte stream into
//! messages and perform the inverse for outgoing data. Framers can
//! also encode "native Rust" types into their serialized, wireline
//! representations.
//!
//! `ardop_interface` provides a simple
//! [framer::Framed](../framer/struct.Framed.html) class for this
//! purpose, but you can use any other design of your choice. An
//! example of the `ardop_interface` framer in action can be found
//! in the `echoserver` example.
//!
//! # Monitoring Status of Transmissions
//!
//! Bytes written to this object for transmission go through three
//! distinct phases:
//!
//! 1. **Staged bytes**: Have been accepted by `ardop_interface`
//!    but might not yet have been sent to the local TNC.
//!
//! 2. **Unacknowledged bytes**: Have been accepted by the local
//!    ARDOP TNC but have not yet been `ACK`'d by the remote peer.
//!
//! 3. **Acknowledged bytes**: Have been successfully sent to, and
//!    are `ACK`'d by, the remote peer.
//!
//! The `ArqStream` has methods for retrieving each of these byte
//! counts. There is also a method for retrieving the count of
//! received bytes.
//!
//! # Panics
//!
//! Using the disconnect-on-Drop behavior will panic if the async
//! runtime is the `LocalPool` from the `futures` crate. We
//! recommend the use of the `runtime` crate instead. Alternatively,
//! ensure to close the connection before letting the `ArqStream`
//! drop.

mod arqstream;
mod connectioninfo;
mod error;

pub use arqstream::ArqStream;
pub use connectioninfo::{CallDirection, ConnectionInfo};
pub use error::ConnectionFailedReason;
