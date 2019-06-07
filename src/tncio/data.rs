//! Data received from the TNC
//!
//! Represents payload data from either an unconnected FEC
//! frame or an ARQ connection with a peer

use bytes::Bytes;

use nom;
use nom::*;

/// Data out is a constant Bytes buffer
pub type DataOut = Bytes;

/// Represents a data fragment received from the TNC
#[derive(Debug, PartialEq, Eq)]
pub enum DataIn {
    /// ARQ Stream
    ///
    /// ARQ is a streaming connection; there are no
    /// message boundaries. The payload bytes in this
    /// `DataIn` are assumed to immediately follow those
    /// from the previous `DataIn`.
    ARQ(Bytes),

    /// FEC Frame
    ///
    /// FEC frames always include a complete message
    FEC(Bytes),
}

impl DataIn {
    /// Tries to frame an incoming data packet
    ///
    /// This method may panic if the data cannot be correctly
    /// framed.
    ///
    /// # Parameters
    /// - `buf`: A byte buffer that is aligned to the start
    ///   of a packed tnc→host data transmission
    ///
    /// # Returns
    /// Tuple of
    /// 0. Number of bytes from `buf` that were consumed
    /// 1. A single parsed `DataIn`, if one can be parsed.
    pub fn parse(buf: &[u8]) -> (usize, Option<DataIn>) {
        match parse_data(buf) {
            Ok((rem, (dtype, data))) => {
                let taken = buf.len() - rem.len();
                let out = match dtype {
                    "ARQ" => Some(DataIn::ARQ(Bytes::from(data))),
                    "FEC" => Some(DataIn::FEC(Bytes::from(data))),
                    "ERR" => None,
                    _ => None,
                };
                (taken, out)
            }
            Err(e) => {
                if e.is_incomplete() {
                    (0, None)
                } else {
                    panic!("Unexpected data channel parse error: {:?}", e);
                }
            }
        }
    }
}

named!(
    parse_data<&[u8], (&str, &[u8])>,
    do_parse!(
        dlen: be_u16 >>
        dtype: map_res!(
            alt!(
                tag!(b"FEC") |
                tag!(b"ARQ") |
                tag!(b"ERR")
            ),
            std::str::from_utf8
        ) >>
        data: take!(dlen - 3) >>
        ((dtype, data))
    )
);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse() {
        let data = b"\x00\x08ARQHELLO";
        let res = DataIn::parse(data);
        assert_eq!(data.len(), res.0);
        assert_eq!(DataIn::ARQ(Bytes::from("HELLO")), res.1.unwrap());

        // err fields are eaten
        let data = b"\x00\x08ERRHELLO";
        let res = DataIn::parse(data);
        assert_eq!(data.len(), res.0);
        assert!(res.1.is_none());

        // not enough bytes
        let data = b"\x00\x08ARQHELL";
        let res = DataIn::parse(data);
        assert_eq!(0, res.0);
        assert!(res.1.is_none());

        // trailing field
        let data = b"\x00\x08ARQHELLO\x00\x08";
        let res = DataIn::parse(data);
        assert_eq!(data.len() - 2, res.0);
        assert_eq!(DataIn::ARQ(Bytes::from("HELLO")), res.1.unwrap());
    }
}
