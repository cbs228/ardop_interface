//! Framing for the TNC control protocol
//!
use std::io;
use std::string::String;

use bytes::BytesMut;
use tokio::codec::{Decoder, Encoder};

use super::super::response::Response;

/// Frames and sends TNC control messages
pub struct TncControlFraming {}

impl TncControlFraming {
    /// New TNC control message framer
    pub fn new() -> TncControlFraming {
        TncControlFraming {}
    }
}

impl Encoder for TncControlFraming {
    type Item = String;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> io::Result<()> {
        dst.extend_from_slice(item.as_bytes());
        Ok(())
    }
}

impl Decoder for TncControlFraming {
    type Item = Response;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Self::Item>> {
        // parse the head of src
        let res = Response::parse(src.as_ref());

        // drop parsed characters from the buffer
        let _ = src.advance(res.0);

        Ok(res.1)
    }
}

#[cfg(test)]
mod test {
    use super::super::super::response::{Event, Response};
    use super::*;

    use std::io::Cursor;

    use tokio::codec::Framed;
    use tokio::prelude::*;

    #[test]
    fn test_decode() {
        let words = b"PENDING\rCANCELPENDING\r".to_vec();
        let curs = Cursor::new(words);
        let framer = Framed::new(curs, TncControlFraming::new());
        let mut syncframer = Stream::wait(framer);

        assert_eq!(
            Response::Event(Event::PENDING),
            syncframer.next().unwrap().unwrap()
        );
        assert_eq!(
            Response::Event(Event::CANCELPENDING),
            syncframer.next().unwrap().unwrap()
        );
    }

    #[test]
    fn test_encode() {
        let curs = Cursor::new(vec![0u8; 0]);
        let framer = Framed::new(curs, TncControlFraming::new());
        let fut = framer
            .send("MYCALL W1AW\r".to_owned())
            .map_err(|_f| {})
            .map(|_r| {});

        tokio::run(fut);
    }
}
