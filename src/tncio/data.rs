//! Data received from the TNC
//!
//! Represents payload data from either an unconnected FEC
//! frame or an ARQ connection with a peer

use std::io;
use std::str;
use std::string::String;

use bytes::Bytes;
use regex::{Captures, Regex};

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

    /// ID Frame
    ///
    /// ID frames are produced by the `SENDID` command and
    /// announce a remote station's callsign and grid.
    /// Contains a tuple of
    ///
    /// 0. Peer callsign
    /// 1. Peer grid, if given
    IDF(String, Option<String>),
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
    pub fn parse(buf: &[u8]) -> io::Result<(usize, Option<DataIn>)> {
        match parse_data(buf) {
            Ok((rem, (dtype, data))) => {
                let taken = buf.len() - rem.len();
                let out = match dtype {
                    "ARQ" => Some(DataIn::ARQ(Bytes::copy_from_slice(data))),
                    "FEC" => Some(DataIn::FEC(Bytes::copy_from_slice(data))),
                    "IDF" => parse_id_frame(data),
                    _ => None,
                };
                Ok((taken, out))
            }
            Err(e) => {
                if e.is_incomplete() {
                    Ok((0, None))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Unexpected data channel parse error: {:?}", e),
                    ))
                }
            }
        }
    }
}

// Read the contents of an ID Frame (IDF) and return
// them if it is valid. ID frames sent by the local
// TNC are ignored.
fn parse_id_frame(inp: &[u8]) -> Option<DataIn> {
    lazy_static! {
        static ref RE: Regex =
            Regex::new("^ID:\\s*([0-9A-Za-z-]+)(?:\\s+\\[(\\w{2,8})\\])?").unwrap();
    }

    let id_string = match str::from_utf8(inp) {
        Ok(st) => st,
        Err(_e) => return None,
    };

    let mtch: Captures = match RE.captures(id_string) {
        Some(c) => c,
        None => return None,
    };

    let peer_call = mtch.get(1).unwrap().as_str().to_owned();
    let peer_grid = match mtch.get(2) {
        Some(gr) => Some(gr.as_str().to_owned()),
        None => None,
    };

    Some(DataIn::IDF(peer_call, peer_grid))
}

named!(
    parse_data<&[u8], (&str, &[u8])>,
    do_parse!(
        dlen: be_u16 >>
        dtype: map_res!(
            take!(3),
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
        let res = DataIn::parse(data).unwrap();
        assert_eq!(data.len(), res.0);
        assert_eq!(DataIn::ARQ(Bytes::from("HELLO")), res.1.unwrap());

        // err fields are eaten
        let data = b"\x00\x08ERRHELLO";
        let res = DataIn::parse(data).unwrap();
        assert_eq!(data.len(), res.0);
        assert!(res.1.is_none());

        // not enough bytes
        let data = b"\x00\x08ARQHELL";
        let res = DataIn::parse(data).unwrap();
        assert_eq!(0, res.0);
        assert!(res.1.is_none());

        // trailing field
        let data = b"\x00\x08ARQHELLO\x00\x08";
        let res = DataIn::parse(data).unwrap();
        assert_eq!(data.len() - 2, res.0);
        assert_eq!(DataIn::ARQ(Bytes::from("HELLO")), res.1.unwrap());

        // unknown frame
        let data = b"\x00\x08ZZZHELLO";
        let res = DataIn::parse(data).unwrap();
        assert_eq!(data.len(), res.0);
        assert!(res.1.is_none());
    }

    #[test]
    fn test_parse_id_frame() {
        let outgoing_id = b"W1AW:[EM00aa]";
        let call_only = b"ID: W1AW";
        let call_and_bad_grid = b"ID: W1AW [bad grid]:";
        let call_and_grid = b"ID: W1AW-8 [EM00]:";

        assert_eq!(None, parse_id_frame(outgoing_id));
        assert_eq!(
            DataIn::IDF("W1AW".to_owned(), None),
            parse_id_frame(call_only).unwrap()
        );
        assert_eq!(
            DataIn::IDF("W1AW".to_owned(), None),
            parse_id_frame(call_and_bad_grid).unwrap()
        );
        assert_eq!(
            DataIn::IDF("W1AW-8".to_owned(), Some("EM00".to_owned())),
            parse_id_frame(call_and_grid).unwrap()
        );
    }
}
