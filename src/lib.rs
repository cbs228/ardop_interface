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
extern crate romio;

pub mod arqstream;
pub mod connectioninfo;
pub mod framer;
pub mod tnc;
pub mod tncdata;
pub mod tncerror;

mod framing;
mod protocol;
mod tncio;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
