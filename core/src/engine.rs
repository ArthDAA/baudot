//! Relie les événements du clavier à la machine à états (D2 B4).
//!
//! Pour chaque [`BaudotEvent`], l'[`Engine`] :
//! 1. traduit la position en touche logique via le [`KeyResolver`] de la
//!    plateforme (disposition active de l'OS, comme v0.1) ;
//! 2. calcule l'auto-répétition comme le faisait le hook v0.1 ;
//! 3. passe la touche à la machine à états ;
//! 4. traduit l'action en ordres : texte à écrire, touche à restituer ;
//! 5. synchronise l'armement du clavier (trame SetArm) avec l'état voulu.

use std::collections::{HashMap, HashSet};

use crate::key::{logical, KeyId, LogicalKey};
use crate::machine::{Action, HudMsg, Layout, StateMachine};
use crate::protocol::{BaudotEvent, HostFrame};

/// Traduit une position physique en touche logique v0.1, selon la
/// disposition active de l'OS. Fourni par la plateforme.
pub trait KeyResolver {
    fn resolve(&self, id: KeyId) -> LogicalKey;
}

/// Ordre vers l'Injector (D2 B7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    /// Écrire ces unités UTF-16 d'un seul bloc.
    Write(Vec<u16>),
    /// Restituer l'appui ou le relâchement de cette position, comme une
    /// touche physique : c'est le PassThrough de v0.1.
    Forward { id: KeyId, down: bool },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decision {
    pub orders: Vec<Order>,
    pub hud: Vec<HudMsg>,
    pub to_keyboard: Vec<HostFrame>,
}

pub struct Engine {
    machine: StateMachine,
    /// Positions enfoncées et la touche logique résolue à l'appui : le
    /// relâchement doit porter la même, même si la disposition a changé
    /// entre-temps.
    down: HashMap<KeyId, LogicalKey>,
    /// Positions dont l'appui a été restitué à l'OS et pas encore le relâchement.
    forwarded: HashSet<KeyId>,
    /// Dernier état d'armement demandé au clavier, tel que le clavier le voit.
    keyboard_armed: bool,
    shifted: bool,
}

impl Engine {
    pub fn new(layout: Layout) -> Self {
        Self { machine: StateMachine::new(layout), down: HashMap::new(), forwarded: HashSet::new(), keyboard_armed: false, shifted: false }
    }

    pub fn machine(&self) -> &StateMachine {
        &self.machine
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.machine.set_layout(layout);
    }

    pub fn handle(&mut self, event: BaudotEvent, resolver: &dyn KeyResolver) -> Decision {
        let mut d = Decision::default();
        let (action, id) = match event {
            BaudotEvent::VarDown => {
                // Le clavier efface son armement à chaque appui de Var.
                self.keyboard_armed = false;
                (self.machine.process_key(logical::VAR, true, self.shifted, false), None)
            }
            BaudotEvent::VarUp => (self.machine.process_key(logical::VAR, false, self.shifted, false), None),
            BaudotEvent::KeyDown { id, upper } => {
                self.shifted = upper;
                let (vk, repeat) = match self.down.get(&id) {
                    Some(&vk) => (vk, true),
                    None => {
                        let vk = resolver.resolve(id);
                        self.down.insert(id, vk);
                        (vk, false)
                    }
                };
                (self.machine.process_key(vk, true, self.shifted, repeat), Some((id, true)))
            }
            BaudotEvent::KeyUp { id, upper } => {
                self.shifted = upper;
                let vk = self.down.remove(&id).unwrap_or_else(|| resolver.resolve(id));
                (self.machine.process_key(vk, false, self.shifted, false), Some((id, false)))
            }
        };

        match action {
            // Seule dérogation à v0.1 : une touche dont l'appui a été restitué
            // à l'OS voit toujours son relâchement restitué. v0.1 avalait le
            // relâchement de Retour arrière en état Armed, laissant l'OS
            // croire la touche enfoncée.
            Action::Swallow if matches!(id, Some((id, false)) if self.forwarded.contains(&id)) => {
                let (id, down) = id.unwrap();
                d.orders.push(Order::Forward { id, down });
            }
            Action::Swallow => {}
            Action::PassThrough => {
                if let Some((id, down)) = id {
                    d.orders.push(Order::Forward { id, down });
                }
            }
            Action::CommitAndSwallow(units) => d.orders.push(Order::Write(units)),
            Action::CommitAndPassThrough(units) => {
                d.orders.push(Order::Write(units));
                if let Some((id, down)) = id {
                    d.orders.push(Order::Forward { id, down });
                }
            }
        }

        for o in &d.orders {
            if let Order::Forward { id, down } = *o {
                if down {
                    self.forwarded.insert(id);
                } else {
                    self.forwarded.remove(&id);
                }
            }
        }
        d.hud = self.machine.drain_hud();
        self.sync_arm(&mut d);
        d
    }

    /// Clavier retiré ou trou de séquence : retour au repos sans écrire, et
    /// relâchement restitué pour toute position encore enfoncée.
    pub fn reset(&mut self) -> Decision {
        let mut d = Decision::default();
        self.machine.reset();
        self.down.clear();
        let mut ids: Vec<KeyId> = self.forwarded.drain().collect();
        ids.sort();
        d.orders.extend(ids.into_iter().map(|id| Order::Forward { id, down: false }));
        d.hud = self.machine.drain_hud();
        self.sync_arm(&mut d);
        d
    }

    fn sync_arm(&mut self, d: &mut Decision) {
        let wanted = self.machine.wants_armed();
        if wanted != self.keyboard_armed {
            self.keyboard_armed = wanted;
            d.to_keyboard.push(HostFrame::SetArm(wanted));
        }
    }
}
