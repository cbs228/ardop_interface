#![feature(async_await)]

extern crate ardop_interface;
extern crate bytes;
#[macro_use]
extern crate clap;
extern crate futures;
#[macro_use]
extern crate log;
extern crate stderrlog;

mod newlineframer;

use std::net::ToSocketAddrs;

use clap::{App, Arg};
use futures::prelude::*;

use ardop_interface::framer::Framed;
use ardop_interface::tnc::*;
use newlineframer::NewlineFramer;

#[runtime::main]
async fn main() {
    // argument parsing
    let matches = App::new("echoserver")
        .version(crate_version!())
        .arg(
            Arg::with_name("ADDRESS")
                .help("TNC hostname:port")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("MYCALL")
                .help("Server callsign")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::with_name("BW")
                .help("ARQ connection bandwidth")
                .default_value("500")
                .required(false)
                .index(3),
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
    let arq_bandwidth = value_t!(matches, "BW", u16).unwrap_or_else(|e| e.exit());
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

    loop {
        // wait for a connection
        let connection = tnc
            .listen(arq_bandwidth, false)
            .await
            .expect("TNC failure during listen");

        // Wrap the I/O connection object in a framer
        // which extracts every line of text. Binary streams
        // like ArqStream have no message boundary delimiters,
        // so it is up to you to split it up.
        let mut framer = Framed::new(connection, NewlineFramer::new());

        loop {
            // read a line
            info!("Waiting for peer data...");
            let line = match framer.next().await {
                None => {
                    // end of connection
                    break;
                }
                Some(line) => line,
            };
            info!("RX: {}", &line);

            // write some
            let num_in = line.len();
            match framer.send(line).await {
                // wrote them
                Ok(_ok) => info!("TX reply ({} bytes)", num_in + 1),

                // peer connection is dead
                Err(e) => {
                    error!("TNC failure during write: {}", e);
                    break;
                }
            }
        }

        // You can reclaim the connection by releasing the
        // framer.
        let (connection, _) = framer.release();
        info!(
            "Terminated connection with {}",
            connection.info().peer_call()
        );
    }
}
