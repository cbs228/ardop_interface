use std::convert::Into;
use std::fmt;
use std::string::String;

/// ARDOP Ping Response
///
/// Indicates that a *solicited* ping reply has been
/// received from a remote peer.
pub struct PingAck {
    peer: String,
    snr: u16,
    decode_quality: u16,
}

impl PingAck {
    // Construct
    pub(crate) fn new<S>(peer: S, snr: u16, decode_quality: u16) -> Self
    where
        S: Into<String>,
    {
        Self {
            peer: peer.into(),
            snr,
            decode_quality,
        }
    }

    /// Peer callsign, with optional -SSID portion
    pub fn peer(&self) -> &String {
        &self.peer
    }

    /// Signal-to-noise ratio (SNR)
    ///
    /// SNR in dB, relative to a 3 kHz noise bandwidth. A value of
    /// of 21 indicates that the SNR is above 20 dB.
    pub fn snr(&self) -> u16 {
        self.snr
    }

    /// Symbol constellation decoding quality
    ///
    /// Quality values range from 30 ­ 100.
    pub fn decode_quality(&self) -> u16 {
        self.decode_quality
    }
}

impl fmt::Display for PingAck {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Ping {}: SNR {} dB - Quality {}",
            &self.peer, self.snr, self.decode_quality
        )
    }
}
