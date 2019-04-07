#![recursion_limit = "128"]
#[allow(unused_imports)]
#[macro_use]
extern crate custom_derive;
#[macro_use]
extern crate enum_derive;
#[allow(unused_imports)]
#[macro_use]
extern crate nom;

pub mod command;
pub mod connection;
pub mod constants;
pub mod response;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
