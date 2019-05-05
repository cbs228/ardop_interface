/// Line delimiter in ARDOP control messages
pub const NEWLINE: u8 = b'\r';

/// Line delimiter in ARDOP control messages (str)
pub const NEWLINE_STR: &'static str = "\r";

/// TNC "true" value
pub const TRUE: &'static str = "TRUE";

/// TNC "false" value
pub const FALSE: &'static str = "FALSE";

custom_derive! {
    /// ARDOP protocol modes
    #[derive(Debug, PartialEq, Eq, EnumFromStr, EnumDisplay, Clone)]
    pub enum ProtocolMode {
        /// Connectionless, forward error-corrected packets
        FEC,

        /// Reliable, connection-oriented protocol with retransmits
        ARQ
    }
}

custom_derive! {
    /// ARDOP command verbs
    #[derive(Debug, PartialEq, Eq, EnumFromStr, EnumDisplay, Clone)]
    pub enum CommandID {
        /// Unclean disconnect
        ABORT,

        /// Attempt link turnover when remote buffer empties
        AUTOBREAK,

        /// Report buffer status
        BUFFER,

        /// Set ARQ call bandwidth
        ARQBW,

        /// Set ARQ timeout
        ARQTIMEOUT,

        /// Attempt ARQ connection
        ARQCALL,

        /// Reports that the RF channel is busy
        BUSY,

        /// Block connection requests when the channel has been busy
        BUSYBLOCK,

        /// Set busy detector threshold (0 -- 10)
        BUSYDET,

        /// Reports successful connection attempt
        CONNECTED,

        /// Send CW ID after a SENDID
        CWID,

        /// Initiate clean disconnect
        DISCONNECT,

        /// TNC command error
        FAULT,

        /// Set Maidenhead grid square
        GRIDSQUARE,

        /// Clear TNC states
        INITIALIZE,

        /// Set leader tone duration (120 -- 2500)
        LEADER,

        /// Enable/disable listening for incoming
        LISTEN,

        /// Reports transitioning to a new State
        NEWSTATE,

        /// Sets my tactical callsigns for listening
        MYAUX,

        /// Sets my callsign and SSID
        MYCALL,

        /// Send a ping request
        PING,

        /// Change the ProtocolMode
        PROTOCOLMODE,

        /// Reports connection rejected by peer
        REJECTED,

        /// Transmit an ID/beacon frame now
        SENDID,

        /// Reports an incoming call directed at this callsign (MYCALL or one of MYAUX)
        TARGET,

        /// Perform a two-tone test for audio calibration
        TWOTONETEST,

        /// Requests and reports TNC software version
        VERSION
    }
}

/// Map a truth value to a TNC string
#[inline]
pub fn truth_str(v: bool) -> &'static str {
    match v {
        true => TRUE,
        false => FALSE,
    }
}

#[cfg(test)]
mod test {
    use std::str;

    use super::*;

    #[test]
    fn test_protocol_mode() {
        assert_eq!(b"FEC", format!("{}", ProtocolMode::FEC).as_bytes());
        assert_eq!(ProtocolMode::ARQ, str::parse("ARQ").unwrap());
    }

    #[test]
    fn test_command() {
        assert_eq!(
            b"PROTOCOLMODE",
            format!("{}", CommandID::PROTOCOLMODE).as_bytes()
        );
        assert_eq!(CommandID::ABORT, str::parse("ABORT").unwrap());
    }
}
