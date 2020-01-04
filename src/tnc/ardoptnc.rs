//! Asynchronous ARDOP TNC Interface

use std::convert::Into;
use std::fmt;
use std::net::SocketAddr;
use std::string::String;
use std::sync::Arc;
use std::time::Duration;

use futures::lock::Mutex;

use super::{DiscoveredPeer, PingAck, PingFailedReason, TncResult};

use crate::arq::{ArqStream, ConnectionFailedReason};
use crate::protocol::command;
use crate::protocol::command::Command;
use crate::tncio::asynctnc::{AsyncTncTcp, ConnectionInfoOrPeerDiscovery};

/// TNC Interface
///
/// See the [module-level](index.html) documentation
/// for examples and usage details.
pub struct ArdopTnc {
    inner: Arc<Mutex<AsyncTncTcp>>,
    mycall: String,
    clear_time: Duration,
    clear_max_wait: Duration,
}

/// Result of [`ArdopTnc::listen_monitor()`](../tnc/struct.ArdopTnc.html#method.listen_monitor)
///
/// This value indicates either the
/// * completion of an inbound ARQ connection; or
/// * discovery of a remote peer, via its transmissions
pub enum ListenMonitor {
    /// ARQ Connection
    Connection(ArqStream),

    /// Heard callsign
    PeerDiscovery(DiscoveredPeer),
}

impl ArdopTnc {
    /// Connect to an ARDOP TNC
    ///
    /// Returns a future which will connect to an ARDOP TNC
    /// and initialize it.
    ///
    /// # Parameters
    /// - `addr`: Network address of the ARDOP TNC's control port.
    /// - `mycall`: The formally-assigned callsign for your station.
    ///   Legitimate call signs include from 3 to 7 ASCII characters
    ///   (A-Z, 0-9) followed by an optional "`-`" and an SSID of
    ///   `-0` to `-15` or `-A` to `-Z`. An SSID of `-0` is treated
    ///   as no SSID.
    ///
    /// # Returns
    /// A new `ArdopTnc`, or an error if the connection or
    /// initialization step fails.
    pub async fn new<S>(addr: &SocketAddr, mycall: S) -> TncResult<Self>
    where
        S: Into<String>,
    {
        let mycall = mycall.into();
        let mycall2 = mycall.clone();

        let tnc = AsyncTncTcp::new(addr, mycall2).await?;
        let inner = Arc::new(Mutex::new(tnc));
        Ok(ArdopTnc {
            inner,
            mycall,
            clear_time: DEFAULT_CLEAR_TIME,
            clear_max_wait: DEFAULT_CLEAR_MAX_WAIT,
        })
    }

    /// Get this station's callsign
    ///
    /// # Returns
    /// The formally assigned callsign for this station.
    pub fn mycall(&self) -> &String {
        &self.mycall
    }

    /// Returns configured clear time
    ///
    /// The *clear time* is the duration that the RF channel must
    /// be sensed as clear before an outgoing transmission will be
    /// allowed to proceed.
    ///
    /// See [`set_clear_time()`](#method.set_clear_time).
    pub fn clear_time(&self) -> &Duration {
        &self.clear_time
    }

    /// Returns configured maximum clear waiting time
    ///
    /// Caps the maximum amount of time that a transmitting
    /// Future, such as [`connect()`](#method.connect) or
    /// [`ping()`](#method.ping), will spend waiting for the
    /// channel to become clear.
    ///
    /// If the channel does not become clear within this
    /// Duration, then a `TimedOut` error is raised.
    ///
    /// See [`set_clear_max_wait()`](#method.set_clear_max_wait).
    pub fn clear_max_wait(&self) -> &Duration {
        &self.clear_max_wait
    }

    /// Returns configured clear time
    ///
    /// The *clear time* is the duration that the RF channel must
    /// be sensed as clear before an outgoing transmission will be
    /// allowed to proceed.
    ///
    /// You should **ALWAYS** allow the busy-detection logic ample
    /// time to declare that the channel is clear of any other
    /// communications. The busy detector's sensitivity may be adjusted
    /// via [`set_busydet()`](#method.set_busydet) if it is too sensitive
    /// for your RF environment.
    ///
    /// If you need to send a *DISTRESS* or *EMERGENCY* signal, you may
    /// set `tm` to zero to disable the busy-detection logic entirely.
    ///
    /// # Parameters
    /// - `tm`: New clear time.
    pub fn set_clear_time(&mut self, tm: Duration) {
        self.clear_time = tm;
    }

    /// Returns configured maximum clear waiting time
    ///
    /// Caps the maximum amount of time that a transmitting
    /// Future, such as [`connect()`](#method.connect) or
    /// [`ping()`](#method.ping), will spend waiting for the
    /// channel to become clear.
    ///
    /// If the channel does not become clear within this
    /// Duration, then a `TimedOut` error is raised.
    ///
    /// # Parameters
    /// - `tm`: New timeout for clear-channel waiting. `tm`
    ///   must be at least the
    ///   [`clear_time()`](#method.clear_time).
    pub fn set_clear_max_wait(&mut self, tm: Duration) {
        self.clear_max_wait = tm;
    }

    /// Ping a remote `target` peer
    ///
    /// When run, this future will
    ///
    /// 1. Wait for a clear channel
    /// 2. Send an outgoing `PING` request
    /// 3. Wait for a reply or for the ping timeout to elapse
    /// 4. Complete with the ping result
    ///
    /// # Parameters
    /// - `target`: Peer callsign, with optional `-SSID` portion
    /// - `attempts`: Number of ping packets to send before
    ///   giving up
    ///
    /// # Return
    /// The outer result contains failures related to the local
    /// TNC connection.
    ///
    /// The inner result is the success or failure of the round-trip
    /// ping. If the ping succeeds, returns an `Ok(PingAck)`
    /// with the response from the remote peer. If the ping fails,
    /// returns `Err(PingFailedReason)`. Errors include:
    ///
    /// * `Busy`: The RF channel was busy during the ping attempt,
    ///           and no ping was sent.
    /// * `NoAnswer`: The remote peer did not answer.
    pub async fn ping<S>(
        &mut self,
        target: S,
        attempts: u16,
    ) -> TncResult<Result<PingAck, PingFailedReason>>
    where
        S: Into<String>,
    {
        let mut tnc = self.inner.lock().await;
        tnc.ping(
            target,
            attempts,
            self.clear_time.clone(),
            self.clear_max_wait.clone(),
        )
        .await
    }

    /// Dial a remote `target` peer
    ///
    /// When run, this future will
    ///
    /// 1. Wait for a clear channel.
    /// 2. Make an outgoing `ARQCALL` to the designated callsign
    /// 3. Wait for a connection to either complete or fail
    /// 4. Successful connections will return an
    ///    [`ArqStream`](../arq/struct.ArqStream.html).
    ///
    /// # Parameters
    /// - `target`: Peer callsign, with optional `-SSID` portion
    /// - `bw`: ARQ bandwidth to use
    /// - `bw_forced`: If false, will potentially negotiate for a
    ///   *lower* bandwidth than `bw` with the remote peer. If
    ///   true, the connection will be made at `bw` rate---or not
    ///   at all.
    /// - `attempts`: Number of connection attempts to make
    ///   before giving up
    ///
    /// # Return
    /// The outer result contains failures related to the local
    /// TNC connection.
    ///
    /// The inner result contains failures related to the RF
    /// connection. If the connection attempt succeeds, returns
    /// a new [`ArqStream`](../arq/struct.ArqStream.html) that
    /// can be used like an asynchronous `TcpStream`.
    pub async fn connect<S>(
        &mut self,
        target: S,
        bw: u16,
        bw_forced: bool,
        attempts: u16,
    ) -> TncResult<Result<ArqStream, ConnectionFailedReason>>
    where
        S: Into<String>,
    {
        let mut tnc = self.inner.lock().await;
        match tnc
            .connect(
                target,
                bw,
                bw_forced,
                attempts,
                self.clear_time.clone(),
                self.clear_max_wait.clone(),
            )
            .await?
        {
            Ok(nfo) => Ok(Ok(ArqStream::new(self.inner.clone(), nfo))),
            Err(e) => Ok(Err(e)),
        }
    }

    /// Listen for incoming connections
    ///
    /// When run, this future will wait for the TNC to accept
    /// an incoming connection to `MYCALL` or one of `MYAUX`.
    /// When a connection is accepted, the future will resolve
    /// to an [`ArqStream`](../arq/struct.ArqStream.html).
    ///
    /// # Parameters
    /// - `bw`: Maximum ARQ bandwidth to use
    /// - `bw_forced`: If false, will potentially negotiate for a
    ///   *lower* bandwidth than `bw` with the remote peer. If
    ///   true, the connection will be made at `bw` rate---or not
    ///   at all.
    ///
    /// # Return
    /// The result contains failures related to the local TNC
    /// connection.
    ///
    /// This method will await forever for an inbound connection
    /// to complete. Connections which fail during the setup phase
    /// will not be reported to the application. Unless the local
    /// TNC fails, this method will not fail.
    ///
    /// # Timeouts
    /// This method will await forever, but one can wrap it in a
    /// timeout with the `async_std` crate to make it expire
    /// sooner.
    ///
    /// ```no_run
    /// use std::net::SocketAddr;
    /// use std::time::Duration;
    /// use async_std::prelude::FutureExt;
    /// use futures::prelude::*;
    ///
    /// use ardop_interface::tnc::*;
    ///
    /// fn main() {
    ///    async_std::task::block_on(async {
    ///         let addr = "127.0.0.1:8515".parse().unwrap();
    ///         let mut tnc = ArdopTnc::new(&addr, "MYC4LL")
    ///             .await
    ///             .unwrap();
    ///          match tnc
    ///             .listen(500, false)
    ///             .timeout(Duration::from_secs(30))
    ///             .await {
    ///                 Err(e) => println!("TNC timeout: {}", e),
    ///                 Ok(Err(e)) => println!("Fatal TNC error: {}", e),
    ///                 Ok(Ok(conn)) => println!("Connected: {}", conn)
    ///             }
    ///    })
    /// }
    /// ```
    ///
    /// An expired timeout will return a
    /// [`TncError::TimedOut`](enum.TncError.html#variant.TimedOut).
    pub async fn listen(&mut self, bw: u16, bw_forced: bool) -> TncResult<ArqStream> {
        let mut tnc = self.inner.lock().await;
        let nfo = tnc.listen(bw, bw_forced).await?;
        Ok(ArqStream::new(self.inner.clone(), nfo))
    }

    /// Passively monitor for band activity
    ///
    /// When run, the TNC will listen passively for peers which
    /// announce themselves via:
    ///
    /// * ID Frames (`IDF`)
    /// * Pings
    ///
    /// This method will await forever for a peer to be discovered
    /// See the [`listen()`](#method.listen) method for a futures
    /// extension which adds a timeout.
    ///
    /// The TNC has no memory of discovered stations and will
    /// return a result every time it hears one.
    ///
    /// # Return
    /// The result contains failures related to the local TNC
    /// connection.
    ///
    /// This method will await forever for a peer discovery. Unless
    /// the local TNC fails, this method will not fail. If a peer
    /// is discovered, a [`DiscoveredPeer`](struct.DiscoveredPeer.html)
    /// is returned.
    pub async fn monitor(&mut self) -> TncResult<DiscoveredPeer> {
        let mut tnc = self.inner.lock().await;
        tnc.monitor().await
    }

    /// Listen for incoming connections or for band activity
    ///
    /// This method combines [`listen()`](#method.listen) and
    /// [`monitor()`](#method.monitor). The TNC will listen for
    /// the next inbound ARQ connection OR for band activity and
    /// return the first it finds.
    ///
    /// The incoming connection may be directed at either `MYCALL`
    /// or any of `MYAUX`.
    ///
    /// Band activity will be reported from any available source.
    /// At present, these sources are available:
    ///
    /// * ID Frames
    /// * Ping requests
    ///
    /// # Parameters
    /// - `bw`: Maximum ARQ bandwidth to use. Only applies to
    ///   incoming connections—not peer discoveries.
    /// - `bw_forced`: If false, will potentially negotiate for a
    ///   *lower* bandwidth than `bw` with the remote peer. If
    ///   true, the connection will be made at `bw` rate---or not
    ///   at all.
    ///
    /// # Return
    /// The result contains failures related to the local TNC
    /// connection. The result will also error if this method is
    /// wrapped in a `.timeout()` future, as per
    /// [`listen()`](#method.listen).
    ///
    /// This method will await forever for an inbound connection
    /// to complete. Connections which fail during the setup phase
    /// will not be reported to the application. Unless the local
    /// TNC fails, this method will not fail.
    pub async fn listen_monitor(&mut self, bw: u16, bw_forced: bool) -> TncResult<ListenMonitor> {
        let mut tnc = self.inner.lock().await;
        match tnc.listen_monitor(bw, bw_forced).await? {
            ConnectionInfoOrPeerDiscovery::Connection(nfo) => Ok(ListenMonitor::Connection(
                ArqStream::new(self.inner.clone(), nfo),
            )),
            ConnectionInfoOrPeerDiscovery::PeerDiscovery(peer) => {
                Ok(ListenMonitor::PeerDiscovery(peer))
            }
        }
    }

    /// Send ID frame
    ///
    /// Sends an ID frame immediately, followed by a CW ID
    /// (if `ArdopTnc::set_cwid()` is set)
    ///
    /// Completion of this future does not indicate that the
    /// ID frame has actually been completely transmitted.
    ///
    /// # Return
    /// An empty if an ID frame was/will be sent, or some `TncError`
    /// if an ID frame will not be sent.
    pub async fn sendid(&mut self) -> TncResult<()> {
        self.command(command::sendid()).await?;
        info!("Transmitting ID frame: {}", self.mycall);
        Ok(())
    }

    /// Start a two-tone test
    ///
    /// Send 5 second two-tone burst, at the normal leader
    /// amplitude. May be used in adjusting drive level to
    /// the radio.
    ///
    /// Completion of this future does not indicate that the
    /// two-tone test has actually been completed.
    ///
    /// # Return
    /// An empty if an two-tone test sequence be sent, or some
    /// `TncError` if the test cannot be performed.
    pub async fn twotonetest(&mut self) -> TncResult<()> {
        self.command(command::twotonetest()).await?;
        info!("Transmitting two-tone test");
        Ok(())
    }

    /// Set ARQ connection timeout
    ///
    /// Set the ARQ Timeout in seconds. If no data has flowed in the
    /// channel in `timeout` seconds, the link is declared dead. A `DISC`
    /// command is sent and a reset to the `DISC` state is initiated.
    ///
    /// If either end of the ARQ session hits it’s `ARQTIMEOUT` without
    /// data flow the link will automatically be terminated.
    ///
    /// # Parameters
    /// - `timeout`: ARQ timeout period, in seconds (30 -- 600)
    ///
    /// # Return
    /// An empty on success or a `TncError` if the command could not
    /// be completed for any reason, including timeouts.
    pub async fn set_arqtimeout(&mut self, timeout: u16) -> TncResult<()> {
        self.command(command::arqtimeout(timeout)).await
    }

    /// Busy detector threshold value
    ///
    /// Sets the current Busy detector threshold value (default = 5). The
    /// default value should be sufficient for most installations. Lower
    /// values will make the busy detector more sensitive; the channel will
    /// be declared busy *more frequently*. Higher values may be used for
    /// high-noise environments.
    ///
    /// # Parameters
    /// - `level`: Busy detector threshold (0 -- 10). A value of 0 will disable
    ///   the busy detector (not recommended).
    ///
    /// # Return
    /// An empty on success or a `TncError` if the command could not
    /// be completed for any reason, including timeouts.
    pub async fn set_busydet(&mut self, level: u16) -> TncResult<()> {
        self.command(command::busydet(level)).await
    }

    /// Send CW after ID frames
    ///
    /// Set to true to send your callsign in morse code (CW), as station ID,
    /// at the end of every ID frame. In many regions, a CW ID is always
    /// sufficient to meet station ID requirements. Some regions may
    /// require it.
    ///
    /// # Parameters
    /// - `cw`: Send CW ID with ARDOP digital ID frames
    ///
    /// # Return
    /// An empty on success or a `TncError` if the command could not
    /// be completed for any reason, including timeouts.
    pub async fn set_cwid(&mut self, set_cwid: bool) -> TncResult<()> {
        self.command(command::cwid(set_cwid)).await
    }

    /// Set your station's grid square
    ///
    /// Sets the 4, 6, or 8-character Maidenhead Grid Square for your
    /// station. A correct grid square is useful for studying and
    /// logging RF propagation-and for bragging rights.
    ///
    /// Your grid square will be sent in ID frames.
    ///
    /// # Parameters
    /// - `grid`: Your grid square (4, 6, or 8-characters).
    ///
    /// # Return
    /// An empty on success or a `TncError` if the command could not
    /// be completed for any reason, including timeouts.
    pub async fn set_gridsquare<S>(&mut self, grid: S) -> TncResult<()>
    where
        S: Into<String>,
    {
        self.command(command::gridsquare(grid)).await
    }

    /// Leader tone duration
    ///
    /// Sets the leader length in ms. (Default is 160 ms). Rounded to
    /// the nearest 20 ms. Note for VOX keying or some SDR radios the
    /// leader may have to be extended for reliable decoding.
    ///
    /// # Parameters
    /// - `duration`: Leader tone duration, milliseconds
    ///
    /// # Return
    /// An empty on success or a `TncError` if the command could not
    /// be completed for any reason, including timeouts.
    pub async fn set_leader(&mut self, duration: u16) -> TncResult<()> {
        self.command(command::leader(duration)).await
    }

    /// Set your station's auxiliary callsigns
    ///
    /// `MYAUX` is only used for `LISTEN`ing, and it will not be used for
    /// connect requests.
    ///
    /// Legitimate call signs include from 3 to 7 ASCII characters (A-Z, 0-9)
    /// followed by an optional "`-`" and an SSID of `-0` to `-15` or `-A`
    /// to `-Z`. An SSID of `-0` is treated as no SSID.
    ///
    /// # Parameters:
    /// - `aux`: Vector of auxiliary callsigns. If empty, all aux callsigns
    ///   will be removed.
    ///
    /// # Return
    /// An empty on success or a `TncError` if the command could not
    /// be completed for any reason, including timeouts.
    pub async fn set_myaux(&mut self, aux: Vec<String>) -> TncResult<()> {
        self.command(command::myaux(aux)).await
    }

    /// Query TNC version
    ///
    /// Queries the ARDOP TNC software for its version number.
    /// The format of the version number is unspecified and may
    /// be empty.
    ///
    /// # Return
    /// Version string, or an error if the version string could
    /// not be retrieved.
    pub async fn version(&mut self) -> TncResult<String> {
        let mut tnc = self.inner.lock().await;
        tnc.version().await
    }

    /// Gets the control connection timeout value
    ///
    /// Commands sent via the `command()` method will
    /// timeout if either the send or receive takes
    /// longer than `timeout`.
    ///
    /// Timeouts cause `TncError::IoError`s of kind
    /// `io::ErrorKind::TimedOut`. Control timeouts
    /// usually indicate a serious problem with the ARDOP
    /// TNC or its connection.
    ///
    /// # Returns
    /// Current timeout value
    pub async fn control_timeout(&self) -> Duration {
        self.inner.lock().await.control_timeout().clone()
    }

    /// Sets timeout for the control connection
    ///
    /// Commands sent via the `command()` method will
    /// timeout if either the send or receive takes
    /// longer than `timeout`.
    ///
    /// Timeouts cause `TncError::IoError`s of kind
    /// `io::ErrorKind::TimedOut`. Control timeouts
    /// usually indicate a serious problem with the ARDOP
    /// TNC or its connection.
    ///
    /// # Parameters
    /// - `timeout`: New command timeout value
    pub async fn set_control_timeout(&mut self, timeout: Duration) {
        let mut tnc = self.inner.lock().await;
        tnc.set_control_timeout(timeout)
    }

    // Send a command to the TNC and await the response
    //
    // A future which will send the given command and wait
    // for a success or failure response. The waiting time
    // is upper-bounded by AsyncTnc's `control_timeout()`
    // value.
    //
    // # Parameters
    // - `cmd`: The Command to send
    //
    // # Returns
    // An empty on success or a `TncError`.
    async fn command<F>(&mut self, cmd: Command<F>) -> TncResult<()>
    where
        F: fmt::Display,
    {
        let mut tnc = self.inner.lock().await;
        tnc.command(cmd).await
    }
}

const DEFAULT_CLEAR_TIME: Duration = Duration::from_secs(10);
const DEFAULT_CLEAR_MAX_WAIT: Duration = Duration::from_secs(90);
