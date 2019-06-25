# ardop_interface

An Async Rust interface to the ARDOP TNC

## Minimal Example

```rust
#![feature(async_await)]
use std::net::SocketAddr;
use futures::prelude::*;
use futures::executor::block_on;

use ardop_interface::tnc::*;

block_on(async {
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
});
```

See the `examples/` directory in the source code distribution
for a complete example client and server. The examples also
demonstrate the `async` runtime, argument parsing, and logging.

## Introduction

`ardop_interface` integrates with the Amateur Radio Digital
Open Protocol ([ARDOP](https://ardop.groups.io/g/main)) soundcard
modem softare. This modem is intended for use with analog single
sideband (SSB) transceivers over radio links. ARDOP is designed
to perform well on the High Frequency (HF) bands, which are
notoriously difficult to use.

Unlike many older digital amateur modes, ARDOP is designed to

* automatically retry failed data; and
* use the fastest reliable mode for the available signal-to-noise
  ratio (SNR).

These properties make ARDOP suitable for use in
automatically-controlled stations, which are not always manned
by a control operator. Think of ARDOP as "*TCP for radio.*"

*This crate is not ARDOP.* This crate is only an interface. In order
to use this crate, you must download and install a compatible
ARDOP software modem. ARDOP software is developed and released
separately, by different developers, under a different open-source
license than this crate.

This crate exposes an `async` Rust API to the ARDOP software.
The [`ArqStream`](arq/struct.ArqStream.html) object is designed
to mimic an async `TcpStream`. This substantially reduces the
API surface of ARDOP, making it easier to incorporate into
higher-level applications.

ARDOP is intended for use by licensed radio amateurs. If you'd
like to get started with the hobby, you should find a club,
"hamfest," or "hamvention" near you!

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

Next, obtain the ARDOP software. This crate has been tested
against John Wiseman's `ardopc` version 1, and these instructions
assume its use. You may be able to obtain this software at
<http://www.cantab.net/users/john.wiseman/Downloads/Beta/TeensyProjects.zip>.

Regardless of your choice of implementations, you must use a modem
which implements ARDOP protocol **version one**. Version two of the ARDOP
protocol has been withdrawn by its authors. Version three is presently
in development, and its interface protocol may be different.

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

Build and run the `echoserver` in the `examples/echoserver`
directory with

```bash
cargo run -- localhost:8515 N0CALL-S 200
```

Replace `N0CALL` with your callsign. The `-S` is a
Service Set Identifier (SSID), which is an arbitrary
single-character extension to your callsign.

Now use the `echoclient` in `examples/echoclient`:

```bash
cargo run -- localhost:8520 N0CALL-C N0CALL-S 200
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

* **Busy channel detection**: This interface relies on the
  ARDOP TNC to perform busy channel detection. Support for
  this functionality varies across ARDOP implementations.

* **ID frames**: ID frames are currently ignored and are not
  processed. This can make it more difficult to discover
  reachable peers.

* **Rig control**: No type of rig control is presently
  integrated. This crate cannot provide the following
  functionality, at present.

  * **Tuning**: `ardop_interface` knows nothing about your rig
    and cannot adjust its frequency or mode.

  * **Keying**: This crate does not key your radio's PTT.
    The ARDOP TNC may be able to do this for you. `ardopc` can
    key transmitters over a serial connection.

  * **Scanning**: The ARDOP frequency agilty / scanning functions
    in `LISTEN` mode are not supported

## Next Steps

What sorts of protocols would you like to see on the air?
With a reliable, TCP-like link between stations, the
ionosphere is the limit!

## Further Reading

* [`tnc`](tnc/index.html): Main TNC command and control
* [`arq`](arq/index.html): Reliable ARQ connections
