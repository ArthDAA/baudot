//! Trames Baudot (D2 B1) : rapports HID bruts de 32 octets, dans les deux sens.
//!
//! ```text
//! [0] préfixe 0xBD  [1] version  [2] type  [3] séquence  [4] Key ID / argument
//! [5] drapeaux (bit0 majuscule = Maj XOR Verr. Maj., calculé par le clavier)
//! [6..32] réservés, à zéro
//! ```
//!
//! Doit rester identique à `firmware/qmk-var/src/baudot_protocol.h`.

use crate::key::KeyId;

pub const FRAME_SIZE: usize = 32;
pub const MAGIC: u8 = 0xBD;
pub const VERSION: u8 = 1;

/// Maj tenue XOR Verr. Maj. actif, relevé par le clavier (D1 AM7).
pub const FLAG_UPPER: u8 = 1 << 0;

// Clavier -> software
pub const T_VAR_DOWN: u8 = 0x01;
pub const T_VAR_UP: u8 = 0x02;
pub const T_KEY_DOWN: u8 = 0x03;
pub const T_KEY_UP: u8 = 0x04;
pub const T_HEARTBEAT: u8 = 0x05;
// Software -> clavier
pub const T_HEARTBEAT_ACK: u8 = 0x81;
pub const T_SET_ARM: u8 = 0x82;

/// Événement utile au cœur, extrait d'une trame clavier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BaudotEvent {
    VarDown,
    VarUp,
    KeyDown { id: KeyId, upper: bool },
    KeyUp { id: KeyId, upper: bool },
}

/// Trame reçue du clavier, décodée.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeviceFrame {
    Event { seq: u8, event: BaudotEvent },
    Heartbeat { seq: u8 },
}

/// Trame envoyée au clavier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostFrame {
    HeartbeatAck,
    SetArm(bool),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameError {
    /// Pas une trame Baudot (réponse VIA ou Keychron) : à ignorer sans bruit.
    NotBaudot,
    /// Trame Baudot d'une autre version de protocole.
    VersionMismatch(u8),
    /// Type inconnu, Key ID hors table, ou trame trop courte.
    Malformed,
}

impl DeviceFrame {
    pub fn decode(buf: &[u8]) -> Result<DeviceFrame, FrameError> {
        if buf.first() != Some(&MAGIC) {
            return Err(FrameError::NotBaudot);
        }
        if buf.len() < 6 {
            return Err(FrameError::Malformed);
        }
        if buf[1] != VERSION {
            return Err(FrameError::VersionMismatch(buf[1]));
        }
        let seq = buf[3];
        let id = KeyId(buf[4]);
        let upper = buf[5] & FLAG_UPPER != 0;
        let event = match buf[2] {
            T_VAR_DOWN => BaudotEvent::VarDown,
            T_VAR_UP => BaudotEvent::VarUp,
            T_KEY_DOWN if id.is_valid() => BaudotEvent::KeyDown { id, upper },
            T_KEY_UP if id.is_valid() => BaudotEvent::KeyUp { id, upper },
            T_HEARTBEAT => return Ok(DeviceFrame::Heartbeat { seq }),
            _ => return Err(FrameError::Malformed),
        };
        Ok(DeviceFrame::Event { seq, event })
    }

    /// Encodage côté clavier : sert au modèle du firmware dans les tests et
    /// aux outils de diagnostic.
    pub fn encode(&self) -> [u8; FRAME_SIZE] {
        let mut f = [0u8; FRAME_SIZE];
        f[0] = MAGIC;
        f[1] = VERSION;
        match *self {
            DeviceFrame::Heartbeat { seq } => {
                f[2] = T_HEARTBEAT;
                f[3] = seq;
            }
            DeviceFrame::Event { seq, event } => {
                f[3] = seq;
                let (t, id, upper) = match event {
                    BaudotEvent::VarDown => (T_VAR_DOWN, KeyId(0), false),
                    BaudotEvent::VarUp => (T_VAR_UP, KeyId(0), false),
                    BaudotEvent::KeyDown { id, upper } => (T_KEY_DOWN, id, upper),
                    BaudotEvent::KeyUp { id, upper } => (T_KEY_UP, id, upper),
                };
                f[2] = t;
                f[4] = id.0;
                f[5] = if upper { FLAG_UPPER } else { 0 };
            }
        }
        f
    }
}

impl HostFrame {
    pub fn encode(&self, seq: u8) -> [u8; FRAME_SIZE] {
        let mut f = [0u8; FRAME_SIZE];
        f[0] = MAGIC;
        f[1] = VERSION;
        f[3] = seq;
        match *self {
            HostFrame::HeartbeatAck => f[2] = T_HEARTBEAT_ACK,
            HostFrame::SetArm(on) => {
                f[2] = T_SET_ARM;
                f[4] = on as u8;
            }
        }
        f
    }

    pub fn decode(buf: &[u8]) -> Result<HostFrame, FrameError> {
        if buf.first() != Some(&MAGIC) {
            return Err(FrameError::NotBaudot);
        }
        if buf.len() < 6 {
            return Err(FrameError::Malformed);
        }
        if buf[1] != VERSION {
            return Err(FrameError::VersionMismatch(buf[1]));
        }
        match buf[2] {
            T_HEARTBEAT_ACK => Ok(HostFrame::HeartbeatAck),
            T_SET_ARM => Ok(HostFrame::SetArm(buf[4] != 0)),
            _ => Err(FrameError::Malformed),
        }
    }
}

/// Suivi des numéros de séquence d'un flux clavier (D2 B1 : un trou de
/// séquence force le cœur à Idle).
#[derive(Default)]
pub struct SeqTracker {
    last: Option<u8>,
}

impl SeqTracker {
    /// Vrai si la trame suit directement la précédente (ou si c'est la
    /// première vue).
    pub fn accept(&mut self, seq: u8) -> bool {
        let ok = match self.last {
            None => true,
            Some(prev) => seq == prev.wrapping_add(1),
        };
        self.last = Some(seq);
        ok
    }

    pub fn reset(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_frames_round_trip() {
        let frames = [
            DeviceFrame::Heartbeat { seq: 7 },
            DeviceFrame::Event { seq: 255, event: BaudotEvent::VarDown },
            DeviceFrame::Event { seq: 0, event: BaudotEvent::VarUp },
            DeviceFrame::Event { seq: 1, event: BaudotEvent::KeyDown { id: KeyId::E, upper: true } },
            DeviceFrame::Event { seq: 2, event: BaudotEvent::KeyUp { id: KeyId::F12, upper: false } },
        ];
        for f in frames {
            let bytes = f.encode();
            assert_eq!(bytes.len(), FRAME_SIZE);
            assert!(bytes[6..].iter().all(|&b| b == 0));
            assert_eq!(DeviceFrame::decode(&bytes), Ok(f));
        }
    }

    #[test]
    fn host_frames_round_trip() {
        for f in [HostFrame::HeartbeatAck, HostFrame::SetArm(true), HostFrame::SetArm(false)] {
            assert_eq!(HostFrame::decode(&f.encode(3)), Ok(f));
        }
    }

    #[test]
    fn foreign_and_bad_frames_are_rejected() {
        // Réponse VIA "unhandled" et réponse Keychron.
        assert_eq!(DeviceFrame::decode(&[0xFF; 32]), Err(FrameError::NotBaudot));
        assert_eq!(DeviceFrame::decode(&[0xA0, 2, 0, 2]), Err(FrameError::NotBaudot));
        let mut f = DeviceFrame::Heartbeat { seq: 0 }.encode();
        f[1] = 9;
        assert_eq!(DeviceFrame::decode(&f), Err(FrameError::VersionMismatch(9)));
        let mut f = DeviceFrame::Event { seq: 0, event: BaudotEvent::VarDown }.encode();
        f[2] = T_KEY_DOWN;
        f[4] = 0; // Key ID absent
        assert_eq!(DeviceFrame::decode(&f), Err(FrameError::Malformed));
        f[4] = KeyId::MAX + 1;
        assert_eq!(DeviceFrame::decode(&f), Err(FrameError::Malformed));
    }

    #[test]
    fn key_frames_carry_only_the_uppercase_bit() {
        for upper in [false, true] {
            let f = DeviceFrame::Event { seq: 0, event: BaudotEvent::KeyDown { id: KeyId::A, upper } }.encode();
            assert_eq!(f[5], upper as u8);
        }
    }

    #[test]
    fn heartbeat_carries_no_key_and_no_flags() {
        let f = DeviceFrame::Heartbeat { seq: 42 }.encode();
        assert_eq!((f[4], f[5]), (0, 0));
    }

    #[test]
    fn sequence_gaps_are_detected() {
        let mut t = SeqTracker::default();
        assert!(t.accept(254));
        assert!(t.accept(255));
        assert!(t.accept(0));
        assert!(!t.accept(2));
        assert!(t.accept(3));
    }
}
