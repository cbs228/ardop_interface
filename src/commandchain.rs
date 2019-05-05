//! A fancy interface for chained commands
//!
//! This module defines a "chain" of commands, which is syntactic
//! sugar for executing multiple ARDOP TNC commands in sequence.
//! For further details, see the module-level documentation for
//! `tnc`.
use std::collections::vec_deque::VecDeque;
use std::fmt;
use std::io;
use std::io::ErrorKind;
use std::string::String;

use crate::protocol::command;
use crate::protocol::{CommandID, CommandOk};
use crate::tncerror::{TncError, TncResult};

/// An object which can be commanded
///
/// Commandable objects can accept a single command, process it
/// synchronously, and return its success or failure.
pub trait Commandable {
    /// Send a single command string
    ///
    /// This method may be used to send arbitrary, unformatted
    /// commands to the TNC. Commands sent in this manner must be
    /// complete, including the trailing `\r` newline separator.
    ///
    /// You should probably use the high-level command methods instead.
    /// See `Tnc::command` for the high-level interface.
    ///
    /// # Parameters
    /// * `cmd`: Raw command string
    ///
    /// # Returns
    /// An empty `Result` if the command was enqueued for sending, or
    /// an `io::Error` otherwise.
    fn send_raw_command(&mut self, cmd: String) -> TncResult<()>;

    /// Blocks until a command response is received
    ///
    /// Waits for acknowledgement of the given command, by ID,
    /// to be returned from the TNC. If no response is received
    /// from the TNC, this method will eventually timeout.
    ///
    /// # Parameters
    /// * `id`: Command ID to wait for
    ///
    /// # returns
    /// A `Result` with the following structure:
    /// * `Ok`: A tuple of
    ///   - The command ID which was sent successfully
    ///   - An optional string response from the TNC. At present,
    ///     only populated for `VERSION` messages
    /// * `Err`: Command failure
    fn await_command_result(&mut self, id: CommandID) -> TncResult<CommandOk>;
}

pub struct CommandChain<'d, D: 'd>
where
    D: Commandable,
{
    dest: &'d mut D,
    cmds: VecDeque<(String, CommandID)>,
}

impl<'d, D> CommandChain<'d, D>
where
    D: Commandable,
{
    /// Create a chain of commands
    ///
    /// The command chain is bound to `dest`, which must exist for
    /// the lifetime of this chain.
    pub fn new(dest: &'d mut D) -> CommandChain<'d, D> {
        let cmds = VecDeque::with_capacity(16);

        CommandChain { dest, cmds }
    }

    /// Send all commands
    ///
    /// Transmits all the commands in this chain. Returns either:
    ///
    /// * The first error, if any; OR
    /// * The last successful result
    pub fn send(&'d mut self) -> TncResult<CommandOk> {
        let mut last: TncResult<CommandOk> = Err(TncError::IoError(io::Error::new(
            ErrorKind::UnexpectedEof,
            "No commands provided",
        )));

        while !self.cmds.is_empty() {
            let cm = self.cmds.pop_front().unwrap();
            self.dest.send_raw_command(cm.0)?;
            last = Ok(self.dest.await_command_result(cm.1)?);
        }
        return last;
    }

    /// Uncleanly abort the connection
    ///
    /// Immediately aborts an ARQ Connection (dirty disconnect) or a
    /// FEC Send session.
    ///
    /// You should use `disconnect()` instead!
    pub fn abort(&'d mut self) -> &'d mut Self {
        self.append(command::abort());
        self
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
    /// # Parameters
    /// - `bw`: Bandwidth, in Hz. Must be supported by the TNC
    /// - `forced`: If true, use only this bandwidth and do not allow
    ///   negotiations.
    pub fn arqbw(&'d mut self, bw: u16, forced: bool) -> &'d mut Self {
        {
            self.append(command::arqbw(bw, forced));
        }
        self
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
    /// # Parameters
    /// - `timeout`: ARQ timeout period, in seconds (30 -- 600)
    pub fn arqtimeout(&'d mut self, timeout: u16) -> &'d mut Self {
        {
            self.append(command::arqtimeout(timeout));
        }
        self
    }

    /// Enable or disable autobreak
    ///
    /// Disables/enables automatic link turnover (BREAK) by IRS when IRS has
    /// outbound data pending and ISS reaches IDLE state.
    ///
    /// # Parameters
    /// - `autobreak`: Enable automatic breaks
    pub fn autobreak(&'d mut self, autobreak: bool) -> &'d mut Self {
        self.append(command::autobreak(autobreak));
        self
    }

    /// Block connections on busy channels
    ///
    /// Set to true to block connection requests until the channel has been
    /// non-busy for a certain period of time.
    ///
    /// # Parameters
    /// - `block`: if true, enable busy channel lockout / blocking
    pub fn busyblock(&'d mut self, block: bool) -> &'d mut Self {
        self.append(command::busyblock(block));
        self
    }

    /// Busy detector threshold value
    ///
    /// Sets the current Busy detector threshold value (default = 5). The
    /// default value should be sufficient for most installations. Lower
    /// values will make the busy detector more sensitive; the channel will
    /// be declared busy *more frequently*. Higher values may be used for
    /// high-noise environments.
    ///
    /// # Parameters
    /// - `level`: Busy detector threshold (0 -- 10). A value of 0 will disable
    ///   the busy detector (not recommended).
    pub fn busydet(&'d mut self, level: u16) -> &'d mut Self {
        self.append(command::busydet(level));
        self
    }

    /// Send CW after ID frames
    ///
    /// Set to true to send your callsign in morse code (CW), as station ID,
    /// at the end of every ID frame. In many regions, a CW ID is always
    /// sufficient to meet station ID requirements. Some regions may
    /// require it.
    ///
    /// # Parameters
    /// - `cw`: Send CW ID with ARDOP digital ID frames
    pub fn cwid(&'d mut self, cw: bool) -> &'d mut Self {
        self.append(command::cwid(cw));
        self
    }

    /// Start disconnect
    ///
    /// Starts an orderly tear-down of an ARQ connection.
    /// Disconnection will be confirmed with the remote peer,
    /// if possible.
    pub fn disconnect(&'d mut self) -> &'d mut Self {
        self.append(command::disconnect());
        self
    }

    /// Set your station's grid square
    ///
    /// Sets the 4, 6, or 8-character Maidenhead Grid Square for your
    /// station. A correct grid square is useful for studying and
    /// logging RF propagation-and for bragging rights.
    ///
    /// Your grid square will be sent in ID frames.
    ///
    /// # Parameters
    /// - `grid`: Your grid square (4, 6, or 8-characters).
    pub fn gridsquare<S>(&'d mut self, grid: S) -> &'d mut Self
    where
        S: Into<String>,
    {
        self.append(command::gridsquare(grid));
        self
    }

    /// Leader tone duration
    ///
    /// Sets the leader length in ms. (Default is 160 ms). Rounded to
    /// the nearest 20 ms. Note for VOX keying or some SDR radios the
    /// leader may have to be extended for reliable decoding.
    ///
    /// # Parameters
    /// - `duration`: Leader tone duration, milliseconds
    pub fn leader(&'d mut self, duration: u16) -> &'d mut Self {
        self.append(command::leader(duration));
        self
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
    /// # Parameters:
    /// - `aux`: Vector of auxiliary callsigns. If empty, all aux callsigns
    ///   will be removed.
    pub fn myaux(&'d mut self, aux: Vec<String>) -> &'d mut Self {
        self.append(command::myaux(aux));
        self
    }

    /// Set your station's callsign
    ///
    /// Sets current call sign. If not a valid call generates a FAULT.
    /// Legitimate call signs include from 3 to 7 ASCII characters (A-Z, 0-9)
    /// followed by an optional "`-`" and an SSID of `-0` to `-15` or `-A`
    /// to `-Z`. An SSID of `-0` is treated as no SSID.
    ///
    /// # Parameters
    /// - `callsign`: Assigned, proper callsign for this station
    pub fn mycall<S>(&'d mut self, callsign: S) -> &'d mut Self
    where
        S: Into<String>,
    {
        self.append(command::mycall(callsign));
        self
    }

    /// Send ID frame
    ///
    /// Sends an ID frame immediately, followed by a CW ID (if `CWID` is set)
    pub fn sendid(&'d mut self) -> &'d mut Self {
        self.append(command::sendid());
        self
    }

    /// Start a two-tone test
    ///
    /// Send 5 second two-tone burst, at the normal leader amplitude. May
    /// be used in adjusting drive level to the radio.
    pub fn twotonetest(&'d mut self) -> &'d mut Self {
        self.append(command::twotonetest());
        self
    }

    /// Query version
    ///
    /// Query the software version of the TNC.
    pub fn version(&'d mut self) -> &'d mut Self {
        self.append(command::version());
        self
    }

    // Append a command
    fn append<I>(&mut self, command: command::Command<I>)
    where
        I: fmt::Display,
    {
        self.cmds
            .push_back((command.to_string(), command.command_id().clone()));
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::protocol::CommandID;

    struct Commando {
        pub count: usize,
        pub last: String,
    }

    impl<'d> Commando {
        fn commands(&'d mut self) -> CommandChain<'d, Commando> {
            CommandChain::new(self)
        }
    }

    impl Commandable for Commando {
        fn send_raw_command(&mut self, cmd: String) -> TncResult<()> {
            println!("Got {}", cmd);
            self.count += 1;
            self.last = cmd;
            Ok(())
        }

        fn await_command_result(&mut self, id: CommandID) -> TncResult<CommandOk> {
            Ok((id, None))
        }
    }

    #[test]
    fn test_none() {
        let mut tnc = Commando {
            count: 0,
            last: "".to_owned(),
        };

        assert!(tnc.commands().send().is_err());
        assert_eq!(tnc.count, 0);
    }

    #[test]
    fn test_single() {
        let mut tnc = Commando {
            count: 0,
            last: "".to_owned(),
        };

        tnc.commands().abort().send().unwrap();

        assert_eq!(tnc.count, 1);
        assert_eq!(tnc.last, "ABORT\r");
    }

    #[test]
    fn test_multiple() {
        let mut tnc = Commando {
            count: 0,
            last: "".to_owned(),
        };

        tnc.commands().abort().mycall("W1AW-C").send().unwrap();

        assert_eq!(tnc.count, 2);
        assert_eq!(tnc.last, "MYCALL W1AW-C\r");
    }
}
