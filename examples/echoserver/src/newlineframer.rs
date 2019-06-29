/// A very simple framer
///
/// This is a Framer which is compatible with the `Framed`
/// type in `ardop_interface`. It splits strings at the
/// UNIX newline character `\n`. It is not particularly robust
/// to the vagaries of UTF-8 decoding, but it does
/// demonstrate the API.
///
/// Real radio protocols should probably use binary "tag length
/// value" instead.
use std::io;
use std::string::String;

use bytes::BufMut;
use bytes::BytesMut;

use ardop_interface::framer::{Decoder, Encoder};

pub struct NewlineFramer {}

impl NewlineFramer {
    pub fn new() -> NewlineFramer {
        NewlineFramer {}
    }
}

impl Encoder for NewlineFramer {
    type EncodeItem = String;

    fn encode(&mut self, item: Self::EncodeItem, dst: &mut BytesMut) -> io::Result<()> {
        dst.extend_from_slice(item.as_bytes());
        dst.put_u8('\n' as u8);
        Ok(())
    }
}

impl Decoder for NewlineFramer {
    type DecodeItem = String;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Self::DecodeItem>> {
        // scan src for a newline
        for i in 0..src.len() {
            if src[i] == '\n' as u8 {
                let mut got = src.split_to(i + 1);
                got.resize(got.len() - 1, 0u8);
                return Ok(Some(String::from_utf8(got.to_vec()).unwrap()));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn test_decode() {
        let mut inp = BytesMut::from(Bytes::from_static(b"HELLO\n\nWORLD\n"));
        let mut frm = NewlineFramer::new();
        assert_eq!("HELLO".to_owned(), frm.decode(&mut inp).unwrap().unwrap());
        assert_eq!("".to_owned(), frm.decode(&mut inp).unwrap().unwrap());
        assert_eq!("WORLD".to_owned(), frm.decode(&mut inp).unwrap().unwrap());
        assert_eq!(None, frm.decode(&mut inp).unwrap());
    }
}
