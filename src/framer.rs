//! A future futures framer
//!
//! The `Framed` wraps an Async I/O type and provides an
//! adapter kit which converts a stream or sink of
//! *messages* into raw *bytes*. The caller must provide
//! a codec which implements `Encoder` and `Decoder`.
//!
//! The `ardop_interface` "`echoserver`" example
//! demonstrates a very simple framer in action.

use std::io;
use std::marker::Unpin;
use std::pin::Pin;

use futures::io::{AsyncRead, AsyncWrite};
use futures::task::Context;
use futures::task::Poll;
use futures::{Sink, Stream};

use bytes::BytesMut;

const READ_SIZE: usize = 8192;
const DEFAULT_SEND_HIGH_WATER_MARK: usize = 65535;

/// Converts messages to byte streams
pub trait Encoder {
    /// The atomic message framed by this `Encoder`
    type EncodeItem;

    /// Frame `item`
    ///
    /// Implementations must write a serialized, framed representation
    /// of `item` to the given `dst` buffer. The encoder *may* enlarge
    /// the buffer to make room for `item`, but in general they are
    /// encouraged to avoid allocations.
    ///
    /// # Parameters
    /// - `item`: The item to frame
    /// - `dst`: Destination buffer
    ///
    /// # Returns
    /// An empty if the framing succeeds. This method may fail if the
    /// `dst` buffer is full and cannot accept any more items.
    fn encode(&mut self, item: Self::EncodeItem, dst: &mut BytesMut) -> io::Result<()>;
}

pub trait Decoder {
    /// The atomic message framed by this `Decoder`
    type DecodeItem;

    /// De-frame `item`
    ///
    /// This method must read `src`, de-frame zero or one items,
    /// and remove the framed bytes from `src`. The de-framed
    /// item must be returned.
    ///
    /// If the codec encounters a bad message from which it
    /// cannot recover, then this method should return an error.
    /// Errors will result in the framer's stream returning
    /// "EOF."
    ///
    /// # Parameters
    /// - `src`: Source byte stream
    ///
    /// # Returns
    /// `Ok(None)` if zero messages can be framed from `src`,
    /// an error if the framer encounters a non-recoverable error,
    /// or an `Ok(Some(DecodeItem))` if a message can be de-framed.
    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Self::DecodeItem>>;
}

/// Converts messages to byte stream, and back again
pub struct Framed<I, C>
where
    I: AsyncRead + AsyncWrite + Unpin,
    C: Encoder + Decoder,
{
    codec: C,
    io: I,
    inbuf: BytesMut,
    outbuf: BytesMut,
    send_high_water_mark: usize,
}

impl<I, C> Framed<I, C>
where
    I: AsyncRead + AsyncWrite + Unpin,
    C: Encoder + Decoder,
{
    /// New framer for I/O
    ///
    /// Creates a new framer which converts between messages
    /// and bytes on the given `io`.
    ///
    /// # Parameter
    /// - `io`: A socket or other `AsyncRead` and `AsyncWrite` type
    /// - `codec`: The encoder used to transform messages
    ///   into bytes
    pub fn new(io: I, codec: C) -> Self {
        Framed {
            codec,
            io,
            inbuf: BytesMut::with_capacity(READ_SIZE),
            outbuf: BytesMut::with_capacity(READ_SIZE),
            send_high_water_mark: DEFAULT_SEND_HIGH_WATER_MARK,
        }
    }

    /// Gets the high-water mark
    ///
    /// When the outgoing buffer reaches the high-water mark,
    /// the `Framer` will stop accepting new outgoing messages
    /// for transmission. The sink will return `Pending` until
    /// the buffer drops below the high-water mark.
    ///
    /// The high water-mark does not impose a hard upper bound
    /// on the length of either buffer, but it does impose a
    /// soft upper bound.
    pub fn send_high_water_mark(&self) -> usize {
        self.send_high_water_mark
    }

    /// Gets the high-water mark
    ///
    /// When the outgoing buffer reaches the high-water mark,
    /// the `Framer` will stop accepting new outgoing messages
    /// for transmission. The sink will return `Pending` until
    /// the buffer drops below the high-water mark.
    ///
    /// The high-water mark does not impose a hard upper bound
    /// on the length of either buffer, but it does impose a
    /// soft upper bound.
    ///
    /// # Parameters
    /// - `hwm`: New send high-water mark
    pub fn set_send_high_water_mark(&mut self, hwm: usize) {
        self.send_high_water_mark = hwm;
    }

    /// Release the I/O and the codec
    ///
    /// Destroys this `FramedRead` and returns its components
    pub fn release(self) -> (I, C) {
        (self.io, self.codec)
    }
}

impl<I, C> Sink<C::EncodeItem> for Framed<I, C>
where
    I: AsyncRead + AsyncWrite + Unpin,
    C: Encoder + Decoder,
{
    type Error = io::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        while self.outbuf.len() > self.send_high_water_mark {
            ready!(Pin::new(&mut *self).poll_flush(cx))?;
        }
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: C::EncodeItem) -> Result<(), Self::Error> {
        // encode the item
        let this = self.get_mut();
        this.codec.encode(item, &mut this.outbuf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        // transmit bytes to socket
        let this = self.get_mut();

        while !this.outbuf.is_empty() {
            let num_written = ready!(Pin::new(&mut this.io).poll_write(cx, this.outbuf.as_ref()))?;
            let _ = this.outbuf.split_to(num_written);

            if num_written <= 0 {
                // a write of zero bytes is sometimes an EOF
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "End of file",
                )));
            }
        }

        // nothing further to send
        return Poll::Ready(Ok(()));
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        ready!(Pin::new(&mut *self).poll_flush(cx))?;
        Pin::new(&mut (*self).io).poll_close(cx)
    }
}

impl<I, C> Stream for Framed<I, C>
where
    I: AsyncRead + AsyncWrite + Unpin,
    C: Encoder + Decoder,
{
    type Item = C::DecodeItem;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let mut buf = &mut this.inbuf;

        loop {
            // try to frame from the buffer
            if !buf.is_empty() {
                match this.codec.decode(&mut buf) {
                    Ok(Some(itm)) => return Poll::Ready(Some(itm)),
                    Err(_e) => return Poll::Ready(None),
                    Ok(None) => { /* continue */ }
                }
            }

            // enlarge the buffer by READ_SIZE, and read that much
            let old_len = buf.len();
            buf.resize(old_len + READ_SIZE, 0u8);
            match Pin::new(&mut this.io).poll_read(cx, &mut buf.as_mut()[old_len..]) {
                Poll::Pending => {
                    buf.resize(old_len, 0u8);
                    return Poll::Pending;
                }
                Poll::Ready(Ok(nread)) => {
                    // shrink the buffer back down
                    buf.resize(old_len + nread, 0u8);
                    if nread <= 0 {
                        // EOF
                        return Poll::Ready(None);
                    }
                }
                Poll::Ready(Err(_e)) => {
                    buf.resize(old_len, 0u8);
                    return Poll::Ready(None);
                }
            }
        }
    }
}

impl<I, C> Unpin for Framed<I, C>
where
    I: AsyncRead + AsyncWrite + Unpin,
    C: Encoder + Decoder,
{
}
