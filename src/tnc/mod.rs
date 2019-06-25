//! Interface to the ARDOP modem
//!
//! This module contains:
//!
//! * [ArdopTnc](struct.ArdopTnc.html): The main interface
//!   to ARDOP
//! * [TncError](enum.TncError.html): Errors which occur
//!   with the *local* ARDOP TNC, such as a broken TCP
//!   connection
//! * [TncResult](type.TncResult.html): The `Result` type
//!   for TNC operations.
//!
//! # Example
//!
//! ```no_run
//! #![feature(async_await)]
//! use std::net::SocketAddr;
//! use futures::prelude::*;
//!
//! use ardop_interface::tnc::*;
//!
//! #[runtime::main]
//! async fn main() {
//!    let addr = "127.0.0.1:8515".parse().unwrap();
//!    let mut tnc = ArdopTnc::new(&addr, "MYC4LL")
//!        .await
//!        .unwrap();
//!    match tnc.version().await {
//!         Ok(version) => println!("Connected to TNC version {}", version),
//!         Err(e) => println!("Can't query TNC version: {}", e)
//!    }
//!    if let Err(e) = tnc.set_gridsquare("EM00").await {
//!         println!("Can't set GRIDSQUARE: {}", e);
//!    }
//! }
//! ```
//!
//! # TNC Commands
//!
//! `ardop_interface` supports the following common TNC commands.
//! More commands are listed on the `ArdopTnc`
//! [page](struct.ArdopTnc.html).
//!
//! * [`ARQCALL`](struct.ArdopTnc.html#method.connect)
//! * [`LISTEN`](struct.ArdopTnc.html#method.listen)
//! * [`ARQTIMEOUT`](struct.ArdopTnc.html#method.set_arqtimeout)
//! * [`CWID`](struct.ArdopTnc.html#method.set_cwid)
//! * [`GRIDSQUARE`](struct.ArdopTnc.html#method.set_gridsquare)
//! * [`LEADER`](struct.ArdopTnc.html#method.set_leader)
//! * [`MYAUX`](struct.ArdopTnc.html#method.set_myaux)
//! * [`VERSION`](struct.ArdopTnc.html#method.version)
//! * [`TWOTONETEST`](struct.ArdopTnc.html#method.twotonetest)
//!
//! All commands are sent with a reasonable (and adjustable) timeout.
//! If the TNC fails to respond in a timely manner, a
//! [TncError](enum.TncError.html) is raised.
//!
//! # Error Handling
//!
//! Most operations on the [ArdopTnc](struct.ArdopTnc.html) return
//! a [TncResult](type.TncResult.html). If the TNC rejects your command,
//! then the `Err()` half of the result will be set to the reason for
//! the failure. For example, attempting to make an ARQ connection while
//! already connected will probably raise a TNC error.
//!
//! TNC operations which interface with a remote peer can also fail.
//! These failures are contained in an *inner* error structure. For
//! example, if a peer rejects a connection due to a bandwidth mismatch,
//! then [`connect()`](struct.ArdopTnc.html#method.connect) will return
//! `Ok(Err(ConnectionFailedReason::IncompatibleBandwidth))`. The outer
//! error indicates that the TNC successfully attempted the command, and
//! the inner error indicates that the peer rejected (or did not answer)
//! it.

mod ardoptnc;
mod error;

pub use ardoptnc::ArdopTnc;
pub use error::{TncError, TncResult};
