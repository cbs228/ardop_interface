use std::io;

use futures::prelude::*;
use tokio::codec::FramedWrite;
use tokio::io::AsyncWrite;

use super::super::framing::control::TncControlFraming;
use super::super::framing::data::TncDataFraming;
use super::super::tncdata::DataOut;

/// Create sink for sending control messages
///
/// Creates a sink which can be used to send messages to
/// the TNC's control port.
///
/// # Parameters
/// - `sock`: The write end of TNC control port socket
///
/// # Returns
/// A sink which will send every stringified `Command`
/// to the TNC.
pub fn control<S>(sock: S) -> impl Sink<SinkItem = String, SinkError = io::Error>
where
    S: AsyncWrite,
{
    FramedWrite::new(sock, TncControlFraming::new())
}

/// Create sink for sending data messages
///
/// Creates a sink which can be used to send bytes to
/// the TNC's data port. Bytes written here are intended
/// for transmission "over-the-air."
///
/// # Parameters
/// - `sock`: The write end of TNC data port socket
///
/// # Returns
/// A sink which will send every `DataOut` fragment to
/// the TNC for immediate transmission. The data will be
/// sent in ARQ mode (if connected) or FEC mode, depending
/// on which mode is in use.
pub fn data<S>(sock: S) -> impl Sink<SinkItem = DataOut, SinkError = io::Error>
where
    S: AsyncWrite,
{
    FramedWrite::new(sock, TncDataFraming::new())
}
