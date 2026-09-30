//! Machine à états (D2 B4).
//!
//! Transposition volontairement ligne à ligne de `StateMachine::process_key`
//! de v0.1 (`main.rs` à la racine) : même états, mêmes transitions, mêmes
//! messages vers le HUD. Seules différences : la touche arrive comme touche
//! logique ([`LogicalKey`]) au lieu d'un code VK lu par un hook, et les
//! messages sont accumulés dans [`StateMachine::drain_hud`] au lieu d'être
//! écrits sur stdout. Toute modification de comportement ici doit d'abord
//! passer par D2 : les tests `tests/v01_equivalence.rs` rejouent v0.1 à côté.

use std::collections::HashMap;

use crate::key::{logical, LogicalKey};

/// Une variante = une valeur scalaire Unicode, en 1 ou 2 unités UTF-16.
/// `units[1] == 0` signifie "pas de second substitut".
pub type CharUnits = [u16; 2];

/// Variantes par touche logique, en minuscules (`min`) et majuscules (`maj`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub min_map: HashMap<LogicalKey, Vec<CharUnits>>,
    pub maj_map: HashMap<LogicalKey, Vec<CharUnits>>,
}

/// Messages vers le HUD : un pour un avec les lignes IPC de v0.1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudMsg {
    Alt(bool),
    Held,
    Show(LogicalKey),
    Cycle(usize),
    Hold(LogicalKey, usize),
    Pop,
    Hide,
    LangShow,
    LangCycle(usize),
    LangCommit(usize),
    LangCancel,
}

impl HudMsg {
    /// Ligne IPC exacte de v0.1 (`ALT:1`, `SHOW:69`...), pour le pont vers
    /// le HUD et les tests d'équivalence.
    pub fn ipc_line(&self) -> String {
        match self {
            HudMsg::Alt(down) => format!("ALT:{}", if *down { 1 } else { 0 }),
            HudMsg::Held => "HELD".into(),
            HudMsg::Show(vk) => format!("SHOW:{vk}"),
            HudMsg::Cycle(i) => format!("CYCLE:{i}"),
            HudMsg::Hold(vk, i) => format!("HOLD:{vk}:{i}"),
            HudMsg::Pop => "POP".into(),
            HudMsg::Hide => "HIDE".into(),
            HudMsg::LangShow => "LANGSHOW".into(),
            HudMsg::LangCycle(i) => format!("LANGCYCLE:{i}"),
            HudMsg::LangCommit(i) => format!("LANGCOMMIT:{i}"),
            HudMsg::LangCancel => "LANGCANCEL".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    ModifierHeld,
    CyclingVariants { active_vk: LogicalKey, index: usize, is_shifted: bool },
    // Dead Keys : une ou plusieurs diacritiques empilées (held_marks)
    // attendent la touche de base. Var n'a pas besoin de rester enfoncée.
    Armed,
    // Var + Espace : sélecteur de langue. `index` est un compteur brut, la
    // liste des langues (et donc le modulo) vit côté frontend.
    CyclingLanguage { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    PassThrough,
    Swallow,
    CommitAndSwallow(Vec<u16>),
    CommitAndPassThrough(Vec<u16>),
}

fn is_case_modifier_key(vk: LogicalKey) -> bool {
    vk == logical::SHIFT || vk == logical::LSHIFT || vk == logical::RSHIFT || vk == logical::CAPS_LOCK
}

/// Une variante est une diacritique "Dead Key" si son unique unité tombe
/// dans un des blocs Unicode de marques combinantes.
pub fn is_combining_mark(units: CharUnits) -> bool {
    units[1] == 0
        && matches!(units[0],
            0x0300..=0x036F   // Combining Diacritical Marks
            | 0x1AB0..=0x1AFF // Combining Diacritical Marks Extended
            | 0x1DC0..=0x1DFF // Combining Diacritical Marks Supplement
            | 0x20D0..=0x20FF // Combining Diacritical Marks for Symbols
            | 0xFE20..=0xFE2F // Combining Half Marks
        )
}

// Touche de base en mode Armed : uniquement A-Z/0-9.
fn plain_char_units(vk: LogicalKey, is_shifted: bool) -> Option<Vec<u16>> {
    let c = match vk {
        0x41..=0x5A => {
            let upper = char::from_u32(vk)?;
            if is_shifted { upper } else { upper.to_ascii_lowercase() }
        }
        0x30..=0x39 => char::from_u32(vk)?,
        _ => return None,
    };
    let mut b = [0u16; 2];
    Some(c.encode_utf16(&mut b).to_vec())
}

pub struct StateMachine {
    state: State,
    config: Layout,
    held_marks: Vec<CharUnits>,
    hud: Vec<HudMsg>,
}

impl StateMachine {
    pub fn new(config: Layout) -> Self {
        Self { state: State::Idle, config, held_marks: Vec::new(), hud: Vec::new() }
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// Remplace le profil sans toucher à l'état en cours (rechargement à chaud).
    pub fn set_layout(&mut self, config: Layout) {
        self.config = config;
    }

    /// Retour forcé au repos (trou de séquence, clavier retiré), sans rien
    /// écrire. Le HUD est refermé s'il était ouvert.
    pub fn reset(&mut self) {
        if self.state != State::Idle || !self.held_marks.is_empty() {
            self.emit(HudMsg::Hide);
        }
        self.state = State::Idle;
        self.held_marks.clear();
    }

    /// Messages HUD produits depuis le dernier appel.
    pub fn drain_hud(&mut self) -> Vec<HudMsg> {
        std::mem::take(&mut self.hud)
    }

    /// Faut-il que le clavier reste armé (ou s'arme au relâchement de Var) ?
    /// Vrai exactement quand v0.1 serait (ou passerait, au relâchement
    /// d'AltGr) en état Armed.
    pub fn wants_armed(&self) -> bool {
        match self.state {
            State::Idle | State::CyclingLanguage { .. } => false,
            State::Armed => true,
            State::ModifierHeld => !self.held_marks.is_empty(),
            State::CyclingVariants { active_vk, index, is_shifted } => {
                let map = if is_shifted { &self.config.maj_map } else { &self.config.min_map };
                map.get(&active_vk)
                    .filter(|v| !v.is_empty())
                    .map(|v| is_combining_mark(v[index % v.len()]))
                    .unwrap_or(false)
            }
        }
    }

    fn emit(&mut self, msg: HudMsg) {
        self.hud.push(msg);
    }

    fn take_commit_units(&mut self, units: CharUnits) -> Vec<u16> {
        let mut out = Vec::with_capacity(2 + self.held_marks.len());
        out.push(units[0]);
        if units[1] != 0 {
            out.push(units[1]);
        }
        for m in self.held_marks.drain(..) {
            out.push(m[0]); // une marque combinante tient toujours en 1 unité
        }
        out
    }

    pub fn process_key(&mut self, vk_code: LogicalKey, is_down: bool, is_shifted: bool, is_repeat: bool) -> Action {
        let is_alt = vk_code == logical::VAR;

        // Signal brut de l'état physique de Var, indépendant de self.state.
        if is_alt && !is_repeat {
            self.emit(HudMsg::Alt(is_down));
        }

        match self.state {
            State::Idle => {
                if is_alt {
                    if is_down {
                        self.emit(HudMsg::Held);
                        self.state = State::ModifierHeld;
                    }
                    return Action::Swallow;
                }
                Action::PassThrough
            }
            State::ModifierHeld => {
                if is_alt {
                    if !is_down {
                        if self.held_marks.is_empty() {
                            self.emit(HudMsg::Hide);
                            self.state = State::Idle;
                        } else {
                            // Rien sélectionné à ce relâchement-ci : les
                            // diacritiques déjà empilées restent en attente.
                            self.state = State::Armed;
                        }
                    }
                    return Action::Swallow;
                }

                if is_down {
                    if vk_code == logical::SPACE {
                        if is_repeat {
                            return Action::Swallow;
                        }
                        self.held_marks.clear();
                        self.emit(HudMsg::LangShow);
                        self.state = State::CyclingLanguage { index: 0 };
                        return Action::Swallow;
                    }

                    let map = if is_shifted { &self.config.maj_map } else { &self.config.min_map };
                    if map.contains_key(&vk_code) {
                        self.emit(HudMsg::Show(vk_code));
                        self.state = State::CyclingVariants { active_vk: vk_code, index: 0, is_shifted };
                        return Action::Swallow;
                    }
                }
                Action::PassThrough
            }
            State::CyclingLanguage { index } => {
                if is_alt {
                    if !is_down {
                        self.emit(HudMsg::LangCommit(index));
                        self.state = State::Idle;
                    }
                    return Action::Swallow;
                }

                if is_down {
                    if is_case_modifier_key(vk_code) {
                        return Action::PassThrough;
                    }

                    if vk_code == logical::SPACE {
                        if !is_repeat {
                            let new_index = index + 1;
                            self.emit(HudMsg::LangCycle(new_index));
                            self.state = State::CyclingLanguage { index: new_index };
                        }
                        return Action::Swallow;
                    }

                    // Toute autre touche annule le sélecteur de langue.
                    self.emit(HudMsg::LangCancel);
                    self.state = State::ModifierHeld;
                }
                Action::PassThrough
            }
            State::CyclingVariants { active_vk, index, is_shifted: current_is_shifted } => {
                if is_alt {
                    if !is_down {
                        let map = if current_is_shifted { &self.config.maj_map } else { &self.config.min_map };
                        if let Some(variants) = map.get(&active_vk).filter(|v| !v.is_empty()) {
                            let idx = index % variants.len();
                            let units = variants[idx];
                            if is_combining_mark(units) {
                                self.held_marks.push(units);
                                self.emit(HudMsg::Hold(active_vk, idx));
                                self.state = State::Armed;
                                return Action::Swallow;
                            }
                            let commit = self.take_commit_units(units);
                            self.emit(HudMsg::Hide);
                            self.state = State::Idle;
                            return Action::CommitAndSwallow(commit);
                        }
                        self.emit(HudMsg::Hide);
                        self.state = State::Idle;
                    }
                    return Action::Swallow;
                }

                if is_down {
                    if is_case_modifier_key(vk_code) {
                        return Action::PassThrough;
                    }

                    if vk_code == active_vk && is_shifted == current_is_shifted {
                        if is_repeat {
                            return Action::Swallow;
                        }
                        let new_index = index + 1;
                        self.emit(HudMsg::Cycle(new_index));
                        self.state = State::CyclingVariants { active_vk, index: new_index, is_shifted: current_is_shifted };
                        return Action::Swallow;
                    }

                    if vk_code == active_vk && is_repeat {
                        return Action::Swallow;
                    }

                    let map_current = if current_is_shifted { &self.config.maj_map } else { &self.config.min_map };
                    let variants_current = match map_current.get(&active_vk).filter(|v| !v.is_empty()) {
                        Some(v) => v,
                        None => {
                            self.emit(HudMsg::Hide);
                            self.state = State::Idle;
                            return Action::PassThrough;
                        }
                    };
                    let idx_current = index % variants_current.len();
                    let units_current = variants_current[idx_current];

                    let map_new = if is_shifted { &self.config.maj_map } else { &self.config.min_map };
                    let starts_new_cycle = map_new.contains_key(&vk_code);

                    if is_combining_mark(units_current) {
                        self.held_marks.push(units_current);
                        self.emit(HudMsg::Hold(active_vk, idx_current));
                        if starts_new_cycle {
                            self.emit(HudMsg::Show(vk_code));
                            self.state = State::CyclingVariants { active_vk: vk_code, index: 0, is_shifted };
                            return Action::Swallow;
                        }
                        self.state = State::ModifierHeld;
                        return Action::PassThrough;
                    }

                    let commit = self.take_commit_units(units_current);
                    if starts_new_cycle {
                        self.emit(HudMsg::Show(vk_code));
                        self.state = State::CyclingVariants { active_vk: vk_code, index: 0, is_shifted };
                        return Action::CommitAndSwallow(commit);
                    } else {
                        self.emit(HudMsg::Hide);
                        self.state = State::ModifierHeld;
                        return Action::CommitAndPassThrough(commit);
                    }
                }

                if !is_down && vk_code == active_vk {
                    return Action::Swallow;
                }
                Action::PassThrough
            }
            State::Armed => {
                if is_alt {
                    // Pas de HELD ici : l'affichage des diacritiques empilées
                    // reste tel quel jusqu'au prochain SHOW/HOLD/HIDE réel.
                    if is_down {
                        self.state = State::ModifierHeld;
                    }
                    return Action::Swallow;
                }

                // Retour arrière dépile la dernière diacritique, toujours avalé.
                if vk_code == logical::BACKSPACE {
                    if is_down && !is_repeat {
                        self.held_marks.pop();
                        if self.held_marks.is_empty() {
                            self.emit(HudMsg::Hide);
                            self.state = State::Idle;
                        } else {
                            self.emit(HudMsg::Pop);
                        }
                    }
                    return Action::Swallow;
                }

                if is_down && !is_repeat {
                    if let Some(base_units) = plain_char_units(vk_code, is_shifted) {
                        let mut units = base_units;
                        for m in &self.held_marks {
                            units.push(m[0]);
                        }
                        self.held_marks.clear();
                        self.emit(HudMsg::Hide);
                        self.state = State::Idle;
                        return Action::CommitAndSwallow(units);
                    }
                }
                Action::PassThrough
            }
        }
    }
}
