//! Status messages received from the TNC
//!

use std::str;
use std::string::String;

use nom;
use nom::types::CompleteStr;
use nom::*;

use super::constants as C;

custom_derive! {
    /// ARQ Connection States
    #[derive(Debug, PartialEq, Eq, EnumFromStr, EnumDisplay)]
    pub enum State {
        /// Codec stopped
        OFFLINE,

        /// ARQ has disconnected
        DISC,

        /// Information Sending Station
        ISS,

        /// Information Receiving Station
        IRS,

        /// Attempting to become the ISS
        IRStoISS,

        /// Not connected or trying to connect, but maybe listening
        IDLE,

        /// Sending FEC data
        FECSend,

        /// Receiving FEC data
        FECRcv,
    }
}

impl State {
    /// True if this state is a connected state
    ///
    /// Returns true if the TNC is connected to a remote
    /// peer when it is in the given state.
    pub fn is_connected(state: Self) -> bool {
        match state {
            State::ISS => true,
            State::IRS => true,
            State::IRStoISS => true,
            _ => false,
        }
    }
}

/// Reasons a remote peer will reject a connection
#[derive(Debug, PartialEq, Eq)]
pub enum ConnectionFailedReason {
    /// Peer is busy with an existing connection, or channel busy?
    Busy,

    /// Bandwidth negotiation failed
    IncompatibleBandwidth,

    /// No answer from peer
    NoAnswer,
}

/// Response messages
///
/// The TNC has two types of output messages:
/// 1. A response to a command (request-reply pattern)
/// 2. An asynchronous reporting on some event (publish-subscribe pattern)
#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    /// Successful command execution
    ///
    /// Reports that a host-initiated command of the given type
    /// has been accepted and has or will be acted upon. Some
    /// commands may have other side effects or state changes.
    /// A positive response does not indicate that the command has
    /// executed to completion. Merely that it has been accepted.
    CommandAccepted(C::CommandID),

    /// An unknown, unsolicited message from the TNC
    Unhandled(String),

    /// Bytes of payload data that is "pending"
    ///
    /// Length of the data that is currently in the process of being
    /// transmitted. Includes data not yet acknowledged by the receiving
    /// station.
    BUFFER(u16),

    /// Indicates that the RF channel has become busy (or not)
    BUSY(bool),

    /// Pending connect or ping not for this station
    ///
    /// Indicates to the host that the prior `PENDING` Connect
    /// Request or `PING` was not to `MYCALL` or one of the `MYAUX`
    /// call signs) This allows the Host to resume scanning.
    CANCELPENDING,

    /// Indicates the successful opening of an ARQ connection
    ///
    /// Values
    /// - Remote call sign. This *should* be a proper callsign.
    /// - Connection bandwidth, in Hz
    /// - Maidenhead grid square of remote peer, if known. If
    ///   not provided, will be None.
    CONNECTED(String, u16, Option<String>),

    /// An existing ARQ link has been disconnected
    DISCONNECTED,

    /// A generic, free-form error message
    FAULT(String),

    /// Announces a state transition to the given State
    NEWSTATE(State),

    /// A connect request or ping frame has been detected
    ///
    /// Indicates to the host application a Connect Request
    /// or `PING` frame type has been detected (may not
    /// necessarily be to `MYCALL` or one of the `MYAUX` call
    /// signs).
    ///
    /// This provides an early warning to the host that a
    /// connection may be in process so it can hold any
    /// scanning activity.
    PENDING,

    /// Reports receipt of a ping
    ///
    /// The ping is not necessarily directed at your station.
    ///
    /// Values:
    /// - Sender call sign. This *should* be a proper callsign.
    /// - Remote/intended call sign. This may be a tactical call.
    /// - SNR (in dB relative to 3 kHz noise bandwidth). An SNR
    ///   of `21` indicates an SNR greater than 20 dB.
    /// - Decoded constellation quality (30 -- 100)
    PING(String, String, u16, u16),

    /// Reports receipt of a ping acknowledgement
    ///
    /// The `PINGACK` asynchronous reply to host is sent ONLY if
    /// a prior `PING` to this station was received within the
    /// last 5 seconds.
    ///
    /// Values:
    /// - SNR (in dB relative to 3 kHz noise bandwidth). An SNR
    ///   of `21` indicates an SNR greater than 20 dB.
    /// - Decoded constellation quality (30 -- 100)
    PINGACK(u16, u16),

    /// Reports transmission of a `PINGACK`
    ///
    /// Indicates to the host that a `PING` was received and
    /// that the TNC has automatically transmitted a
    /// `PINGACK` reply.
    PINGREPLY,

    /// Key transmitter
    ///
    /// Indicates that the control software should key or unkey the
    /// transmitter. To operate correctly the transmitter PTT should
    /// be activated within 50 ms of receipt of this response.
    PTT(bool),

    /// Connection rejected by peer
    REJECTED(ConnectionFailedReason),

    /// A textual status message for the user
    STATUS(String),

    /// Announces target of incoming call
    ///
    /// Sent to indicate the receipt of an incoming ARQ connection
    /// request. Value is the "dialed callsign" being called by the
    /// remote peer. This may be one of our "`MYAUX`" callsigns or
    /// our proper assigned `MYCALL` (plus SSID).
    TARGET(String),

    /// Reports version information
    VERSION(String),
}

impl Response {
    /// Parse zero or one TNC `Response` from raw bytes
    ///
    /// Scans the provided byte stream for the next TNC message and
    /// returns it. The remaining bytes, which are not yet parsed,
    /// are also returned.
    ///
    /// Parameters
    /// - `inp`: Raw bytes received from the TNC
    ///
    /// Return Value
    ///
    pub fn parse(inp: &[u8]) -> (&[u8], Option<Response>) {
        match parse_line(inp) {
            Err(e) => {
                if e.is_incomplete() {
                    (inp, None)
                } else {
                    panic!("Unexpected parse error: {:?}", e);
                }
            }
            Ok(res) => {
                if res.1.len() == 0 {
                    (res.0, None)
                } else {
                    let out = parse_response(res.1).unwrap();
                    (res.0, Some(out.1))
                }
            }
        }
    }
}

// Reads a complete line of text
named!(
    parse_line<&[u8], CompleteStr>,
    do_parse!(
        lin: map_res!(
            take_until_and_consume!(C::NEWLINE_STR),
            std::str::from_utf8
        ) >>
        (CompleteStr(lin))
    )
);

// Parse all Responses
named!(
    parse_response<CompleteStr, Response>,
    do_parse!(
        aa: alt!(
            parse_buffer |
            parse_busy |
            parse_cancelpending |
            parse_connected |
            parse_disconnected |
            parse_fault |
            parse_newstate |
            parse_pending |
            parse_pingack |
            parse_pingreply |
            parse_ptt |
            parse_rejected |
            parse_status |
            parse_target |
            parse_version |
            parse_command_ok |
            parse_anything
        ) >>
        (aa)
    )
);

// Parses any line
//
// Always run this parser LAST
named!(
    parse_anything<CompleteStr, Response>,
    do_parse!(
        aa: map!(
            take_while!(|_x| true),
            |s: CompleteStr| (*s).to_owned()
        ) >>
        (Response::Unhandled(aa))
    )
);

// Tries to parse positive acknowledgements to commands
//
// Always run this parser after you have tried all "unsolicited
// message" responses first.
named!(
    parse_command_ok<CompleteStr, Response>,
    do_parse!(
        cmd: map_res!(
            take_while!(is_alpha),
            |s: CompleteStr| (*s).parse::<C::CommandID>()
        ) >>
        take_while!(|_x| true) >>
        (Response::CommandAccepted(cmd))
    )
);

named!(
    parse_buffer<CompleteStr, Response>,
    do_parse!(
        tag!(r"BUFFER") >>
        take_while!(is_space) >>
        aa: parse_u16 >>
        (Response::BUFFER(aa))
    )
);

named!(
    parse_busy<CompleteStr, Response>,
    do_parse!(
        tag!(r"BUSY") >>
        take_while!(is_space) >>
        aa: parse_boolstr_like >>
        (Response::BUSY(aa))
    )
);

named!(
    parse_cancelpending<CompleteStr, Response>,
    do_parse!(
        tag!(r"CANCELPENDING") >>
        (Response::CANCELPENDING)
    )
);

named!(
    parse_connected<CompleteStr, Response>,
    do_parse!(
        tag!(r"CONNECTED") >>
        take_while!(is_space) >>
        aa: take_while!(is_call_letters) >>
        take_while!(is_space) >>
        bw: parse_u16 >>
        gs: opt!(
            do_parse!(
                take_while1!(is_space) >>
                gs: take_while!(is_alphanumeric) >>
                ((*gs).to_owned())
            )
        ) >>
        (Response::CONNECTED((*aa).to_owned(), bw, gs))
    )
);

named!(
    parse_disconnected<CompleteStr, Response>,
    do_parse!(
        tag!(r"DISCONNECTED") >>
        (Response::DISCONNECTED)
    )
);

named!(
    parse_newstate<CompleteStr, Response>,
    do_parse!(
        tag!(r"NEWSTATE") >>
        take_while!(is_space) >>
        aa: map_res!(
            take_while!(is_alphanumeric),
            |s: CompleteStr| (*s).parse::<State>()
        ) >>
        (Response::NEWSTATE(aa))
    )
);

named!(
    parse_fault<CompleteStr, Response>,
    do_parse!(
        tag!(r"FAULT") >>
        take_while!(is_space) >>
        aa: take_while!(|_x| true) >>
        (Response::FAULT((*aa).to_owned()))
    )
);

named!(
    parse_pending<CompleteStr, Response>,
    do_parse!(
        tag!(r"PENDING") >>
        (Response::PENDING)
    )
);

named!(
    parse_ping<CompleteStr, Response>,
    do_parse!(
        tag!(r"PING") >>
        take_while!(is_space) >>
        tx: take_while!(is_call_letters) >>
        tag!(">") >>
        rem: take_while!(is_call_letters) >>
        take_while!(is_space) >>
        snr: parse_u16 >>
        take_while!(is_space) >>
        qual: parse_u16 >>
        (Response::PING((*tx).to_owned(), (*rem).to_owned(), snr, qual))
    )
);

named!(
    parse_pingack<CompleteStr, Response>,
    do_parse!(
        tag!(r"PINGACK") >>
        take_while!(is_space) >>
        snr: parse_u16 >>
        take_while!(is_space) >>
        qual: parse_u16 >>
        (Response::PINGACK(snr, qual))
    )
);

named!(
    parse_pingreply<CompleteStr, Response>,
    do_parse!(
        tag!(r"PINGREPLY") >>
        (Response::PINGREPLY)
    )
);

named!(
    parse_ptt<CompleteStr, Response>,
    do_parse!(
        tag!(r"PTT") >>
        take_while!(is_space) >>
        aa: parse_boolstr_like >>
        (Response::PTT(aa))
    )
);

named!(
    parse_status<CompleteStr, Response>,
    do_parse!(
        tag!(r"STATUS") >>
        take_while!(is_space) >>
        aa: take_while!(|_x| true) >>
        (Response::STATUS((*aa).to_owned()))
    )
);

named!(
    parse_version<CompleteStr, Response>,
    do_parse!(
        tag!(r"VERSION") >>
        take_while!(is_space) >>
        aa: take_while!(|_x| true) >>
        (Response::VERSION((*aa).to_owned()))
    )
);

named!(
    parse_state<CompleteStr, State>,
    map_res!(
        nom::alpha,
        |inp: CompleteStr| str::parse::<State>(*inp)
    )
);

named!(
    parse_target<CompleteStr, Response>,
    do_parse!(
        tag!(r"TARGET") >>
        take_while!(is_space) >>
        aa: take_while!(is_call_letters) >>
        (Response::TARGET((*aa).to_owned()))
    )
);

named!(
    parse_rejected<CompleteStr, Response>,
    do_parse!(
        tag!(r"REJECTED") >>
        aa: parse_rejection_reason >>
        (Response::REJECTED(aa))
    )
);

named!(
    parse_rejection_reason<CompleteStr, ConnectionFailedReason>,
    map_res!(
        nom::alpha,
        |inp: CompleteStr| match *inp {
            r"BW" => Ok(ConnectionFailedReason::IncompatibleBandwidth),
            r"BUSY" => Ok(ConnectionFailedReason::Busy),
            _ => Err(())
        }
    )
);

named!(
    parse_u16<CompleteStr, u16>,
    map_res!(
      take_while!(is_numeric),
      |s: CompleteStr| (*s).parse::<u16>()
    )
);

named!(
    parse_boolstr_like<CompleteStr, bool>,
    alt!(
        do_parse!(
            tag!(C::TRUE) >> (true)
        ) |
        do_parse!(
            tag!(C::FALSE) >> (false)
        )
    )
);

#[allow(dead_code)]
#[inline]
fn is_space(c: char) -> bool {
    c == ' '
}

#[allow(dead_code)]
#[inline]
fn is_alpha(c: char) -> bool {
    c.is_ascii_alphabetic()
}

#[allow(dead_code)]
#[inline]
fn is_alphanumeric(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

#[allow(dead_code)]
#[inline]
fn is_call_letters(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

#[allow(dead_code)]
#[inline]
fn is_numeric(c: char) -> bool {
    c.is_numeric()
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str;

    #[test]
    fn test_state() {
        assert_eq!(b"IDLE", format!("{}", State::IDLE).as_bytes());
        assert_eq!(State::DISC, str::parse("DISC").unwrap());
        assert_eq!(true, State::is_connected(State::IRStoISS));
        assert_eq!(false, State::is_connected(State::DISC));
    }

    #[test]
    fn test_parse_line() {
        let res = parse_line("HELO WORLD\r".as_bytes());
        let r = res.unwrap().1;
        assert_eq!(CompleteStr("HELO WORLD"), r);

        let res = parse_line("\r".as_bytes());
        assert_eq!(CompleteStr(""), res.unwrap().1);
    }

    #[test]
    fn test_parse_anything() {
        let res = parse_anything(CompleteStr("HELO WORLD"));
        assert_eq!(Response::Unhandled("HELO WORLD".to_owned()), res.unwrap().1);
    }

    #[test]
    fn test_parse_u16() {
        let res = parse_u16(CompleteStr("5455"));
        assert_eq!(5455u16, res.unwrap().1);
    }

    #[test]
    fn test_parse_boolstr_like() {
        let res = parse_boolstr_like(CompleteStr("TRUE\r"));
        assert_eq!(true, res.unwrap().1);

        let res = parse_boolstr_like(CompleteStr("FALSE"));
        assert_eq!(false, res.unwrap().1);
    }

    #[test]
    fn test_parse_rejected() {
        assert_eq!(
            Response::REJECTED(ConnectionFailedReason::IncompatibleBandwidth),
            parse_rejected(CompleteStr("REJECTEDBW")).unwrap().1
        );
    }

    #[test]
    fn test_parse_buffer() {
        let res = parse_buffer(CompleteStr("BUFFER 160"));
        assert_eq!(Response::BUFFER(160), res.unwrap().1);
    }

    #[test]
    fn test_parse_busy() {
        let res = parse_busy(CompleteStr("BUSY TRUE"));
        assert_eq!(Response::BUSY(true), res.unwrap().1);

        let res = parse_busy(CompleteStr("BUSY FALSE"));
        assert_eq!(Response::BUSY(false), res.unwrap().1);
    }

    #[test]
    fn test_parse_connected() {
        let res = parse_connected(CompleteStr("CONNECTED W1AW-Z 500 EM00"));
        assert_eq!(
            Response::CONNECTED("W1AW-Z".to_owned(), 500, Some("EM00".to_owned())),
            res.unwrap().1
        );

        let res = parse_connected(CompleteStr("CONNECTED W1AW-Z 500"));
        assert_eq!(
            Response::CONNECTED("W1AW-Z".to_owned(), 500, None),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_fault() {
        let res = parse_fault(CompleteStr("FAULT it isn't working"));
        assert_eq!(
            Response::FAULT("it isn't working".to_owned()),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_newstate() {
        let res = parse_newstate(CompleteStr("NEWSTATE DISC"));
        assert_eq!(Response::NEWSTATE(State::DISC), res.unwrap().1);
    }

    #[test]
    fn test_parse_ping() {
        let res = parse_ping(CompleteStr("PING W1AW>CQ 10 80"));
        assert_eq!(
            Response::PING("W1AW".to_owned(), "CQ".to_owned(), 10, 80),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_pingack() {
        let res = parse_pingack(CompleteStr("PINGACK 10 80"));
        assert_eq!(Response::PINGACK(10, 80), res.unwrap().1);
    }

    #[test]
    fn test_parse_pingreply() {
        let res = parse_pingreply(CompleteStr("PINGREPLY"));
        assert_eq!(Response::PINGREPLY, res.unwrap().1);
    }

    #[test]
    fn test_parse_ptt() {
        let res = parse_ptt(CompleteStr("PTT TRUE"));
        assert_eq!(Response::PTT(true), res.unwrap().1);
    }

    #[test]
    fn test_parse_status() {
        let res = parse_status(CompleteStr("STATUS everything alright"));
        assert_eq!(
            Response::STATUS("everything alright".to_owned()),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_target() {
        let res = parse_target(CompleteStr("TARGET W1AW-Z"));
        assert_eq!(Response::TARGET("W1AW-Z".to_owned()), res.unwrap().1);
    }

    #[test]
    fn test_parse_version() {
        let res = parse_version(CompleteStr("VERSION 1.0.4"));
        assert_eq!(Response::VERSION("1.0.4".to_owned()), res.unwrap().1);
    }

    #[test]
    fn test_parse_command_ok() {
        let res = parse_command_ok(CompleteStr("MYAUX"));
        assert_eq!(
            Response::CommandAccepted(C::CommandID::MYAUX),
            res.unwrap().1
        );

        let res = parse_command_ok(CompleteStr("ARQBW now 2500"));
        assert_eq!(
            Response::CommandAccepted(C::CommandID::ARQBW),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_response() {
        let res = parse_response(CompleteStr("VERSION 1.0.4-b4"));
        assert_eq!(Response::VERSION("1.0.4-b4".to_owned()), res.unwrap().1);

        let res = parse_response(CompleteStr("blah blah"));
        assert_eq!(Response::Unhandled("blah blah".to_owned()), res.unwrap().1);

        let res = parse_response(CompleteStr("CONNECTED W1AW 500"));
        assert_eq!(
            Response::CONNECTED("W1AW".to_owned(), 500, None),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse() {
        let res = Response::parse("PENDING\rCANCELPENDING\r".as_bytes());
        assert_eq!(14, res.0.len());
        assert_eq!(Some(Response::PENDING), res.1);
        let res = Response::parse(res.0);
        assert_eq!(0, res.0.len());
        assert_eq!(Some(Response::CANCELPENDING), res.1);
        let res = Response::parse(res.0);
        assert_eq!(0, res.0.len());
        assert_eq!(None, res.1);

        let res = Response::parse("\r\r\r".as_bytes());
        assert_eq!(2, res.0.len());
        assert_eq!(None, res.1);

        let res = Response::parse("NEWSTATE IRS\r".as_bytes());
        assert_eq!(Some(Response::NEWSTATE(State::IRS)), res.1);
    }
}
