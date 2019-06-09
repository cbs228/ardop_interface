//! Asynchronous ARDOP TNC Interface

use std::convert::Into;
use std::fmt;
use std::net::SocketAddr;
use std::string::String;
use std::sync::Arc;
use std::time::Duration;

use futures::executor::ThreadPool;
use futures::lock::Mutex;

use super::TncResult;

use crate::arq::{ArqStream, ConnectionFailedReason};
use crate::protocol::command;
use crate::protocol::command::Command;
use crate::tncio::asynctnc::AsyncTncTcp;

/// Default minimum clear time for new outgoing connections
pub const DEFAULT_MIN_CLEAR_TIME: &Duration = &Duration::from_secs(15);

/// TNC Interface
pub struct ArdopTnc {
    inner: Arc<Mutex<AsyncTncTcp>>,
    mycall: String,
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
        let pool = ThreadPool::new().expect("Could not create TNC thread pool");
        ArdopTnc::new_from_pool(addr, mycall, DEFAULT_MIN_CLEAR_TIME, pool).await
    }

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
    /// - `min_clear_time`: Minimum duration for channel to be clear
    ///   in order for outgoing connection requests to be made. This
    ///   value cannot be changed after the TNC is instantiated.
    ///
    /// # Returns
    /// A new `ArdopTnc`, or an error if the connection or
    /// initialization step fails.
    pub async fn new_with_clear_time<'a, S>(
        addr: &'a SocketAddr,
        mycall: S,
        min_clear_time: &'a Duration,
    ) -> TncResult<Self>
    where
        S: Into<String>,
    {
        let pool = ThreadPool::new().expect("Could not create TNC thread pool");
        ArdopTnc::new_from_pool(addr, mycall, min_clear_time, pool).await
    }

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
    /// - `min_clear_time`: Minimum duration for channel to be clear
    ///   in order for outgoing connection requests to be made. This
    ///   value cannot be changed after the TNC is instantiated.
    /// - `pool`: A `ThreadPool` which will be used to schedule TNC
    ///   tasks. At present, the TNC must run a task in the
    ///   background in order to keep the channel free/clear indicator
    ///   current.
    ///
    /// # Returns
    /// A new `ArdopTnc`, or an error if the connection or
    /// initialization step fails.
    pub async fn new_from_pool<'a, S>(
        addr: &'a SocketAddr,
        mycall: S,
        min_clear_time: &'a Duration,
        pool: ThreadPool,
    ) -> TncResult<Self>
    where
        S: Into<String>,
    {
        let mycallstr = mycall.into();
        let mycallstr2 = mycallstr.clone();
        let tnc = AsyncTncTcp::new(pool, addr, mycallstr2, min_clear_time.clone()).await?;
        let inner = Arc::new(Mutex::new(tnc));
        Ok(ArdopTnc {
            inner,
            mycall: mycallstr,
        })
    }

    /// Get this station's callsign
    ///
    /// # Returns
    /// The formally assigned callsign for this station.
    pub fn mycall(&self) -> &String {
        &self.mycall
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
    /// - `busy_timeout`: Wait this long, at maximum, for a clear
    ///   channel before giving up.
    ///
    /// # Return
    /// The outer result contains failures related to the local
    /// TNC connection.
    ///
    /// The inner result contains failures related to the RF
    /// connection. If the connection attempt succeeds, returns
    /// a new `ArqStream` that can be used like an asynchronous
    /// `TcpStream`.
    pub async fn connect<S>(
        &mut self,
        target: S,
        bw: u16,
        bw_forced: bool,
        attempts: u16,
        busy_timeout: Duration,
    ) -> TncResult<Result<ArqStream, ConnectionFailedReason>>
    where
        S: Into<String>,
    {
        let mut tnc = self.inner.lock().await;
        match tnc
            .connect(target, bw, bw_forced, attempts, busy_timeout)
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
    /// The inner result contains failures related to the RF
    /// connection. If the connection attempt succeeds, returns
    /// a new `ArqStream` that can be used like an asynchronous
    /// `TcpStream`.
    pub async fn listen(
        &mut self,
        bw: u16,
        bw_forced: bool,
    ) -> TncResult<Result<ArqStream, ConnectionFailedReason>> {
        let mut tnc = self.inner.lock().await;
        match tnc.listen(bw, bw_forced).await? {
            Ok(nfo) => Ok(Ok(ArqStream::new(self.inner.clone(), nfo))),
            Err(e) => Ok(Err(e)),
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
        self.command(command::sendid()).await
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
        self.command(command::twotonetest()).await
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
    /// Timeouts cause `TncError::CommandTimeout` errors.
    /// If a command times out, there is likely a serious
    /// problem with the ARDOP TNC or its connection.
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
    /// # Parameters
    /// - `timeout`: New command timeout value
    pub async fn set_control_timeout(&mut self, timeout: Duration) {
        let mut tnc = self.inner.lock().await;
        tnc.set_control_timeout(timeout)
    }

    /// Gets the event timeout value
    ///
    /// Limits the amount of time that the client is willing
    /// to wait for a connection-related event, such as a
    /// connection or disconnection.
    ///
    /// Timeouts cause `TncError::CommandTimeout` errors.
    /// If an event times out, there is likely a serious
    /// problem with the ARDOP TNC or its connection.
    ///
    /// # Returns
    /// Current timeout value
    pub async fn event_timeout(&self) -> Duration {
        self.inner.lock().await.event_timeout().clone()
    }

    /// Sets timeout for the control connection
    ///
    /// Limits the amount of time that the client is willing
    /// to wait for a connection-related event, such as a
    /// connection or disconnection.
    ///
    /// # Parameters
    /// - `timeout`: New event timeout value
    pub async fn set_event_timeout(&mut self, timeout: Duration) {
        let mut tnc = self.inner.lock().await;
        tnc.set_event_timeout(timeout)
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
