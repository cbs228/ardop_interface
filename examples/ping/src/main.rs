#![feature(async_await)]

extern crate ardop_interface;
#[macro_use]
extern crate clap;
extern crate futures;
#[macro_use]
extern crate log;
extern crate stderrlog;

use std::net::ToSocketAddrs;
use std::process::exit;

use clap::{App, Arg};

use ardop_interface::tnc::*;

#[runtime::main]
async fn main() {
    // argument parsing
    let matches = App::new("ping")
        .version(crate_version!())
        .arg(
            Arg::with_name("ADDRESS")
                .help("TNC hostname:port")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("MYCALL")
                .help("Client callsign")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::with_name("TARGET")
                .help("Peer callsign-SSID to ping")
                .required(true)
                .index(3),
        )
        .arg(
            Arg::with_name("attempts")
                .short("n")
                .default_value("5")
                .help("Number of connection attempts"),
        )
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .help("Increase message verbosity"),
        )
        .get_matches();

    let tnc_address_str = matches.value_of("ADDRESS").unwrap();
    let mycallstr = matches.value_of("MYCALL").unwrap();
    let targetcallstr = matches.value_of("TARGET").unwrap();
    let attempts = value_t!(matches, "attempts", u16).unwrap_or_else(|e| e.exit());
    let verbose = matches.occurrences_of("verbosity") as usize;

    stderrlog::new()
        .module(module_path!())
        .module("ardop_interface")
        .verbosity(verbose + 2)
        .timestamp(stderrlog::Timestamp::Millisecond)
        .color(stderrlog::ColorChoice::Auto)
        .init()
        .unwrap();

    // parse and resolve socket address of TNC
    let tnc_address = tnc_address_str
        .to_socket_addrs()
        .expect("Invalid socket address")
        .next()
        .expect("Error resolving TNC address");

    // connect to TNC
    let mut tnc = ArdopTnc::new(&tnc_address, mycallstr)
        .await
        .expect("Unable to connect to ARDOP TNC");

    // perform the ping
    match tnc
        .ping(targetcallstr, attempts)
        .await
        .expect("TNC error while pinging")
    {
        Some(_repl) => info!("Ping {}: succeed", targetcallstr),
        None => {
            error!("Ping {}: failed", targetcallstr);
            exit(1);
        }
    }
}