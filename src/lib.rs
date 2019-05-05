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
extern crate futures;
extern crate tokio;
extern crate tokio_io;

pub mod commandchain;
pub mod connectioninfo;
pub mod framing;
pub mod io;
pub mod protocol;
pub mod tncdata;
pub mod tncerror;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
