//! Status messages received from the TNC
//!
//! The main method, `Response::parse()`, invokes a `nom` parser which
//! attempts to parse a TNC output from the provided byte stream.
//! In the output,
//!
//! 0. Contains the remaining bytes which have not yet been parsed
//!    into a TNC message. Provide these bytes to future calls to
//!    `Response::parse()`.
//!
//! 1. If the parser matched anything contains `Some` `Response`,
//!    which is further enumerated.

use std::str;
use std::string::String;

use nom;
use nom::types::CompleteStr;
use nom::*;

use super::constants::{CommandID, FALSE, NEWLINE_STR, TRUE};
use crate::arq::ConnectionInfo;
use crate::tncerror::ConnectionFailedReason;

custom_derive! {
    /// ARQ Connection States
    #[derive(Debug, PartialEq, Eq, EnumFromStr, EnumDisplay, Clone)]
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
    pub fn is_connected(state: &Self) -> bool {
        match state {
            &State::ISS => true,
            &State::IRS => true,
            &State::IRStoISS => true,
            _ => false,
        }
    }
}

/// Announces a change in the ARQ connection state
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConnectionStateChange {
    /// An open connection has been closed
    ///
    /// Connections may be closed by `DISCONNECT` tear-down
    /// or by an `ABORT`. The loss of connection reason is
    /// not enumerated here, however.
    Closed,

    /// Successfully connected
    ///
    /// Data is information about the connection
    Connected(ConnectionInfo),

    /// Failed to connect
    ///
    /// No connection was ever successfully made with
    /// the remote peer. Failure is not limited to
    /// `CONNECT` requests. Even when `LISTEN`ing, it
    /// is possible for the bandwidth negotiation to
    /// fail.
    Failed(ConnectionFailedReason),

    /// Reports on the progress of data transmission
    ///
    /// This message reports the current length of the
    /// TNC's outbound `BUFFER`. When the `SendBuffer`
    /// reaches zero, all enqueued data has been transmitted.
    /// If your application allows the buffer to empty, a
    /// link turnover is likely to occur.
    ///
    /// For ARQ connections, the `SendBuffer` counts the total
    /// length of all un-ACK'd data. A `SendBuffer(0)` event
    /// indicates that the peer has successfully received all
    /// outstanding data.
    SendBuffer(u64),
}

/// Event messages
///
/// These events are always sent asynchronously—i.e., not at the
/// request of the host.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Event {
    /// Bytes of payload data that is "pending"
    ///
    /// Length of the data that is currently in the process of being
    /// transmitted. Includes data not yet acknowledged by the receiving
    /// station.
    BUFFER(u64),

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

    /// An unknown, unsolicited message from the TNC
    Unknown(String),
}

/// Command success
///
/// Reports a successful command execution. A tuple of:
/// 1. ID of the command that succeeded
/// 2. Optional response text. At present, this is only
///    populated for `VERSION` responses.
///
pub type CommandOk = (CommandID, Option<String>);

/// Command success or failure
///
/// Reports that a command initiated by the host has either
/// succeeded or failed.
///
/// - For successful commands, returns the Command ID and,
///   for some commands, a descriptive string. At present,
///   the string is only populated for `VERSION` responses.
///
/// - For erroneous commands, returns the error message.
///   There is no standard way to determine the ID of a
///   failed command.
pub type CommandResult = Result<CommandOk, String>;

/// Response messages
///
/// The TNC has two types of output messages:
/// 1. A response to a command (request-reply pattern)
/// 2. An asynchronous reporting on some event (publish-subscribe pattern)
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Response {
    /// Command success or failure
    ///
    /// Reports that a command initiated by the host has either
    /// succeeded or failed.
    ///
    /// - For successful commands, returns the Command ID and,
    ///   for some commands, a descriptive string. At present,
    ///   the string is only populated for `VERSION` responses.
    ///
    /// - For erroneous commands, returns the error message.
    ///   There is no standard way to determine the ID of a
    ///   failed command.
    CommandResult(CommandResult),

    /// Asynchronous event message
    ///
    /// `Event`s report `CONNECTED`, `DISCONNECTED`, and many other
    /// potential state transitions.
    Event(Event),
}

impl Response {
    /// Parse zero or one TNC `Response` from raw bytes
    ///
    /// Scans the provided byte stream for the next TNC message and
    /// returns it. The remaining bytes, which are not yet parsed,
    /// are also returned.
    ///
    /// A catch-all Response, `Response::Event(Event::Unknown(String))`
    /// exists to parse messages which are otherwise not parseable.
    ///
    /// Parameters
    /// - `inp`: Raw bytes received from the TNC
    ///
    /// Return Value
    /// 0. Number of bytes consumed
    /// 1. A `Response` parsed from `inp`, if any.
    pub fn parse(inp: &[u8]) -> (usize, Option<Response>) {
        let inp_len = inp.len();
        match parse_line(inp) {
            Err(e) => {
                if e.is_incomplete() {
                    (0usize, None)
                } else {
                    panic!("Unexpected parse error: {:?}", e);
                }
            }
            Ok(res) => {
                if res.1.len() == 0 {
                    (inp_len - res.0.len(), None)
                } else {
                    let out = parse_response(res.1).unwrap();
                    (inp_len - res.0.len(), Some(out.1))
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
            take_until_and_consume!(NEWLINE_STR),
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
            parse_ping |
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
        (Response::Event(Event::Unknown(aa)))
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
            |s: CompleteStr| (*s).parse::<CommandID>()
        ) >>
        take_while!(|_x| true) >>
        (Response::CommandResult(Ok((cmd, None))))
    )
);

named!(
    parse_buffer<CompleteStr, Response>,
    do_parse!(
        tag!(r"BUFFER") >>
        take_while!(is_space) >>
        aa: parse_u64 >>
        (Response::Event(Event::BUFFER(aa)))
    )
);

named!(
    parse_busy<CompleteStr, Response>,
    do_parse!(
        tag!(r"BUSY") >>
        take_while!(is_space) >>
        aa: parse_boolstr_like >>
        (Response::Event(Event::BUSY(aa)))
    )
);

named!(
    parse_cancelpending<CompleteStr, Response>,
    do_parse!(
        tag!(r"CANCELPENDING") >>
        (Response::Event(Event::CANCELPENDING))
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
        (Response::Event(Event::CONNECTED((*aa).to_owned(), bw, gs)))
    )
);

named!(
    parse_disconnected<CompleteStr, Response>,
    do_parse!(
        tag!(r"DISCONNECTED") >>
        (Response::Event(Event::DISCONNECTED))
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
        (Response::Event(Event::NEWSTATE(aa)))
    )
);

named!(
    parse_fault<CompleteStr, Response>,
    do_parse!(
        tag!(r"FAULT") >>
        take_while!(is_space) >>
        aa: take_while!(|_x| true) >>
        (Response::CommandResult(Err((*aa).to_owned())))
    )
);

named!(
    parse_pending<CompleteStr, Response>,
    do_parse!(
        tag!(r"PENDING") >>
        (Response::Event(Event::PENDING))
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
        (Response::Event(Event::PING((*tx).to_owned(), (*rem).to_owned(), snr, qual)))
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
        (Response::Event(Event::PINGACK(snr, qual)))
    )
);

named!(
    parse_pingreply<CompleteStr, Response>,
    do_parse!(
        tag!(r"PINGREPLY") >>
        (Response::Event(Event::PINGREPLY))
    )
);

named!(
    parse_ptt<CompleteStr, Response>,
    do_parse!(
        tag!(r"PTT") >>
        take_while!(is_space) >>
        aa: parse_boolstr_like >>
        (Response::Event(Event::PTT(aa)))
    )
);

named!(
    parse_status<CompleteStr, Response>,
    do_parse!(
        tag!(r"STATUS") >>
        take_while!(is_space) >>
        aa: take_while!(|_x| true) >>
        (Response::Event(Event::STATUS((*aa).to_owned())))
    )
);

named!(
    parse_version<CompleteStr, Response>,
    do_parse!(
        tag!(r"VERSION") >>
        take_while!(is_space) >>
        aa: take_while!(|_x| true) >>
        (Response::CommandResult(Ok((CommandID::VERSION, Some((*aa).to_owned())))))
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
        (Response::Event(Event::TARGET((*aa).to_owned())))
    )
);

named!(
    parse_rejected<CompleteStr, Response>,
    do_parse!(
        tag!(r"REJECTED") >>
        aa: parse_rejection_reason >>
        (Response::Event(Event::REJECTED(aa)))
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
    parse_u64<CompleteStr, u64>,
    map_res!(
      take_while!(is_numeric),
      |s: CompleteStr| (*s).parse::<u64>()
    )
);

named!(
    parse_boolstr_like<CompleteStr, bool>,
    alt!(
        do_parse!(
            tag!(TRUE) >> (true)
        ) |
        do_parse!(
            tag!(FALSE) >> (false)
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
        assert_eq!(true, State::is_connected(&State::IRStoISS));
        assert_eq!(false, State::is_connected(&State::DISC));
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
        assert_eq!(
            Response::Event(Event::Unknown("HELO WORLD".to_owned())),
            res.unwrap().1
        );
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
            Response::Event(Event::REJECTED(
                ConnectionFailedReason::IncompatibleBandwidth
            )),
            parse_rejected(CompleteStr("REJECTEDBW")).unwrap().1
        );
    }

    #[test]
    fn test_parse_buffer() {
        let res = parse_buffer(CompleteStr("BUFFER 160"));
        assert_eq!(Response::Event(Event::BUFFER(160)), res.unwrap().1);
    }

    #[test]
    fn test_parse_busy() {
        let res = parse_busy(CompleteStr("BUSY TRUE"));
        assert_eq!(Response::Event(Event::BUSY(true)), res.unwrap().1);

        let res = parse_busy(CompleteStr("BUSY FALSE"));
        assert_eq!(Response::Event(Event::BUSY(false)), res.unwrap().1);
    }

    #[test]
    fn test_parse_connected() {
        let res = parse_connected(CompleteStr("CONNECTED W1AW-Z 500 EM00"));
        assert_eq!(
            Response::Event(Event::CONNECTED(
                "W1AW-Z".to_owned(),
                500,
                Some("EM00".to_owned())
            )),
            res.unwrap().1
        );

        let res = parse_connected(CompleteStr("CONNECTED W1AW-Z 500"));
        assert_eq!(
            Response::Event(Event::CONNECTED("W1AW-Z".to_owned(), 500, None)),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_fault() {
        let res = parse_fault(CompleteStr("FAULT it isn't working"));
        assert_eq!(
            Response::CommandResult(Err("it isn't working".to_owned())),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_newstate() {
        let res = parse_newstate(CompleteStr("NEWSTATE DISC"));
        assert_eq!(
            Response::Event(Event::NEWSTATE(State::DISC)),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_ping() {
        let res = parse_ping(CompleteStr("PING W1AW>CQ 10 80"));
        assert_eq!(
            Response::Event(Event::PING("W1AW".to_owned(), "CQ".to_owned(), 10, 80)),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_pingack() {
        let res = parse_pingack(CompleteStr("PINGACK 10 80"));
        assert_eq!(Response::Event(Event::PINGACK(10, 80)), res.unwrap().1);
    }

    #[test]
    fn test_parse_pingreply() {
        let res = parse_pingreply(CompleteStr("PINGREPLY"));
        assert_eq!(Response::Event(Event::PINGREPLY), res.unwrap().1);
    }

    #[test]
    fn test_parse_ptt() {
        let res = parse_ptt(CompleteStr("PTT TRUE"));
        assert_eq!(Response::Event(Event::PTT(true)), res.unwrap().1);
    }

    #[test]
    fn test_parse_status() {
        let res = parse_status(CompleteStr("STATUS everything alright"));
        assert_eq!(
            Response::Event(Event::STATUS("everything alright".to_owned())),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_target() {
        let res = parse_target(CompleteStr("TARGET W1AW-Z"));
        assert_eq!(
            Response::Event(Event::TARGET("W1AW-Z".to_owned())),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_version() {
        let res = parse_version(CompleteStr("VERSION 1.0.4"));
        assert_eq!(
            Response::CommandResult(Ok((CommandID::VERSION, Some("1.0.4".to_owned())))),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_command_ok() {
        let res = parse_command_ok(CompleteStr("MYAUX"));
        assert_eq!(
            Response::CommandResult(Ok((CommandID::MYAUX, None))),
            res.unwrap().1
        );

        let res = parse_command_ok(CompleteStr("ARQBW now 2500"));
        assert_eq!(
            Response::CommandResult(Ok((CommandID::ARQBW, None))),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse_response() {
        let res = parse_response(CompleteStr("VERSION 1.0.4-b4"));
        assert_eq!(
            Response::CommandResult(Ok((CommandID::VERSION, Some("1.0.4-b4".to_owned())))),
            res.unwrap().1
        );

        let res = parse_response(CompleteStr("blah blah"));
        assert_eq!(
            Response::Event(Event::Unknown("blah blah".to_owned())),
            res.unwrap().1
        );

        let res = parse_response(CompleteStr("CONNECTED W1AW 500"));
        assert_eq!(
            Response::Event(Event::CONNECTED("W1AW".to_owned(), 500, None)),
            res.unwrap().1
        );
    }

    #[test]
    fn test_parse() {
        let data = "PENDING\rCANCELPENDING\r";
        let res = Response::parse(data.as_bytes());
        assert_eq!(8, res.0);
        assert_eq!(Some(Response::Event(Event::PENDING)), res.1);
        let res = Response::parse(&data.as_bytes()[8..]);
        assert_eq!(14, res.0);
        assert_eq!(Some(Response::Event(Event::CANCELPENDING)), res.1);
        let res = Response::parse(&data.as_bytes()[22..]);
        assert_eq!(0, res.0);
        assert_eq!(None, res.1);

        let res = Response::parse("\r\r\r".as_bytes());
        assert_eq!(1, res.0);
        assert_eq!(None, res.1);

        let res = Response::parse("NEWSTATE IRS\r".as_bytes());
        assert_eq!(Some(Response::Event(Event::NEWSTATE(State::IRS))), res.1);
    }
}
