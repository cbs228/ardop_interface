use std::convert::Into;
use std::fmt;
use std::string::String;

/// ARDOP Discovered Peer
///
/// Announces a peer that has been discovered from its
/// ping requests or identity frames.
pub struct DiscoveredPeer {
    peer: String,
    snr: Option<u16>,
    grid_square: Option<String>,
}

impl DiscoveredPeer {
    // Construct
    pub(crate) fn new<S>(peer: S, snr: Option<u16>, grid_square: Option<String>) -> Self
    where
        S: Into<String>,
    {
        Self {
            peer: peer.into(),
            snr,
            grid_square,
        }
    }

    /// Peer callsign, with optional -SSID portion
    pub fn peer(&self) -> &String {
        &self.peer
    }

    /// Signal-to-noise ratio (SNR)
    ///
    /// SNR in dB, relative to a 3 kHz noise bandwidth. A value of
    /// of 21 indicates that the SNR is above 20 dB. If unknown,
    /// returns None.
    pub fn snr(&self) -> Option<u16> {
        self.snr
    }

    /// Peer Maidenhead Grid Square
    ///
    /// Returns the peer's self-reported grid square, if known.
    /// If unknown, returns None.
    pub fn grid_square(&self) -> &Option<String> {
        &self.grid_square
    }
}

impl fmt::Display for DiscoveredPeer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.peer)?;
        match &self.grid_square {
            None => write!(f, " [????]")?,
            Some(gr) => write!(f, " [{}]", gr)?,
        }
        match self.snr {
            None => write!(f, ""),
            Some(snr) => write!(f, ": {} dB", snr),
        }
    }
}
