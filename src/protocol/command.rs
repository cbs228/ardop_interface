//! TNC Commands
//!
//! All commands are sent from the host to the TNC.
//! Commands are accepted when they are echoed back
//! and rejected by `FAULT` messages.
#![allow(dead_code)]

use std::convert::Into;
use std::fmt;
use std::string::String;
use std::vec::Vec;

use super::constants as C;
use super::constants::{CommandID, ProtocolMode, NEWLINE_STR};

/// An arbitrary command sent to the TNC
pub struct Command<I>
where
    I: fmt::Display,
{
    id: CommandID,
    args: Option<I>,
}

/// Uncleanly abort the connection
///
/// Immediately aborts an ARQ Connection (dirty disconnect) or a
/// FEC Send session.
///
/// You should use `disconnect()` instead!
pub fn abort() -> Command<arg::NoArg> {
    Command {
        id: CommandID::ABORT,
        args: None,
    }
}

/// Set ARQ bandwidth
///
/// Set/gets the bandwidth for ARQ mode. This sets the maximum negotiated
/// bandwidth r sets the forced bandwidth to a specific value. Attempting
/// to change bandwidth while a connection is in process will generate a
/// FAULT.  If no parameter is given will return the current bandwidth
/// setting. This bandwidth setting applies to all call signs used
/// (`MYCALL` plus optional call signs `MYAUX`)
///
/// Parameters
/// - `bw`: Bandwidth, in Hz. Must be supported by the TNC
/// - `forced`: If true, use only this bandwidth and do not allow
///   negotiations.
pub fn arqbw(bw: u16, forced: bool) -> Command<arg::ArqBw> {
    Command {
        id: CommandID::ARQBW,
        args: Some(arg::ArqBw { bw, forced }),
    }
}

/// Make a new outgoing ARQ connection attempt
///
/// The TNC will attempt to call the given `target`. Acceptance of this
/// command does not imply that the connection has succeeded—merely that
/// the TNC will make the attempt.
///
/// Parameters
/// - `target`: Call sign must be a legitimate call sign,
///   a tactical callsign, or "`CQ`."
/// - `attempts`: Repeat count, 2 -- 15.
pub fn arqcall<S>(target: S, attempts: u16) -> Command<arg::ArqCall>
where
    S: Into<String>,
{
    let s = target.into();
    Command {
        id: CommandID::ARQCALL,
        args: Some(arg::ArqCall {
            target: s,
            attempts,
        }),
    }
}

/// Set ARQ connection timeout
///
/// Set/get the ARQ Timeout in seconds. If no data has flowed in the
/// channel in `timeout` seconds the link is declared dead. A `DISC`
/// command is sent and a reset to the `DISC` state is initiated.
///
/// If either end of the ARQ session hits it’s `ARQTIMEOUT` without
/// data flow the link will automatically be terminated.
///
/// Parameters
/// - `timeout`: ARQ timeout period, in seconds (30 -- 600)
pub fn arqtimeout(timeout: u16) -> Command<u16> {
    Command {
        id: CommandID::ARQTIMEOUT,
        args: Some(timeout),
    }
}

/// Enable or disable autobreak
///
/// Disables/enables automatic link turnover (BREAK) by IRS when IRS has
/// outbound data pending and ISS reaches IDLE state.
///
/// Parameters
/// - `autobreak`: Enable automatic breaks
pub fn autobreak(autobreak: bool) -> Command<arg::BoolArg> {
    Command {
        id: CommandID::AUTOBREAK,
        args: Some(arg::BoolArg { arg: autobreak }),
    }
}

/// Block connections on busy channels
///
/// Set to true to block connection requests until the channel has been
/// non-busy for a certain period of time.
///
/// Parameters
/// - `block`: if true, enable busy channel lockout / blocking
pub fn busyblock(block: bool) -> Command<arg::BoolArg> {
    Command {
        id: CommandID::BUSYBLOCK,
        args: Some(arg::BoolArg { arg: block }),
    }
}

/// Busy detector threshold value
///
/// Sets the current Busy detector threshold value (default = 5). The
/// default value should be sufficient for most installations. Lower
/// values will make the busy detector more sensitive; the channel will
/// be declared busy *more frequently*. Higher values may be used for
/// high-noise environments.
///
/// Parameters
/// - `level`: Busy detector threshold (0 -- 10). A value of 0 will disable
///   the busy detector (not recommended).
pub fn busydet(level: u16) -> Command<u16> {
    Command {
        id: CommandID::BUSYDET,
        args: Some(level),
    }
}

/// Send CW after ID frames
///
/// Set to true to send your callsign in morse code (CW), as station ID,
/// at the end of every ID frame. In many regions, a CW ID is always
/// sufficient to meet station ID requirements. Some regions may
/// require it.
///
/// Parameters
/// - `cw`: Send CW ID with ARDOP digital ID frames
pub fn cwid(cw: bool) -> Command<arg::BoolArg> {
    Command {
        id: CommandID::CWID,
        args: Some(arg::BoolArg { arg: cw }),
    }
}

/// Start disconnect
///
/// Starts an orderly tear-down of an ARQ connection.
/// Disconnection will be confirmed with the remote peer,
/// if possible.
pub fn disconnect() -> Command<arg::NoArg> {
    Command {
        id: CommandID::DISCONNECT,
        args: None,
    }
}

/// Set your station's grid square
///
/// Sets the 4, 6, or 8-character Maidenhead Grid Square for your
/// station. A correct grid square is useful for studying and
/// logging RF propagation-and for bragging rights.
///
/// Your grid square will be sent in ID frames.
///
/// Parameters
/// - `grid`: Your grid square (4, 6, or 8-characters).
pub fn gridsquare<S>(grid: S) -> Command<String>
where
    S: Into<String>,
{
    let s = grid.into();
    Command {
        id: CommandID::GRIDSQUARE,
        args: Some(s),
    }
}

/// Clears any pending queued values in the TNC interface
///
/// All new TCP connections to the TNC should start with this command.
/// This command resets the TNC to initial conditions.
pub fn initialize() -> Command<arg::NoArg> {
    Command {
        id: CommandID::INITIALIZE,
        args: None,
    }
}

/// Leader tone duration
///
/// Sets the leader length in ms. (Default is 160 ms). Rounded to
/// the nearest 20 ms. Note for VOX keying or some SDR radios the
/// leader may have to be extended for reliable decoding.
///
/// Parameters
/// - `duration`: Leader tone duration, milliseconds
pub fn leader(duration: u16) -> Command<u16> {
    Command {
        id: CommandID::LEADER,
        args: Some(duration),
    }
}

/// Listen for incoming connections
///
/// Enables/disables server’s response to an ARQ connect request to
/// `MYCALL` or any of `MYAUX` call signs. Also enables/disables the
/// decoding of a `PING` frame to `MYCALL` or any of the `MYAUX` call
/// signs in either ARQ or FEC modes.
///
/// Incoming connections will be automatically accepted.
///
/// Parameters
/// - `listen`: Enable listening
pub fn listen(listen: bool) -> Command<arg::BoolArg> {
    Command {
        id: CommandID::LISTEN,
        args: Some(arg::BoolArg { arg: listen }),
    }
}

/// Set your station's auxiliary callsigns
///
/// `MYAUX` is only used for `LISTEN`ing, and it will not be used for
/// connect requests.
///
/// Legitimate call signs include from 3 to 7 ASCII characters (A-Z, 0-9)
/// followed by an optional "`-`" and an SSID of `-0` to `-15` or `-A`
/// to `-Z`. An SSID of `-0` is treated as no SSID.
///
/// Parameters:
/// - `aux`: Vector of auxiliary callsigns. If empty, all aux callsigns
///   will be removed.
pub fn myaux(aux: Vec<String>) -> Command<arg::MyAux> {
    Command {
        id: CommandID::MYAUX,
        args: Some(arg::MyAux { aux }),
    }
}

/// Set your station's callsign
///
/// Sets current call sign. If not a valid call generates a FAULT.
/// Legitimate call signs include from 3 to 7 ASCII characters (A-Z, 0-9)
/// followed by an optional "`-`" and an SSID of `-0` to `-15` or `-A`
/// to `-Z`. An SSID of `-0` is treated as no SSID.
///
/// Parameters
/// - `callsign`: Assigned, proper callsign for this station
pub fn mycall<S>(callsign: S) -> Command<String>
where
    S: Into<String>,
{
    let s = callsign.into();
    Command {
        id: CommandID::MYCALL,
        args: Some(s),
    }
}

/// Send a ping request
///
/// If the target callsign is not connected, decodes a PING, and has
/// `ENABLEPINGACK` and `LISTEN` set, it will reply with a `PINGACK`
/// which includes the received PING S:N and decode quality.
/// A properly decoded `PINGACK` will terminate the Ping.
///
/// Parameters
/// - `target`: Target callsign, which may be a tactical call.
/// - `attempts`: Repeat count, 2 -- 15.
pub fn ping<S>(target: S, attempts: u16) -> Command<arg::ArqCall>
where
    S: Into<String>,
{
    let s = target.into();
    Command {
        id: CommandID::PING,
        args: Some(arg::ArqCall {
            target: s,
            attempts,
        }),
    }
}

/// Set protocol mode
///
/// Selects the TNC's mode of operation.
///
/// Parameters
/// - `mode`: `FEC` or `ARQ`
pub fn protocolmode(mode: ProtocolMode) -> Command<ProtocolMode> {
    Command {
        id: CommandID::PROTOCOLMODE,
        args: Some(mode),
    }
}

/// Send ID frame
///
/// Sends an ID frame immediately, followed by a CW ID (if `CWID` is set)
pub fn sendid() -> Command<arg::NoArg> {
    Command {
        id: CommandID::SENDID,
        args: None,
    }
}

/// Start a two-tone test
///
/// Send 5 second two-tone burst, at the normal leader amplitude. May
/// be used in adjusting drive level to the radio.
pub fn twotonetest() -> Command<arg::NoArg> {
    Command {
        id: CommandID::TWOTONETEST,
        args: None,
    }
}

/// Query version
///
/// Query the software version of the TNC.
pub fn version() -> Command<arg::NoArg> {
    Command {
        id: CommandID::VERSION,
        args: None,
    }
}

impl<I> Command<I>
where
    I: fmt::Display,
{
    /// Command identifier
    pub fn command_id(&self) -> &CommandID {
        &self.id
    }
}

// generic formatter
impl<I> fmt::Display for Command<I>
where
    I: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.args {
            None => write!(f, "{}{}", self.command_id(), NEWLINE_STR),
            Some(x) => write!(f, "{} {}{}", self.command_id(), x, NEWLINE_STR),
        }
    }
}

mod arg {
    use super::C;

    use std::fmt;
    use std::string::String;
    use std::vec;

    // No arguments
    pub struct NoArg {}

    impl fmt::Display for NoArg {
        fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
            Ok(())
        }
    }

    // A single boolean argument
    pub struct BoolArg {
        pub arg: bool,
    }

    impl fmt::Display for BoolArg {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{}", C::truth_str(self.arg),)
        }
    }

    // Set ARQ bandwidth negotiation
    pub struct ArqBw {
        pub bw: u16,
        pub forced: bool,
    }

    impl fmt::Display for ArqBw {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            const FORCED: &'static [&'static str] = &["MAX", "FORCED"];

            write!(f, "{}{}", self.bw, FORCED[self.forced as usize])
        }
    }

    // Make an outgoing ARQ connection attempt
    pub struct ArqCall {
        pub target: String,
        pub attempts: u16,
    }

    impl fmt::Display for ArqCall {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{} {}", self.target, self.attempts)
        }
    }

    // Set auxiliary callsign(s)
    pub struct MyAux {
        pub aux: vec::Vec<String>,
    }

    impl fmt::Display for MyAux {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            if self.aux.is_empty() {
                write!(f, "X")
            } else {
                write!(f, "{}", self.aux.join(","))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_abort() {
        let cmd = abort();
        let cmdstring = format!("{}", cmd);
        assert_eq!("ABORT\r", cmdstring);
    }

    #[test]
    fn test_autobreak() {
        let cmd = autobreak(false);
        let cmdstring = format!("{}", cmd);
        assert_eq!("AUTOBREAK FALSE\r", cmdstring);
    }

    #[test]
    fn test_mycall() {
        let cmd = mycall("W1AW");
        let cmdstring = format!("{}", cmd);
        assert_eq!("MYCALL W1AW\r", cmdstring);
    }

    #[test]
    fn test_arqbw() {
        let cmd = arqbw(500, true);
        let cmdstring = format!("{}", cmd);
        assert_eq!("ARQBW 500FORCED\r", cmdstring);

        let cmd = arqbw(2500, false);
        let cmdstring = format!("{}", cmd);
        assert_eq!("ARQBW 2500MAX\r", cmdstring);
    }

    #[test]
    fn test_arqcall() {
        let cmd = arqcall("W1AW-10", 5);
        let cmdstring = format!("{}", cmd);
        assert_eq!("ARQCALL W1AW-10 5\r", cmdstring);
    }

    #[test]
    fn test_protocolmode() {
        let cmd = protocolmode(ProtocolMode::FEC);
        let cmdstring = format!("{}", cmd);
        assert_eq!("PROTOCOLMODE FEC\r", cmdstring);
    }

    #[test]
    fn test_myaux() {
        let cmd = myaux(vec![]);
        let cmdstring = format!("{}", cmd);
        assert_eq!("MYAUX X\r", cmdstring);

        let cmd = myaux(vec!["W1AW-1".to_owned()]);
        let cmdstring = format!("{}", cmd);
        assert_eq!("MYAUX W1AW-1\r", cmdstring);

        let cmd = myaux(vec!["W1AW-1".to_owned(), "W1AW-Z".to_owned()]);
        let cmdstring = format!("{}", cmd);
        assert_eq!("MYAUX W1AW-1,W1AW-Z\r", cmdstring);
    }

    #[test]
    fn test_ping() {
        let cmd = ping("W1AW-10", 5);
        let cmdstring = format!("{}", cmd);
        assert_eq!("PING W1AW-10 5\r", cmdstring);
    }
}
