//! A future futures framer
//!
//! The `FramedWrite` translates messages into an non-delimited
//! stream of bytes. The `FramedRead` does the inverse:
//! transforming a non-delimited stream of bytes into messages.
//! Both include an internal buffer of work.
//!

use std::io;
use std::marker::Unpin;
use std::pin::Pin;

use futures::io::{AsyncRead, AsyncWrite};
use futures::task::Context;
use futures::task::Poll;
use futures::{Sink, Stream};

use bytes::BytesMut;

const READ_SIZE: usize = 8192;

/// Converts messages to byte streams
pub trait Encoder {
    /// The atomic message framed by this `Encoder`
    type EncodeItem;

    /// Frame `item`
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
    /// and remove the framed bytes from `src`.
    ///
    /// # Parameters
    /// - `src`: Source byte stream
    ///
    /// # Returns
    /// `Ok(None)` if zero messages can be framed from `src`,
    /// an error if the framer fails to parse an item, or an
    /// `Ok(Some(DecodeItem))` if a message can be de-framed.
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
        }
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
    type SinkError = io::Error;

    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::SinkError>> {
        // we are always ready to receive
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: C::EncodeItem) -> Result<(), Self::SinkError> {
        // encode the item
        let this = self.get_mut();
        this.codec.encode(item, &mut this.outbuf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::SinkError>> {
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

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::SinkError>> {
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

        // try to frame from the buffer
        let mut buf = &mut this.inbuf;
        if !buf.is_empty() {
            match this.codec.decode(&mut buf) {
                Ok(Some(itm)) => return Poll::Ready(Some(itm)),
                Err(_e) => return Poll::Ready(None),
                Ok(None) => { /* continue */ }
            }
        }

        loop {
            // enlarge the buffer by READ_SIZE, and read that much
            let old_len = buf.len();
            buf.resize(old_len + READ_SIZE, 0u8);
            match ready!(Pin::new(&mut this.io).poll_read(cx, &mut buf.as_mut()[old_len..])) {
                Ok(nread) => {
                    // shrink the buffer back down
                    buf.resize(old_len + nread, 0u8);
                    if nread <= 0 {
                        // EOF
                        return Poll::Ready(None);
                    }
                }
                Err(_e) => {
                    buf.resize(old_len, 0u8);
                    return Poll::Ready(None);
                }
            }

            // try to frame
            match this.codec.decode(&mut buf) {
                Ok(Some(itm)) => return Poll::Ready(Some(itm)),
                Err(_e) => return Poll::Ready(None),
                Ok(None) => continue,
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
