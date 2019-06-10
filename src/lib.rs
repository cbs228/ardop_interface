//! An Async Rust interface to the ARDOP TNC

#![feature(async_await)]
#![recursion_limit = "128"]
#[allow(unused_imports)]
#[macro_use]
extern crate custom_derive;
#[macro_use]
extern crate enum_derive;
#[allow(unused_imports)]
#[macro_use]
extern crate nom;
extern crate bytes;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate log;
extern crate async_timer;
extern crate num;
extern crate runtime;

pub mod arq;
pub mod framer;
pub mod tnc;

mod framing;
mod protocol;
mod tncio;
