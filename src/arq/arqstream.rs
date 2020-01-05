//! Asynchronous ARQ Connections
//!
//! This module exposes the `ArqStream` type which implements
//! `AsyncRead` and `AsyncWrite`. These traits enable access
//! to RF connections much as one would use a TCP socket.

use std::fmt;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::executor;
use futures::io::{AsyncRead, AsyncWrite};
use futures::lock::{Mutex, MutexLockFuture};
use futures::task::{Context, Poll};

use super::connectioninfo::ConnectionInfo;
use crate::tncio::arqstate::ArqState;
use crate::tncio::asynctnc::AsyncTncTcp;

/// A TCP-like interface for ARQ RF connections
///
/// See the [module-level](index.html) documentation
/// for examples and usage details.
pub struct ArqStream {
    tnc: Arc<Mutex<AsyncTncTcp>>,
    state: ArqState,
}

impl ArqStream {
    /// True if the connection was open (at last check)
    ///
    /// This method returns `true` if the connection was
    /// believed to be open during the last I/O operation
    /// conducted to the ARDOP TNC.
    ///
    /// Even if this value returns `true`, the connection
    /// may be detected as dead during the next read or
    /// write.
    pub fn is_open(&self) -> bool {
        self.state.is_open()
    }

    /// True if the connection is disconnecting
    ///
    /// This method returns `true` if the local side has
    /// initiated a disconnect but the disconnect has yet
    /// to complete.
    ///
    /// While the disconnect is "in flight," `is_open()`
    /// will continue to return true.
    pub fn is_disconnecting(&self) -> bool {
        self.state.is_disconnecting()
    }

    /// Return connection information
    ///
    /// Includes immutable details about the connection, such
    /// as the local and remote callsigns.
    pub fn info(&self) -> &ConnectionInfo {
        self.state.info()
    }

    /// Returns total number of bytes received
    ///
    /// Counts the total number of *payload* bytes which have
    /// been transmitted over the air *AND* acknowledged by
    /// the local station. This value is aggregated over the
    /// lifetime of the `ArqStream`.
    pub fn bytes_received(&self) -> u64 {
        self.state.bytes_received()
    }

    /// Total number of bytes successfully transmitted
    ///
    /// Counts the total number of *payload* bytes which have
    /// been transmitted over the air *AND* acknowledged by
    /// the remote peer. This value is aggregated over the
    /// lifetime of the `ArqStream`.
    pub fn bytes_transmitted(&self) -> u64 {
        self.state.bytes_transmitted()
    }

    /// Total number of bytes pending peer acknowledgement
    ///
    /// Counts the total number of bytes that have been
    /// accepted by the local ARDOP TNC but have not yet
    /// been delivered to the peer.
    ///
    /// Bytes accepted by this object become *staged*. Once
    /// the TNC has accepted responsibility for the bytes,
    /// they become *unacknowledged*. Once the remote peer
    /// has acknowledged the data, the bytes become
    /// *transmitted*.
    pub fn bytes_unacknowledged(&self) -> u64 {
        self.state.bytes_unacknowledged()
    }

    /// Bytes pending acceptance by the local TNC
    ///
    /// Counts the total number of bytes which have been
    /// accepted by this object internally but have not
    /// yet been delivered to the TNC for transmission.
    ///
    /// Bytes accepted by this object become *staged*. Once
    /// the TNC has accepted responsibility for the bytes,
    /// they become *unacknowledged*. Once the remote peer
    /// has acknowledged the data, the bytes become
    /// *transmitted*.
    pub fn bytes_staged(&self) -> u64 {
        self.state.bytes_staged()
    }

    /// Returns total time elapsed while the connection is/was open
    ///
    /// Returns the total time, in a monotonic reference frame,
    /// elapsed between
    /// 1. the connection being opened; and
    /// 2. the connection being closed
    /// If the connection is still open, then (2) is assumed to be
    /// `now`.
    ///
    /// # Return
    /// Time elapsed since connection was open
    pub fn elapsed_time(&self) -> Duration {
        self.state.elapsed_time()
    }

    // Create from TNC interface and info about a fresh connection
    pub(crate) fn new(tnc: Arc<Mutex<AsyncTncTcp>>, info: ConnectionInfo) -> Self {
        ArqStream {
            tnc,
            state: ArqState::new(info),
        }
    }
}

impl AsyncRead for ArqStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();

        let mut lock_future: MutexLockFuture<AsyncTncTcp> = this.tnc.lock();
        let mut tnc = ready!(Pin::new(&mut lock_future).poll(cx));

        let data = tnc.data_stream_sink();
        this.state.poll_read(data, cx, buf)
    }
}

impl AsyncWrite for ArqStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();

        let mut lock_future: MutexLockFuture<AsyncTncTcp> = this.tnc.lock();
        let mut tnc = ready!(Pin::new(&mut lock_future).poll(cx));

        let data = tnc.data_stream_sink();
        this.state.poll_write(data, cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        let mut lock_future: MutexLockFuture<AsyncTncTcp> = this.tnc.lock();
        let mut tnc = ready!(Pin::new(&mut lock_future).poll(cx));

        let data = tnc.data_stream_sink();
        this.state.poll_flush(data, cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.is_open() {
            return Poll::Ready(Ok(()));
        }
        if !this.is_disconnecting() {
            this.state.shutdown_write();
        }

        let mut lock_future: MutexLockFuture<AsyncTncTcp> = this.tnc.lock();
        let mut tnc = ready!(Pin::new(&mut lock_future).poll(cx));

        match ready!(tnc.poll_disconnect(cx)) {
            Ok(k) => {
                // disconnect done
                this.state.shutdown_read();
                Poll::Ready(Ok(k))
            }
            Err(e) => {
                error!(
                    "Unclean disconnect to {}: {}",
                    this.state.info().peer_call(),
                    &e
                );
                this.state.shutdown_read();
                Poll::Ready(Err(e))
            }
        }
    }
}

impl Drop for ArqStream {
    fn drop(&mut self) {
        // if we are already closed, we drop
        if !self.is_open() {
            return;
        }

        // Mark as closed for writing
        self.state.shutdown_write();

        // This will panic if a LocalPool is being used to
        // provide the asynchronous runtime. You cannot execute a
        // LocalPool executor from within another LocalPool executor.
        //
        // We recommend the use of either "tokio" or "async_std"
        // crates to provide your runtime environment.
        let tncref = self.tnc.clone();
        executor::block_on(async move {
            let mut tnc = tncref.lock().await;
            let _ = tnc.disconnect().await;
        });

        // Mark as fully disconnected
        self.state.shutdown_read();
    }
}

impl fmt::Display for ArqStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}

impl Unpin for ArqStream {}
