//! Framing for the TNC data protocol
//!
use std::cmp::min;
use std::io;

use bytes::{BufMut, BytesMut};

use crate::framer::{Decoder, Encoder};

use crate::tncio::data::{DataIn, DataOut};

/// Frames and sends TNC data messages
pub struct TncDataFraming {}

impl TncDataFraming {
    /// New TNC data message framer
    pub fn new() -> TncDataFraming {
        TncDataFraming {}
    }
}

impl Encoder for TncDataFraming {
    type EncodeItem = DataOut;

    fn encode(&mut self, item: Self::EncodeItem, dst: &mut BytesMut) -> io::Result<()> {
        // data is prefixed with a big endian size
        //
        // if we have more than 2**16 bytes to send, we need to split it up
        // into blocks for the TNC.
        let mut pos = 0usize;
        while pos < item.len() {
            let remain = item.len() - pos;
            let chunk_size = min(remain, u16::max_value() as usize);
            dst.reserve(2);
            dst.put_u16_be(chunk_size as u16);
            dst.extend_from_slice(&item.as_ref()[pos..pos + chunk_size]);

            pos += chunk_size;
        }
        Ok(())
    }
}

impl Decoder for TncDataFraming {
    type DecodeItem = DataIn;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Self::DecodeItem>> {
        if src.len() < 5 {
            return Ok(None);
        }
        let out = DataIn::parse(src.as_ref());
        let _ = src.split_to(out.0);
        Ok(out.1)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    use bytes::{Buf, Bytes};
    use futures::executor::ThreadPool;
    use futures::prelude::*;

    use crate::framer::Framed;

    #[test]
    fn test_encode() {
        let dataempty = Bytes::from(vec![0u8; 0]);
        let bigdata = Bytes::from(vec![0u8; 66000]);
        let littledata = Bytes::from("HI!!");
        let mut outbuf = BytesMut::new();
        let mut codec = TncDataFraming::new();

        // nothing -> nothing
        codec.encode(dataempty, &mut outbuf).unwrap();
        assert_eq!(outbuf.len(), 0);

        // really big messages are split
        codec.encode(littledata, &mut outbuf).unwrap();
        assert_eq!(outbuf.as_ref(), b"\x00\x04HI!!");

        outbuf.clear();

        codec.encode(bigdata, &mut outbuf).unwrap();
        assert_eq!(outbuf.len(), 66000 + 2 * 2);
        assert_eq!(outbuf[0], 255u8);
        assert_eq!(outbuf[1], 255u8);
        assert_eq!(
            Cursor::new(&outbuf[65537..65539]).get_u16_be(),
            (66000 - 65535) as u16
        );
    }

    #[test]
    fn test_decode() {
        let words = b"\x00\x08ARQHELLO\x00\x08FECWORLDERR".to_vec();
        let curs = Cursor::new(words);
        let mut framer = Framed::new(curs, TncDataFraming::new());
        let mut exec = ThreadPool::new().expect("Failed to create threadpool");
        exec.run(async {
            assert_eq!(
                DataIn::ARQ(Bytes::from("HELLO")),
                framer.next().await.unwrap()
            );
            assert_eq!(
                DataIn::FEC(Bytes::from("WORLD")),
                framer.next().await.unwrap()
            );
        });
    }
}
