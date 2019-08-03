//! Asynchronous ARDOP TNC backend
//!
//! This module contains the "meat" of the ARDOP TNC
//! interface. This object is asynchronous and returns
//! futures for all blocking operations. It also includes
//! management of ARQ connections.
//!
//! Higher-level objects will separate control functions
//! and ARQ connections into separate objects.
//!
use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::string::String;
use std::time::{Duration, Instant};

use futures::io::{AsyncRead, AsyncWrite};
use futures::sink::{Sink, SinkExt};
use futures::stream::{Stream, StreamExt};
use futures::task::{noop_waker_ref, Context, Poll};
use futures_codec::Framed;

use runtime::net::TcpStream;
use runtime::time::FutureExt;

use super::controlstream;
use super::controlstream::{ControlSink, ControlStreamEvents, ControlStreamResults};
use super::data::{DataIn, DataOut};
use super::dataevent::{DataEvent, DataEventStream};

use crate::arq::{ConnectionFailedReason, ConnectionInfo};
use crate::framing::data::TncDataFraming;
use crate::protocol::command;
use crate::protocol::command::Command;
use crate::protocol::constants::{CommandID, ProtocolMode};
use crate::protocol::response::{CommandOk, CommandResult, ConnectionStateChange};
use crate::tnc::{DiscoveredPeer, PingAck, PingFailedReason, TncError, TncResult};

// Offset between control port and data port
const DATA_PORT_OFFSET: u16 = 1;

// Default timeout for local TNC commands
const DEFAULT_TIMEOUT_COMMAND: Duration = Duration::from_millis(20000);

// Timeout for async disconnect
const TIMEOUT_DISCONNECT: Duration = Duration::from_secs(60);

// Ping timeout, per ping sent
const TIMEOUT_PING: Duration = Duration::from_secs(5);

/// The output of `listen_monitor()`
pub enum ConnectionInfoOrPeerDiscovery {
    Connection(ConnectionInfo),
    PeerDiscovery(DiscoveredPeer),
}

/// Asynchronous ARDOP TNC
///
/// This object communicates with the ARDOP program
/// via TCP, and it holds all I/O resources for this
/// task.
pub struct AsyncTnc<I>
where
    I: AsyncRead + AsyncWrite + Unpin + Send,
{
    data_stream: DataEventStream<Framed<I, TncDataFraming>, ControlStreamEvents<I>>,
    control_in_res: ControlStreamResults<I>,
    control_out: ControlSink<I>,
    control_timeout: Duration,
    disconnect_progress: DisconnectProgress,
}

/// Type specialization for public API
pub type AsyncTncTcp = AsyncTnc<TcpStream>;

impl<I: 'static> AsyncTnc<I>
where
    I: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Connect to an ARDOP TNC
    ///
    /// Returns a future which will connect to an ARDOP TNC
    /// and initialize it.
    ///
    /// # Parameters
    /// - `control_addr`: Network address of the ARDOP TNC's
    ///   control port.
    /// - `mycall`: The formally-assigned callsign for your station.
    ///   Legitimate call signs include from 3 to 7 ASCII characters
    ///   (A-Z, 0-9) followed by an optional "`-`" and an SSID of
    ///   `-0` to `-15` or `-A` to `-Z`. An SSID of `-0` is treated
    ///   as no SSID.
    ///
    /// # Returns
    /// A new `AsyncTnc`, or an error if the connection or
    /// initialization step fails.
    pub async fn new<S>(control_addr: &SocketAddr, mycall: S) -> TncResult<AsyncTnc<TcpStream>>
    where
        S: Into<String>,
    {
        let data_addr = SocketAddr::new(control_addr.ip(), control_addr.port() + DATA_PORT_OFFSET);

        // connect
        let stream_control: TcpStream = TcpStream::connect(control_addr).await?;
        let stream_data: TcpStream = TcpStream::connect(&data_addr).await?;

        let mut out = AsyncTnc::new_from_streams(stream_control, stream_data, mycall);

        // Try to initialize the TNC. If we fail here, we will bail
        // with an error instead.
        match out.initialize().await {
            Ok(()) => {
                info!("Initialized ARDOP TNC at {}", &control_addr);
                Ok(out)
            }
            Err(e) => {
                error!(
                    "Unable to initialize ARDOP TNC at {}: {}",
                    &control_addr, &e
                );
                Err(e)
            }
        }
    }

    /// New from raw I/O types
    ///
    /// # Parameters
    /// - `stream_control`: I/O stream to the TNC's control port
    /// - `stream_data`: I/O stream to the TNC's data port
    /// - `mycall`: The formally-assigned callsign for your station.
    ///   Legitimate call signs include from 3 to 7 ASCII characters
    ///   (A-Z, 0-9) followed by an optional "`-`" and an SSID of
    ///   `-0` to `-15` or `-A` to `-Z`. An SSID of `-0` is treated
    ///   as no SSID.
    ///
    /// # Returns
    /// A new `AsyncTnc`.
    pub(crate) fn new_from_streams<S>(stream_control: I, stream_data: I, mycall: S) -> AsyncTnc<I>
    where
        S: Into<String>,
    {
        // create receiver streams for the control port
        let (control_in_evt, control_in_res, control_out) =
            controlstream::controlstream(stream_control);

        // create data port input/output framer
        let data_inout = Framed::new(stream_data, TncDataFraming::new());

        AsyncTnc {
            data_stream: DataEventStream::new(mycall, data_inout, control_in_evt),
            control_in_res,
            control_out,
            control_timeout: DEFAULT_TIMEOUT_COMMAND,
            disconnect_progress: DisconnectProgress::NoProgress,
        }
    }

    /// Get this station's callsign
    ///
    /// # Returns
    /// The formally assigned callsign for this station.
    pub fn mycall(&self) -> &String {
        self.data_stream.state().mycall()
    }

    /// Gets the control connection timeout value
    ///
    /// Commands sent via the `command()` method will
    /// timeout if either the send or receive takes
    /// longer than `timeout`.
    ///
    /// Command timeouts cause an `TncError::IoError`
    /// of type `io::ErrorKind::TimedOut`. This error
    /// indicates that the socket connection is likely dead.
    ///
    /// # Returns
    /// Current timeout value
    pub fn control_timeout(&self) -> &Duration {
        &self.control_timeout
    }

    /// Sets timeout for the control connection
    ///
    /// Commands sent via the `command()` method will
    /// timeout if either the send or receive takes
    /// longer than `timeout`.
    ///
    /// Command timeouts cause an `TncError::IoError`
    /// of type `io::ErrorKind::TimedOut`. This error
    /// indicates that the socket connection is likely dead.
    ///
    /// # Parameters
    /// - `timeout`: New command timeout value
    pub fn set_control_timeout(&mut self, timeout: Duration) {
        self.control_timeout = timeout;
    }

    /// Events and data stream, for both incoming and outgoing data
    ///
    /// The stream emits both connection-relevant events and
    /// data received from remote peers.
    ///
    /// The sink accepts data for transmission over the air.
    /// Each FEC transmission must be sent in one call. ARQ
    /// data may be sent in a streaming manner.
    ///
    /// # Return
    /// Stream + Sink reference
    pub fn data_stream_sink(
        &mut self,
    ) -> &mut (impl Stream<Item = DataEvent> + Sink<DataOut, Error = io::Error> + Unpin) {
        &mut self.data_stream
    }

    /// Send a command to the TNC and await the response
    ///
    /// A future which will send the given command and wait
    /// for a success or failure response. The waiting time
    /// is upper-bounded by AsyncTnc's `control_timeout()`
    /// value.
    ///
    /// # Parameters
    /// - `cmd`: The Command to send
    ///
    /// # Returns
    /// An empty on success or a `TncError`.
    pub async fn command<F>(&mut self, cmd: Command<F>) -> TncResult<()>
    where
        F: fmt::Display,
    {
        match execute_command(
            &mut self.control_out,
            &mut self.control_in_res,
            &self.control_timeout,
            cmd,
        )
        .await
        {
            Ok(cmdok) => {
                debug!("Command {} OK", cmdok.0);
                Ok(())
            }
            Err(e) => {
                warn!("Command failed: {}", &e);
                Err(e)
            }
        }
    }

    /// Wait for a clear channel
    ///
    /// Waits for the RF channel to become clear, according
    /// to the TNC's busy-channel detection logic. This method
    /// will await for at most `max_wait` for the RF channel to
    /// be clear for at least `clear_time`.
    ///
    /// # Parameters
    /// - `clear_time`: Time that the channel must be clear. If
    ///    zero, busy-detection logic is disabled.
    /// - `max_wait`: Wait no longer than this time for a clear
    ///   channel.
    ///
    /// # Returns
    /// If `max_wait` elapses before the channel becomes clear,
    /// returns `TncError::TimedOut`. If the channel has become
    /// clear, and has remained clear for `clear_time`, then
    /// returns `Ok`.
    pub async fn await_clear(&mut self, clear_time: Duration, max_wait: Duration) -> TncResult<()> {
        if clear_time == Duration::from_secs(0) {
            warn!("Busy detector is DISABLED. Assuming channel is clear.");
            return Ok(());
        }

        // Consume all events which are available, but don't
        // actually wait.
        let mut ctx = Context::from_waker(noop_waker_ref());
        loop {
            match Pin::new(&mut self.data_stream).poll_next(&mut ctx) {
                Poll::Pending => break,
                Poll::Ready(None) => return Err(connection_reset_err().into()),
                Poll::Ready(Some(_evt)) => continue,
            }
        }

        // Determine if we have been clear for long enough.
        // If so, we're done.
        if self.data_stream.state().clear_time() > clear_time {
            info!("Channel is CLEAR and READY for use at +0.0 seconds");
            return Ok(());
        }

        info!(
            "Waiting for a clear channel: {:0.1} seconds within {:0.1} seconds...",
            clear_time.as_millis() as f32 / 1000.0f32,
            max_wait.as_millis() as f32 / 1000.0f32
        );

        // Until we time out...
        let wait_start = Instant::now();
        loop {
            // how much longer can we wait?
            let wait_elapsed = wait_start.elapsed();
            if wait_elapsed >= max_wait {
                info!("Timed out while waiting for clear channel.");
                return Err(TncError::TimedOut);
            }
            let mut wait_remaining = max_wait - wait_elapsed;

            // if we are clear, we only need to wait until our clear_time
            if !self.data_stream.state().busy() {
                let clear_elapsed = self.data_stream.state().clear_time();
                if clear_elapsed >= clear_time {
                    info!(
                        "Channel is CLEAR and READY for use at +{:0.1} seconds",
                        wait_elapsed.as_millis() as f32 / 1000.0f32
                    );
                    break;
                }
                let clear_remaining = clear_time - clear_elapsed;
                wait_remaining = wait_remaining.min(clear_remaining)
            }

            // wait for next state transition
            match self.next_state_change_timeout(wait_remaining).await {
                Err(TncError::TimedOut) => continue,
                Err(_e) => return Err(_e),
                Ok(ConnectionStateChange::Busy(busy)) => {
                    if busy {
                        info!(
                            "Channel is BUSY at +{:0.1} seconds",
                            wait_start.elapsed().as_millis() as f32 / 1000.0f32
                        );
                    } else {
                        info!(
                            "Channel is CLEAR at +{:0.1} seconds",
                            wait_start.elapsed().as_millis() as f32 / 1000.0f32
                        );
                    }
                }
                _ => continue,
            }
        }

        Ok(())
    }

    /// Ping a remote `target` peer
    ///
    /// When run, this future will
    ///
    /// 1. Wait for a clear channel
    /// 2. Send an outgoing `PING` request
    /// 3. Wait for a reply or for the ping timeout to elapse
    ///
    /// # Parameters
    /// - `target`: Peer callsign, with optional `-SSID` portion
    /// - `attempts`: Number of ping packets to send before
    ///   giving up
    /// - `clear_time`: Minimum time channel must be clear. If zero,
    ///   busy channel detection is disabled.
    /// - `clear_max_wait`: Maximum time user is willing to wait for
    ///   a clear channel.
    ///
    /// # Return
    /// The outer result contains failures related to the local
    /// TNC connection.
    ///
    /// The inner result is the success or failure of the ping
    /// operation. If the ping succeeds, returns an `Ok(PingAck)`
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
        clear_time: Duration,
        clear_max_wait: Duration,
    ) -> TncResult<Result<PingAck, PingFailedReason>>
    where
        S: Into<String>,
    {
        if attempts <= 0 {
            return Ok(Err(PingFailedReason::NoAnswer));
        }

        // Wait for clear channel
        match self.await_clear(clear_time, clear_max_wait).await {
            Ok(_ok) => {}
            Err(TncError::TimedOut) => return Ok(Err(PingFailedReason::Busy)),
            Err(e) => return Err(e),
        }

        // Send the ping
        let target_string = target.into();
        info!("Pinging {} ({} attempts)...", &target_string, attempts);
        self.command(command::ping(target_string.clone(), attempts))
            .await?;

        // The ping will expire at timeout_seconds in the future
        let timeout_seconds = attempts as u64 * TIMEOUT_PING.as_secs();
        let start = Instant::now();

        loop {
            match self.next_state_change_timeout(TIMEOUT_PING.clone()).await {
                Err(TncError::TimedOut) => { /* ignore */ }
                Ok(ConnectionStateChange::PingAck(snr, quality)) => {
                    let ack = PingAck::new(target_string, snr, quality);
                    info!("{}", &ack);
                    return Ok(Ok(ack));
                }
                Err(e) => return Err(e),
                _ => { /* ignore */ }
            }

            if start.elapsed().as_secs() >= timeout_seconds {
                // ping timeout
                break;
            }
        }

        info!("Ping {}: ping timeout", &target_string);
        Ok(Err(PingFailedReason::NoAnswer))
    }

    /// Dial a remote `target` peer
    ///
    /// When run, this future will
    ///
    /// 1. Wait for a clear channel
    /// 2. Make an outgoing `ARQCALL` to the designated callsign
    /// 3. Wait for a connection to either complete or fail
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
    /// - `clear_time`: Minimum time channel must be clear If zero,
    ///   busy channel detection is disabled.
    /// - `clear_max_wait`: Maximum time user is willing to wait for
    ///   a clear channel.
    ///
    /// # Return
    /// The outer result contains failures related to the local
    /// TNC connection.
    ///
    /// The inner result contains failures related to the RF
    /// connection. If the connection attempt succeeds, returns
    /// information about the connection. If the attempt fails,
    /// returns the failure reason.
    pub async fn connect<S>(
        &mut self,
        target: S,
        bw: u16,
        bw_forced: bool,
        attempts: u16,
        clear_time: Duration,
        clear_max_wait: Duration,
    ) -> TncResult<Result<ConnectionInfo, ConnectionFailedReason>>
    where
        S: Into<String>,
    {
        let target_string = target.into();

        // wait for clear channel
        match self.await_clear(clear_time, clear_max_wait).await {
            Ok(_clear) => { /* no-op*/ }
            Err(TncError::TimedOut) => return Ok(Err(ConnectionFailedReason::Busy)),
            Err(e) => return Err(e),
        }

        // configure the ARQ mode
        self.command(command::listen(false)).await?;
        self.command(command::protocolmode(ProtocolMode::ARQ))
            .await?;
        self.command(command::arqbw(bw, bw_forced)).await?;

        // dial
        //
        // success here merely indicates that a connect request is
        // in-flight
        info!(
            "Connecting to {}: dialing at {} Hz BW...",
            &target_string, bw
        );
        match self
            .command(command::arqcall(target_string.clone(), attempts))
            .await
        {
            Ok(()) => { /* no-op */ }
            Err(e) => {
                error!(
                    "Connection to {} failed: TNC rejected request: {}.",
                    &target_string, &e
                );
                return Err(e);
            }
        }

        // wait for success or failure of the connection
        loop {
            match self.next_state_change().await? {
                ConnectionStateChange::Connected(info) => {
                    info!("CONNECTED {}", &info);
                    return Ok(Ok(info));
                }
                ConnectionStateChange::Failed(fail) => {
                    info!("Connection to {} failed: {}", &target_string, &fail);
                    return Ok(Err(fail));
                }
                ConnectionStateChange::Closed => {
                    info!("Connection to {} failed: not connected", &target_string);
                    return Ok(Err(ConnectionFailedReason::NoAnswer));
                }
                _ => { /* ignore */ }
            }
        }
    }

    /// Listen for incoming connections
    ///
    /// When run, this future will wait for the TNC to accept
    /// an incoming connection to `MYCALL` or one of `MYAUX`.
    /// When a connection is accepted, the future will resolve
    /// to a `ConnectionInfo`.
    ///
    /// # Parameters
    /// - `bw`: ARQ bandwidth to use
    /// - `bw_forced`: If false, will potentially negotiate for a
    ///   *lower* bandwidth than `bw` with the remote peer. If
    ///   true, the connection will be made at `bw` rate---or not
    ///   at all.
    ///
    /// # Return
    /// The outer result contains failures related to the local
    /// TNC connection.
    ///
    /// This method will await forever for an inbound connection
    /// to complete. Unless the local TNC fails, this method will
    /// not fail.
    pub async fn listen(&mut self, bw: u16, bw_forced: bool) -> TncResult<ConnectionInfo> {
        loop {
            match self.listen_monitor(bw, bw_forced).await? {
                ConnectionInfoOrPeerDiscovery::Connection(conn_info) => return Ok(conn_info),
                _ => continue,
            }
        }
    }

    /// Passively monitor for peers
    ///
    /// When run, the TNC will listen passively for peers which
    /// announce themselves via:
    ///
    /// * ID Frames (`IDF`)
    /// * Pings
    ///
    /// If such an announcement is heard, the future will return
    /// a DiscoveredPeer.
    ///
    /// # Return
    /// The outer result contains failures related to the local
    /// TNC connection.
    ///
    /// This method will await forever for a peer broadcast. Unless
    /// the local TNC fails, this method will not fail.
    pub async fn monitor(&mut self) -> TncResult<DiscoveredPeer> {
        // disable listening
        self.command(command::protocolmode(ProtocolMode::ARQ))
            .await?;
        self.command(command::listen(true)).await?;

        info!("Monitoring for available peers...");

        // wait for peer transmission
        loop {
            match self.next_state_change().await? {
                ConnectionStateChange::IdentityFrame(call, grid) => {
                    let peer = DiscoveredPeer::new(call, None, grid);
                    info!("ID frame: {}", peer);
                    return Ok(peer);
                }
                ConnectionStateChange::Ping(src, _dst, snr, _quality) => {
                    let peer = DiscoveredPeer::new(src, Some(snr), None);
                    info!("Ping: {}", peer);
                    return Ok(peer);
                }
                _ => continue,
            }
        }
    }

    /// Listen for incoming connections and peer identities
    ///
    /// When run, this future will wait for the TNC to accept
    /// an incoming connection to `MYCALL` or one of `MYAUX`.
    /// When a connection is accepted, the future will resolve
    /// to a `ConnectionInfo`. This method will also listen for
    /// beacons (ID frames) and pings and return information
    /// about discovered peers.
    ///
    /// # Parameters
    /// - `bw`: ARQ bandwidth to use
    /// - `bw_forced`: If false, will potentially negotiate for a
    ///   *lower* bandwidth than `bw` with the remote peer. If
    ///   true, the connection will be made at `bw` rate---or not
    ///   at all.
    ///
    /// # Return
    /// The outer result contains failures related to the local
    /// TNC connection.
    ///
    /// This method will await forever for an inbound connection
    /// to complete or for a peer identity to be received. Unless
    /// the local TNC fails, this method will not fail.
    pub async fn listen_monitor(
        &mut self,
        bw: u16,
        bw_forced: bool,
    ) -> TncResult<ConnectionInfoOrPeerDiscovery> {
        // configure the ARQ mode and start listening
        self.command(command::protocolmode(ProtocolMode::ARQ))
            .await?;
        self.command(command::arqbw(bw, bw_forced)).await?;
        self.command(command::listen(true)).await?;

        info!("Listening for {} at {} Hz...", self.mycall(), bw);

        // wait until we connect
        loop {
            match self.next_state_change().await? {
                ConnectionStateChange::Connected(info) => {
                    info!("CONNECTED {}", &info);
                    self.command(command::listen(false)).await?;
                    return Ok(ConnectionInfoOrPeerDiscovery::Connection(info));
                }
                ConnectionStateChange::Failed(fail) => {
                    info!("Incoming connection failed: {}", fail);
                }
                ConnectionStateChange::IdentityFrame(call, grid) => {
                    let peer = DiscoveredPeer::new(call, None, grid);
                    info!("ID frame: {}", peer);
                    return Ok(ConnectionInfoOrPeerDiscovery::PeerDiscovery(peer));
                }
                ConnectionStateChange::Ping(src, _dst, snr, _quality) => {
                    let peer = DiscoveredPeer::new(src, Some(snr), None);
                    info!("Ping: {}", peer);
                    return Ok(ConnectionInfoOrPeerDiscovery::PeerDiscovery(peer));
                }
                ConnectionStateChange::Closed => {
                    info!("Incoming connection failed: not connected");
                }
                _ => continue,
            }
        }
    }

    /// Disconnect any in-progress ARQ connection
    ///
    /// When this future has returned, the ARDOP TNC has
    /// disconnected from any in-progress ARQ session.
    /// This method is safe to call even when no
    /// connection is in progress. Any errors which
    /// result from the disconnection process are ignored.
    pub async fn disconnect(&mut self) {
        match self.command(command::disconnect()).await {
            Ok(()) => { /* no-op */ }
            Err(_e) => return,
        };

        for _i in 0..2 {
            match self
                .next_state_change_timeout(TIMEOUT_DISCONNECT.clone())
                .await
            {
                Err(_timeout) => {
                    // disconnect timeout; try to abort
                    warn!("Disconnect timed out. Trying to abort.");
                    let _ = self.command(command::abort()).await;
                    continue;
                }
                Ok(ConnectionStateChange::Closed) => {
                    break;
                }
                _ => { /* no-op */ }
            }
        }
    }

    /// Perform disconnect by polling
    ///
    /// A polling method for disconnecting ARQ sessions. Call
    /// this method until it returns `Ready` `Ok` to disconnect
    /// any in-progress ARQ connection.
    ///
    /// If a task execution context is available, the async
    /// method `AsyncTnc::disconnect()` may be a better
    /// alternative to this method.
    ///
    /// # Parameters
    /// - `cx`: Polling context
    ///
    /// # Return
    /// * `Poll::Pending` if this method is waiting on I/O and
    ///   needs to be re-polled
    /// * `Poll::Ready(Ok(()))` if any in-progress ARQ connection
    ///   is now disconnected
    /// * `Poll::Ready(Err(e))` if the connection to the local
    ///   ARDOP TNC has been interrupted.
    pub fn poll_disconnect(&mut self, cx: &mut Context) -> Poll<io::Result<()>> {
        match ready!(execute_disconnect(
            &mut self.disconnect_progress,
            cx,
            &mut self.control_out,
            &mut self.control_in_res,
            &mut self.data_stream,
        )) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(TncError::IoError(e)) => Poll::Ready(Err(e)),
            Err(_tnc_err) => Poll::Ready(Err(connection_reset_err())),
        }
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
        let cmd = command::version();
        let vers = execute_command(
            &mut self.control_out,
            &mut self.control_in_res,
            &self.control_timeout,
            cmd,
        )
        .await?;
        match vers.1 {
            None => Ok("".to_owned()),
            Some(v) => Ok(v),
        }
    }

    // Initialize the ARDOP TNC
    async fn initialize(&mut self) -> TncResult<()> {
        self.data_stream.state_mut().reset();
        self.command(command::initialize()).await?;
        self.command(command::listen(false)).await?;
        self.command(command::protocolmode(ProtocolMode::FEC))
            .await?;
        self.command(command::mycall(self.mycall().clone())).await?;
        self.command(command::busyblock(true)).await?;
        Ok(())
    }

    // wait for a connection state change
    async fn next_state_change(&mut self) -> TncResult<ConnectionStateChange> {
        self.next_state_change_timeout(Duration::from_secs(0)).await
    }

    // wait for a connection state change (specified timeout, zero for infinite)
    async fn next_state_change_timeout(
        &mut self,
        timeout: Duration,
    ) -> TncResult<ConnectionStateChange> {
        loop {
            let res = if timeout == Duration::from_secs(0) {
                self.data_stream.next().await
            } else {
                self.data_stream.next().timeout(timeout.clone()).await?
            };
            match res {
                None => return Err(TncError::IoError(connection_reset_err())),
                Some(DataEvent::Event(event)) => return Ok(event),
                Some(DataEvent::Data(DataIn::IDF(peer_call, peer_grid))) => {
                    return Ok(ConnectionStateChange::IdentityFrame(peer_call, peer_grid))
                }
                Some(_data) => { /* consume it */ }
            }
        }
    }
}

impl<I> Unpin for AsyncTnc<I> where I: AsyncRead + AsyncWrite + Unpin + Send {}

// Future which executes a command on the TNC
//
// Sends a command to the TNC and awaits the result.
// The caller will wait no longer than `timeout`.
async fn execute_command<'d, W, R, F>(
    outp: &'d mut W,
    inp: &'d mut R,
    timeout: &'d Duration,
    cmd: Command<F>,
) -> TncResult<CommandOk>
where
    W: Sink<String> + Unpin,
    R: Stream<Item = CommandResult> + Unpin,
    F: fmt::Display,
{
    let send_raw = cmd.to_string();
    debug!("Sending TNC command: {}", &send_raw);

    // send
    let _ = outp.send(send_raw).timeout(timeout.clone()).await?;

    match inp.next().timeout(timeout.clone()).await {
        // timeout elapsed
        Err(_timeout) => Err(TncError::IoError(connection_timeout_err())),
        // lost connection
        Ok(None) => Err(TncError::IoError(connection_reset_err())),
        // TNC FAULT means our command has failed
        Ok(Some(Err(badcmd))) => Err(TncError::CommandFailed(badcmd)),
        Ok(Some(Ok((in_id, msg)))) => {
            if in_id == *cmd.command_id() {
                // success
                Ok((in_id, msg))
            } else {
                // success, but this isn't the command we are looking for
                Err(TncError::IoError(command_response_invalid_err()))
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
enum DisconnectProgress {
    NoProgress,
    SentDisconnect,
    AckDisconnect,
}

// A polling function which drives the TNC to disconnection
//
// The AsyncWrite trait requires a polling disconnect. We can't
// just run a future and be done with it. Rather than using all
// of our fancy async methods, we have to write all the polling
// logic to do disconnects "by hand."
//
// This method will return Pending while a disconnect is waiting
// on I/O. When it returns Ready(Ok(())), the disconnect is
// finished.
fn execute_disconnect<K, S, E, Z>(
    state: &mut DisconnectProgress,
    cx: &mut Context<'_>,
    ctrl_out: &mut K,
    ctrl_in: &mut S,
    evt_in: &mut E,
) -> Poll<TncResult<()>>
where
    K: Sink<String, Error = Z> + Unpin,
    S: Stream<Item = CommandResult> + Unpin,
    E: Stream<Item = DataEvent> + Unpin,
    crate::tnc::TncError: std::convert::From<Z>,
{
    loop {
        match state {
            DisconnectProgress::NoProgress => {
                // ready to send command?
                ready!(Pin::new(&mut *ctrl_out).poll_ready(cx))?;

                // send it
                *state = DisconnectProgress::SentDisconnect;
                Pin::new(&mut *ctrl_out).start_send(format!("{}", command::disconnect()))?;
            }
            DisconnectProgress::SentDisconnect => {
                // try to flush the outgoing command
                ready!(Pin::new(&mut *ctrl_out).poll_flush(cx))?;

                // read the next command response
                match ready!(Pin::new(&mut *ctrl_in).poll_next(cx)) {
                    None => {
                        // I/O error
                        *state = DisconnectProgress::NoProgress;
                        return Poll::Ready(Err(TncError::IoError(connection_reset_err())));
                    }
                    Some(Err(_e)) => {
                        // probably already disconnected
                        *state = DisconnectProgress::NoProgress;
                        return Poll::Ready(Ok(()));
                    }
                    Some(Ok((CommandID::DISCONNECT, _msg))) => {
                        // Command execution in progress.
                        *state = DisconnectProgress::AckDisconnect;
                    }
                    Some(_ok) => {
                        // not the right command response -- keep looking
                    }
                }
            }
            DisconnectProgress::AckDisconnect => {
                match ready!(Pin::new(&mut *evt_in).poll_next(cx)) {
                    None => {
                        *state = DisconnectProgress::NoProgress;
                        return Poll::Ready(Err(TncError::IoError(connection_reset_err())));
                    }
                    Some(DataEvent::Event(ConnectionStateChange::Closed)) => {
                        *state = DisconnectProgress::NoProgress;
                        return Poll::Ready(Ok(()));
                    }
                    Some(_dataevt) => { /* no-op; keep waiting */ }
                }
            }
        }
    }
}

fn connection_reset_err() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionReset,
        "Lost connection to ARDOP TNC",
    )
}

fn connection_timeout_err() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "Lost connection to ARDOP TNC: command timed out",
    )
}

fn command_response_invalid_err() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "TNC sent an unsolicited or invalid command response",
    )
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    use futures::channel::mpsc;
    use futures::sink;
    use futures::stream;
    use futures::task;

    use crate::protocol::constants::CommandID;

    #[runtime::test]
    async fn test_execute_command_good_response() {
        let cmd_out = command::listen(true);
        let res_in: Vec<CommandResult> = vec![Ok((CommandID::LISTEN, None))];

        let mut sink_out = sink::drain();
        let mut stream_in = stream::iter(res_in.into_iter());
        let timeout = Duration::from_secs(10);

        let res = execute_command(&mut sink_out, &mut stream_in, &timeout, cmd_out).await;

        match res {
            Ok((CommandID::LISTEN, None)) => assert!(true),
            _ => assert!(false),
        }
    }

    #[runtime::test]
    async fn test_execute_command_bad_response() {
        let cmd_out = command::listen(true);
        let res_in: Vec<CommandResult> = vec![Ok((CommandID::VERSION, None))];

        let mut sink_out = sink::drain();
        let mut stream_in = stream::iter(res_in.into_iter());
        let timeout = Duration::from_secs(10);

        let res = execute_command(&mut sink_out, &mut stream_in, &timeout, cmd_out).await;
        match res {
            Err(TncError::IoError(e)) => assert_eq!(io::ErrorKind::InvalidData, e.kind()),
            _ => assert!(false),
        }
    }

    #[runtime::test]
    async fn test_execute_command_eof() {
        let cmd_out = command::listen(true);
        let res_in: Vec<CommandResult> = vec![];

        let mut sink_out = sink::drain();
        let mut stream_in = stream::iter(res_in.into_iter());
        let timeout = Duration::from_secs(10);

        let res = execute_command(&mut sink_out, &mut stream_in, &timeout, cmd_out).await;
        match res {
            Err(TncError::IoError(e)) => assert_eq!(e.kind(), io::ErrorKind::ConnectionReset),
            _ => assert!(false),
        }
    }

    #[runtime::test]
    async fn test_execute_command_timeout() {
        let cmd_out = command::listen(true);

        let mut sink_out = sink::drain();
        let mut stream_in = stream::once(futures::future::pending());
        let timeout = Duration::from_micros(2);

        let res = execute_command(&mut sink_out, &mut stream_in, &timeout, cmd_out).await;
        match res {
            Err(TncError::IoError(e)) => assert_eq!(io::ErrorKind::TimedOut, e.kind()),
            _ => assert!(false),
        }
    }

    #[runtime::test]
    async fn test_streams() {
        let stream_ctrl = Cursor::new(b"BUSY FALSE\rINPUTPEAKS BLAH\rREJECTEDBW\r".to_vec());
        let stream_data = Cursor::new(b"\x00\x08ARQHELLO\x00\x0BIDFID: W1AW".to_vec());

        let mut tnc = AsyncTnc::new_from_streams(stream_ctrl, stream_data, "W1AW");

        futures::executor::block_on(async {
            match tnc.data_stream_sink().next().await {
                Some(DataEvent::Data(_d)) => assert!(true),
                _ => assert!(false),
            }

            match tnc.data_stream_sink().next().await {
                Some(DataEvent::Data(DataIn::IDF(_i0, _i1))) => assert!(true),
                _ => assert!(false),
            }

            match tnc.data_stream_sink().next().await {
                Some(DataEvent::Event(ConnectionStateChange::Busy(false))) => assert!(true),
                _ => assert!(false),
            }

            match tnc.data_stream_sink().next().await {
                Some(DataEvent::Event(ConnectionStateChange::Failed(
                    ConnectionFailedReason::IncompatibleBandwidth,
                ))) => assert!(true),
                _ => assert!(false),
            }

            assert!(tnc.data_stream_sink().next().await.is_none());
        });
    }

    #[runtime::test]
    async fn test_execute_disconnect() {
        let mut cx = Context::from_waker(task::noop_waker_ref());

        let (mut ctrl_out_snd, mut ctrl_out_rx) = mpsc::unbounded();
        let (ctrl_in_snd, mut ctrl_in_rx) = mpsc::unbounded();
        let (evt_in_snd, mut evt_in_rx) = mpsc::unbounded();

        // starts disconnection, but no connection in progress
        let mut state = DisconnectProgress::NoProgress;
        ctrl_in_snd
            .unbounded_send(Err("not from state".to_owned()))
            .unwrap();
        match execute_disconnect(
            &mut state,
            &mut cx,
            &mut ctrl_out_snd,
            &mut ctrl_in_rx,
            &mut evt_in_rx,
        ) {
            Poll::Ready(Ok(())) => assert!(true),
            _ => assert!(false),
        }
        assert_eq!(DisconnectProgress::NoProgress, state);

        // starts disconnection
        state = DisconnectProgress::NoProgress;
        match execute_disconnect(
            &mut state,
            &mut cx,
            &mut ctrl_out_snd,
            &mut ctrl_in_rx,
            &mut evt_in_rx,
        ) {
            Poll::Pending => assert!(true),
            _ => assert!(false),
        }
        assert_eq!(DisconnectProgress::SentDisconnect, state);
        let _ = ctrl_out_rx.try_next().unwrap();

        // make progress towards disconnection
        ctrl_in_snd
            .unbounded_send(Ok((CommandID::DISCONNECT, None)))
            .unwrap();
        match execute_disconnect(
            &mut state,
            &mut cx,
            &mut ctrl_out_snd,
            &mut ctrl_in_rx,
            &mut evt_in_rx,
        ) {
            Poll::Pending => assert!(true),
            _ => assert!(false),
        }
        assert_eq!(DisconnectProgress::AckDisconnect, state);

        // finish disconnect
        evt_in_snd
            .unbounded_send(DataEvent::Event(ConnectionStateChange::SendBuffer(0)))
            .unwrap();
        evt_in_snd
            .unbounded_send(DataEvent::Event(ConnectionStateChange::Closed))
            .unwrap();
        match execute_disconnect(
            &mut state,
            &mut cx,
            &mut ctrl_out_snd,
            &mut ctrl_in_rx,
            &mut evt_in_rx,
        ) {
            Poll::Ready(Ok(())) => assert!(true),
            _ => assert!(false),
        }
        assert_eq!(DisconnectProgress::NoProgress, state);
    }

}
