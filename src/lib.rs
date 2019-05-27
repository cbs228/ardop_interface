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
extern crate chrono;
#[macro_use]
extern crate futures;
extern crate async_timer;
extern crate romio;

pub mod commandchain;
pub mod connectioninfo;
pub mod framing;
pub mod protocol;
pub mod tncdata;
pub mod tncerror;
pub mod tncio;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
