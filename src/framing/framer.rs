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

/// Converts messages to byte stream
pub struct FramedWrite<I, E>
where
    I: AsyncWrite + Unpin,
    E: Encoder,
{
    encoder: E,
    io: I,
    buf: BytesMut,
}

impl<I, E> FramedWrite<I, E>
where
    I: AsyncWrite + Unpin,
    E: Encoder,
{
    /// New framer for output
    ///
    /// Creates a new framer which reads messages streamed
    /// to it and writes bytes to `io`.
    ///
    /// # Parameter
    /// - `io`: A socket or other `AsyncWrite` type
    /// - `codec`: The encoder used to transform messages
    ///   into bytes
    pub fn new(io: I, codec: E) -> FramedWrite<I, E> {
        FramedWrite {
            encoder: codec,
            io,
            buf: BytesMut::with_capacity(READ_SIZE),
        }
    }

    /// Release the I/O and the codec
    ///
    /// Destroys this `FramedWrite` and returns its components
    pub fn release(self) -> (I, E) {
        (self.io, self.encoder)
    }
}

impl<I, E> Unpin for FramedWrite<I, E>
where
    I: AsyncWrite + Unpin,
    E: Encoder,
{
}

impl<I, E> Sink<E::EncodeItem> for FramedWrite<I, E>
where
    I: AsyncWrite + Unpin,
    E: Encoder,
{
    type SinkError = io::Error;

    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::SinkError>> {
        // we are always ready to receive
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: E::EncodeItem) -> Result<(), Self::SinkError> {
        // encode the item
        let this = self.get_mut();
        this.encoder.encode(item, &mut this.buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::SinkError>> {
        // transmit bytes to socket
        let this = self.get_mut();
        if this.buf.is_empty() {
            // nothing to send

        }

        while !this.buf.is_empty() {
            let num_written = ready!(Pin::new(&mut this.io).poll_write(cx, this.buf.as_ref()))?;
            let _ = this.buf.split_to(num_written);

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

/// Converts byte stream to messages
pub struct FramedRead<O, D>
where
    O: AsyncRead + Unpin,
    D: Decoder,
{
    decoder: D,
    io: O,
    buf: BytesMut,
}

impl<O, D> FramedRead<O, D>
where
    O: AsyncRead + Unpin,
    D: Decoder,
{
    /// New framer for input
    ///
    /// Creates a new framer which reads from `io` and
    /// de-frames bytes with the given `codec`
    ///
    /// # Parameters
    /// - `io`: A socket or other `AsyncRead`
    /// - `codec`: Codec which de-frames bytes and
    ///   transforms them into messages.
    ///
    /// # Returns
    /// A `Stream` of items from `io`
    pub fn new(io: O, codec: D) -> FramedRead<O, D> {
        FramedRead {
            decoder: codec,
            io,
            buf: BytesMut::with_capacity(READ_SIZE),
        }
    }

    /// Release the I/O and the codec
    ///
    /// Destroys this `FramedRead` and returns its components
    pub fn release(self) -> (O, D) {
        (self.io, self.decoder)
    }
}

impl<O, D> Unpin for FramedRead<O, D>
where
    O: AsyncRead + Unpin,
    D: Decoder,
{
}

const READ_SIZE: usize = 8192;

impl<O, D> Stream for FramedRead<O, D>
where
    O: AsyncRead + Unpin,
    D: Decoder,
{
    type Item = D::DecodeItem;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // try to frame from the buffer
        let mut buf = &mut this.buf;
        if !buf.is_empty() {
            match this.decoder.decode(&mut buf) {
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
            match this.decoder.decode(&mut buf) {
                Ok(Some(itm)) => return Poll::Ready(Some(itm)),
                Err(_e) => return Poll::Ready(None),
                Ok(None) => continue,
            }
        }
    }
}
