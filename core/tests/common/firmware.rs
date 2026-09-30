// Modèle du module firmware "Var" (D2 B2), miroir ligne à ligne de
// `firmware/qmk-var/src/var.c`. Sert à alimenter le Core dans les tests
// d'équivalence exactement comme le ferait le clavier. Toute modification
// du C doit être répercutée ici (et inversement).
#![allow(dead_code)]

use std::collections::HashSet;

use omnikey_core::protocol::{DeviceFrame, HostFrame};
use omnikey_core::{BaudotEvent, KeyId};

/// Touche physique du modèle.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Phys {
    Var,
    Shift,
    Caps,
    Pos(KeyId),
}

/// Ce que le firmware fait d'un événement de touche.
#[derive(Debug, PartialEq, Eq)]
pub enum FwOut {
    /// L'événement part normalement vers l'OS.
    Pass,
    /// L'événement est supprimé côté OS, rien n'est émis.
    Nothing,
    /// L'événement est supprimé côté OS et cette trame est émise.
    Frame([u8; 32]),
}

pub struct Firmware {
    pub host_alive: bool,
    pub held: bool,
    pub armed: bool,
    pub arm_requested: bool,
    redirected: HashSet<KeyId>,
    seq: u8,
}

impl Firmware {
    pub fn new(host_alive: bool) -> Self {
        Self { host_alive, held: false, armed: false, arm_requested: false, redirected: HashSet::new(), seq: 0 }
    }

    fn frame(&mut self, event: BaudotEvent) -> FwOut {
        let f = DeviceFrame::Event { seq: self.seq, event }.encode();
        self.seq = self.seq.wrapping_add(1);
        FwOut::Frame(f)
    }

    /// `shift` / `caps` : état au moment de l'événement (mods QMK, LED hôte).
    /// Seul leur XOR voyage dans la trame (D1 AM7).
    pub fn key(&mut self, phys: Phys, pressed: bool, shift: bool, caps: bool) -> FwOut {
        let upper = shift ^ caps;
        let id = match phys {
            Phys::Var => {
                if pressed {
                    self.armed = false;
                    self.arm_requested = false;
                    if self.host_alive {
                        self.held = true;
                        return self.frame(BaudotEvent::VarDown);
                    }
                } else if self.held {
                    self.held = false;
                    let f = self.frame(BaudotEvent::VarUp);
                    self.armed = self.arm_requested && self.host_alive;
                    self.arm_requested = false;
                    return f;
                }
                return FwOut::Nothing; // Var n'émet jamais de scancode (M2)
            }
            // Maj et Verr. Maj. ne sont jamais dans la table.
            Phys::Shift | Phys::Caps => return FwOut::Pass,
            Phys::Pos(id) => id,
        };

        if !pressed {
            if !self.redirected.remove(&id) {
                return FwOut::Pass;
            }
            return self.frame(BaudotEvent::KeyUp { id, upper });
        }
        if self.held || (self.armed && id.is_armable()) {
            self.redirected.insert(id);
            return self.frame(BaudotEvent::KeyDown { id, upper });
        }
        FwOut::Pass
    }

    pub fn host(&mut self, bytes: &[u8]) {
        let Ok(frame) = HostFrame::decode(bytes) else { return };
        match frame {
            HostFrame::HeartbeatAck => {}
            HostFrame::SetArm(true) => {
                if self.held && self.host_alive {
                    self.arm_requested = true;
                }
            }
            HostFrame::SetArm(false) => {
                self.arm_requested = false;
                self.armed = false;
            }
        }
    }

    /// Perte du software (plus de réponse au Heartbeat).
    pub fn host_lost(&mut self) {
        self.host_alive = false;
        self.held = false;
        self.armed = false;
        self.arm_requested = false;
    }
}
