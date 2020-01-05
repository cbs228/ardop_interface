extern crate ardop_interface;
extern crate async_std;
#[macro_use]
extern crate clap;
extern crate futures;
#[macro_use]
extern crate log;
extern crate futures_codec;
extern crate stderrlog;

use std::net::ToSocketAddrs;
use std::time::Duration;

use async_std::task;
use clap::{App, Arg};
use futures::prelude::*;
use futures_codec::{Framed, LinesCodec};

use ardop_interface::arq::ArqStream;
use ardop_interface::tnc::*;

fn main() {
    task::block_on(async_main())
}

async fn async_main() {
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
        .arg(
            Arg::with_name("beacon")
                .short("b")
                .long("beacon")
                .default_value("0")
                .help("Beacon interval (seconds)"),
        )
        .get_matches();

    let tnc_address_str = matches.value_of("ADDRESS").unwrap();
    let mycallstr = matches.value_of("MYCALL").unwrap();
    let arq_bandwidth = value_t!(matches, "BW", u16).unwrap_or_else(|e| e.exit());
    let beacon_timeout =
        Duration::from_secs(value_t!(matches, "beacon", u64).unwrap_or_else(|e| e.exit()));
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

    // if beaconing, send beacon
    if beacon_timeout > Duration::from_nanos(0) {
        tnc.sendid().await.expect("Failed to send beacon.");
    }

    loop {
        // wait for a connection, a peer discovery, or
        // for our beacon interval to expire.
        match tnc
            .listen_monitor(arq_bandwidth, false, beacon_timeout)
            .await
        {
            Err(TncError::TimedOut) => {
                // Our listen_monitor future timed out, which means
                // it is time to send a beacon.
                tnc.sendid().await.expect("Failed to send beacon.");
            }
            Err(e) => {
                panic!("TNC failed to send beacon: {}", e);
            }
            Ok(ListenMonitor::Connection(conn)) => handle_connection(conn).await,
            Ok(ListenMonitor::PeerDiscovery(_peer)) => {
                // We heard a callsign! Your application might log this,
                // or even try to call the peer.
            }
        }
    }
}

async fn handle_connection(connection: ArqStream) {
    // Wrap the I/O connection object in a framer
    // which extracts every line of text. Binary streams
    // like ArqStream have no message boundary delimiters,
    // so it is up to you to split it up.
    let mut framer = Framed::new(connection, LinesCodec {});

    loop {
        // read a line
        info!("Waiting for peer data...");
        let line = match framer.next().await {
            None => {
                // end of connection
                break;
            }
            Some(Ok(line)) => line,
            Some(Err(e)) => {
                // end of connection, abnormally
                warn!("Unable to frame line: {}", e);
                break;
            }
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
