# ardop_interface

An Async Rust interface to the ARDOP TNC

[Documentation](https://docs.rs/ardop_interface) |
[Crate](https://crates.io/crates/ardop_interface) |
[Git](https://github.com/cbs228/ardop_interface)

## Introduction

`ardop_interface` integrates with the Amateur Radio Digital
Open Protocol ([ARDOP](https://ardop.groups.io/g/main)) soundcard
modem softare. The ARDOP modem is intended to provide reliable,
low-speed connectivity over High Frequency (HF) radio links.

*This crate is not ARDOP.* This crate is only an interface. With
this interface, and separate ARDOP modem software, you can build
full-featured Rust applications that communicate over the radio.

## Minimal Example

```rust
#![feature(async_await)]
use std::net::SocketAddr;
use futures::prelude::*;

use ardop_interface::tnc::*;

#[runtime::main]
async fn main() {
   let addr = "127.0.0.1:8515".parse().unwrap();
   let mut tnc = ArdopTnc::new(&addr, "MYC4LL")
       .await
       .unwrap();
   let mut conn = tnc.connect("T4GET", 500, false, 3)
       .await
       .expect("TNC failure")
       .expect("Connection failed");
   conn.write_all(b"Hello, world!\n").await.unwrap();
   conn.close().await;
}
```

See the
[`examples/`](https://github.com/cbs228/ardop_interface/tree/master/examples)
directory in the source code repository for a complete client
and server. The examples also demonstrate the `async` `runtime` crate,
argument parsing, and logging.

## The ARDOP Modem

Like most amateur radio modems, ARDOP is designed to interface
with analog, single-sideband (SSB) transceivers via a sound
card interface. A computer—which may be a simple, single-board
computer—turns data into sound, and back again.

ARDOP is designed to:

* Automatically retry failed transmissions. This mode of
  operation is called *automatic repeat request* (ARQ),
  and it helps ensure that data reaches its final destination.

* Use the fastest available mode for the signal-to-noise ratio
  and band conditions

* Support unattended, automatic station operation

* Perform well on the high frequency (HF) bands

Best of all, ARDOP has an open development model. Full protocol
specifications are available, and open-source implementations of
the modem exist.

ARDOP is intended for use by licensed radio amateurs. If you'd
like to get started with the hobby, you should find a club,
"hamfest," or "hamvention" near you!

## The Rust Interface

The ARDOP software has a standardized "terminal node controller"
(TNC) interface for clients to use. To use the TNC, clients must
make two simultaneous TCP connections and perform a great deal
of serialization and de-serialization. This crate handles many
of these details for you.

This crate exposes an `async` API that many rustaceans will
find familiar: socket programming. The
[`ArqStream`](arq/index.html) object is designed
to mimic an async `TcpStream`. Once a connection is made,
data is exchanged with asynchronous reads from, and writes to,
the `ArqStream` object.

The `async` API allows ARDOP to coexist with native TCP sockets,
GUIs, and other I/O processes while maintaining a small system
resource footprint.

## Development Status

This crate's API will not stabilize until the stable release
of
* the [futures-preview](https://docs.rs/futures-preview/) crate; and
* the [async-await](https://areweasyncyet.rs/) Rust language feature.

For now, it is necessary to use a `nightly` Rust toolchain.

This crate has been tested against rust `1.37.0-nightly` built
on `2019-06-23`. Earlier builds may not function correctly.

## Prerequisites

First, ensure that a nightly Rust is installed and available on
your platform.

```bash
rustup toolchain install nightly
rustup default nightly
```

Next, obtain a compatible implementation of ARDOP. You must use
ARDOP software which implements protocol **version one**.
The ARDOP v2 specification has been withdrawn by its authors, and
version three is presently in development. The TNC interface on
which this crate depends can change during major releases.

These instructions assume the use of John Wiseman's `ardopc`, version 1.
Other implementations will probably work, but this crate has not
been tested against them. You may be able to obtain this software at
<http://www.cantab.net/users/john.wiseman/Downloads/Beta/TeensyProjects.zip>
or from the ARDOP [forums](https://ardop.groups.io/g/users/topics).

You will need your system's C/C++ compiler in order to build `ardopc`.
Debian-based distributions can install these with:

```bash
sudo apt-get install build-essential
```

Unpack the archive, `cd` into the `ARDOPC` subdirectory, and
run `make` to build the software. There is also a Visual Studio
project for Windows users. Alternatively, binary builds may be
available in
<http://www.cantab.net/users/john.wiseman/Downloads/Beta/>.

You should now be able to invoke ARDOP as

```bash
./ardopc PORT INDEV OUTDEV
```

where
* `PORT` is the desired TCP control port (typically `8515`)
* `INDEV` is the ALSA device name for your "`line in`"
  soundcard port. If your system has pulseaudio, you may use
  it with the `pulse` device.
* `OUTDEV` is the ALSA device name for your "`line out`"
  soundcard port. If your system has pulseaudio, you may use
  it with the `pulse` device.

## Running the Examples

The examples are not published to `crates.io.` You will need
to clone our source repository instead:

```bash
git clone https://github.com/cbs228/ardop_interface.git
```

To conduct a local test of ARDOP, you must run two instances
of the ARDOP modem. To make an acoustic channel between the
two the modems, place your microphone in close proximity to
your speakers. Alternatively, on linux, you may also use the
PulseAudio null sink:

```bash
pacmd load-module module-null-sink sink_name=Virtual1
pacmd set-default-sink Virtual1
pacmd set-default-source Virtual1.monitor
```

Start two instances of ardopc

```bash
./ardopc 8515 pulse pulse &
./ardopc 8520 pulse pulse &
```

Build and run the `echoserver` package with

```bash
cargo run --package echoserver -- localhost:8515 MYCALL-S 200
```

Replace `MYCALL` with your callsign. The `-S` is a
Service Set Identifier (SSID), which is an arbitrary
single-character extension to your callsign.

Now run the `echoclient` with

```bash
cargo run --package echoclient -- localhost:8520 MYCALL-C MYCALL-S 200
```

The `echoclient` will send a number of pre-programmed
text stanzas to the `echoserver`, which will parrot them
back. Both the client and server will print their progress
to stderr. The demo succeeds if the client prints

```txt
Echo server echoed all stanzas correctly.
```

You can also use the `echoserver` interactively via a
line-oriented chat program, like `ARIM`.

When you are finished, remove the null sink if you are
using it.

```bash
pacmd unload-module module-null-sink
```

## Unsupported ARDOP Features

At present, this crate only supports the ARDOP `ARQ`
connection-oriented protocol, which is TCP-like. ARDOP also
has a connectionless `FEC` protocol, which is UDP-like.
`ardop_interface` does not currently support FEC mode.

The following other features are currently not implemented
by this crate.

* **ID frames**: ID frames are not yet supported. At present,
  they are silently discarded.

* **Pings**: Not yet supported.

* **Busy channel detection**: This crate relies on the
  ARDOP TNC to perform busy channel detection. Support for
  this functionality varies across ARDOP implementations.

* **Rig control**: No type of rig control is presently
  integrated. This crate cannot provide the following
  functionality, at present.

  * **Tuning**: `ardop_interface` knows nothing about your rig
    and cannot adjust its frequency or mode.

  * **Keying**: This crate does not key your radio's PTT.
    The ARDOP TNC may be able to do this for you. `ardopc` can
    key transmitters over a serial connection.

  * **Scanning**: The ARDOP frequency agility / scanning functions
    in `LISTEN` mode are not supported

## Next Steps

What sorts of protocols would you like to see on the air?
With a reliable, TCP-like link between stations, the
ionosphere is the limit!

## Further Reading

* [`tnc`](tnc/index.html): Main TNC command and control via the
  `ArdopTnc`
* [`arq`](arq/index.html): Reliable ARQ connections with
  `ArqStream`

License: MIT OR Apache-2.0
