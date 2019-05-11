//! Framing for the TNC control protocol
//!
use std::io;
use std::string::String;

use bytes::BytesMut;

use super::framer::{Decoder, Encoder};

use crate::protocol::response::Response;

/// Frames and sends TNC control messages
pub struct TncControlFraming {}

impl TncControlFraming {
    /// New TNC control message framer
    pub fn new() -> TncControlFraming {
        TncControlFraming {}
    }
}

impl Encoder for TncControlFraming {
    type EncodeItem = String;

    fn encode(&mut self, item: Self::EncodeItem, dst: &mut BytesMut) -> io::Result<()> {
        dst.extend_from_slice(item.as_bytes());
        Ok(())
    }
}

impl Decoder for TncControlFraming {
    type DecodeItem = Response;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Self::DecodeItem>> {
        // parse the head of src
        let res = Response::parse(src.as_ref());

        // drop parsed characters from the buffer
        let _ = src.advance(res.0);

        Ok(res.1)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;
    use std::str;

    use futures::executor::ThreadPool;
    use futures::prelude::*;

    use super::super::framer::{FramedRead, FramedWrite};
    use crate::protocol::response::{Event, Response};

    #[test]
    fn test_decode() {
        let words = b"PENDING\rCANCELPENDING\r".to_vec();
        let curs = Cursor::new(words);
        let mut framer = FramedRead::new(curs, TncControlFraming::new());

        let mut exec = ThreadPool::new().expect("Failed to create threadpool");
        exec.run(async {
            let e1 = await!(framer.next());
            assert_eq!(Response::Event(Event::PENDING), e1.unwrap());

            let e2 = await!(framer.next());
            assert_eq!(Response::Event(Event::CANCELPENDING), e2.unwrap());

            let e3 = await!(framer.next());
            assert!(e3.is_none());
        });
    }

    #[test]
    fn test_encode() {
        let curs = Cursor::new(vec![0u8; 24]);
        let mut framer = FramedWrite::new(curs, TncControlFraming::new());

        let mut exec = ThreadPool::new().expect("Failed to create threadpool");
        exec.run(async {
            await!(framer.send("MYCALL W1AW\r".to_owned())).unwrap();
            await!(framer.send("LISTEN TRUE\r".to_owned())).unwrap();
        });
        let (curs, _) = framer.release();
        assert_eq!(
            "MYCALL W1AW\rLISTEN TRUE\r",
            str::from_utf8(curs.into_inner().as_ref()).unwrap()
        );
    }
}
