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
use std::str;

use clap::{App, Arg};
use futures::prelude::*;

use ardop_interface::tnc::*;

const SHAKESPEARE: &[&[u8]] = &[
    b"Now is the winter of our discontent / Made glorious summer by this sun of York.\n",
    b"Some are born great, some achieve greatness / And some have greatness thrust upon them.\n",
    b"Friends, Romans, countrymen - lend me your ears! / I come not to praise Caesar, but to bury him.\n",
    b"The evil that men do lives after them / The good is oft interred with their bones.\n",
];

#[runtime::main]
async fn main() {
    // argument parsing
    let matches = App::new("echoclient")
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
                .help("Peer callsign-SSID to dial")
                .required(true)
                .index(3),
        )
        .arg(
            Arg::with_name("BW")
                .help("ARQ connection bandwidth")
                .default_value("500")
                .required(false)
                .index(4),
        )
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .help("Increase message verbosity"),
        )
        .arg(
            Arg::with_name("attempts")
                .short("n")
                .default_value("5")
                .help("Number of connection attempts"),
        )
        .get_matches();

    let tnc_address_str = matches.value_of("ADDRESS").unwrap();
    let mycallstr = matches.value_of("MYCALL").unwrap();
    let targetcallstr = matches.value_of("TARGET").unwrap();
    let arq_bandwidth = value_t!(matches, "BW", u16).unwrap_or_else(|e| e.exit());
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

    // set a more reasonable ARQ timeout
    tnc.set_arqtimeout(30).await.expect("Can't set ARQTIMEOUT.");

    // set my grid square
    tnc.set_gridsquare("EM00")
        .await
        .expect("Can't set GRIDSQUARE.");

    // make the connection
    let mut connection = match tnc
        .connect(targetcallstr, arq_bandwidth, false, attempts)
        .await
        .expect("TNC error while dialing")
    {
        Err(reason) => {
            error!("Unable to connect to {}: {}", targetcallstr, reason);
            exit(1)
        }
        Ok(conn) => conn,
    };

    // try to transmit each stanza
    let mut iter = 0usize;
    for &stanza in SHAKESPEARE {
        let rxbuf = &mut [0u8; 512];

        // write some
        match connection.write_all(&stanza).await {
            // wrote them
            Ok(_ok) => info!("TX: {}", str::from_utf8(&stanza).unwrap()),

            // peer connection is dead
            Err(e) => {
                error!("Transmit failed: {}", e);
                break;
            }
        }

        // read back -- no need for framing, as we know precisely
        // the length of data that we are going to receive
        match connection.read_exact(&mut rxbuf[0..stanza.len()]).await {
            Ok(_o) => (),
            Err(e) => {
                error!("Receive failed: {}", e);
                break;
            }
        }

        // check for match
        debug!("RX: {}", str::from_utf8(&rxbuf[0..stanza.len()]).unwrap());
        if stanza == &rxbuf[0..stanza.len()] {
            info!("Received good echo for stanza {}.", iter);
        }

        iter += 1;
    }

    if iter >= SHAKESPEARE.len() {
        info!("Echo server echoed all stanzas correctly. Will now disconnect.");
    } else {
        error!("Echo server failed to echo all stanzas.");
    }

    // connection drops automatically as it passes out of scope
}
