// Banc d'essai partagé : oracle v0.1, modèle du firmware, dispositions de
// test et les deux chaînes complètes (v0.1 et v0.2) à comparer.
#![allow(dead_code)]

pub mod firmware;
pub mod v01_oracle;

use std::collections::HashSet;
use std::path::PathBuf;

use omnikey_core::protocol::DeviceFrame;
use omnikey_core::{conf, Engine, KeyId, KeyResolver, LogicalKey, Order};

use firmware::{Firmware, FwOut, Phys};

pub fn binaries_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../omnikey-hud/src-tauri/binaries")
}

// ---------------------------------------------------------------------------
// Dispositions : position -> code de touche v0.1 (ce que Windows donnerait)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum OsLayout {
    /// Windows "US" (kbdus), la disposition du poste d'Arthus.
    Us,
    /// Windows "Français" (kbdfr).
    Fr,
}

const LETTER_ROWS: [(&[u8], u8); 3] = [(b"QWERTYUIOP", 14), (b"ASDFGHJKL", 26), (b"ZXCVBNM", 39)];

impl KeyResolver for OsLayout {
    fn resolve(&self, id: KeyId) -> LogicalKey {
        let fr = matches!(self, OsLayout::Fr);
        // Lettres : position QWERTY, permutées en AZERTY.
        for (row, base) in LETTER_ROWS {
            if (base..base + row.len() as u8).contains(&id.0) {
                let c = row[(id.0 - base) as usize];
                let c = if fr {
                    match c {
                        b'Q' => b'A',
                        b'A' => b'Q',
                        b'W' => b'Z',
                        b'Z' => b'W',
                        b'M' => return 0xBC, // VK_OEM_COMMA
                        other => other,
                    }
                } else {
                    c
                };
                return c as u32;
            }
        }
        match id.0 {
            2..=10 => (b'1' + id.0 - 2) as u32,
            11 => b'0' as u32,
            _ => match (id, fr) {
                (KeyId::GRAVE, false) => 0xC0,
                (KeyId::GRAVE, true) => 0xDE,
                (KeyId::MINUS, false) => 0xBD,
                (KeyId::MINUS, true) => 0xDB,
                (KeyId::EQUAL, _) => 0xBB,
                (KeyId::LBRACKET, false) => 0xDB,
                (KeyId::LBRACKET, true) => 0xDD,
                (KeyId::RBRACKET, false) => 0xDD,
                (KeyId::RBRACKET, true) => 0xBA,
                (KeyId::SEMICOLON, false) => 0xBA,
                (KeyId::SEMICOLON, true) => b'M' as u32,
                (KeyId::QUOTE, false) => 0xDE,
                (KeyId::QUOTE, true) => 0xC0,
                (KeyId::NONUS_HASH, _) => 0xDC,
                (KeyId::NONUS_BACKSLASH, _) => 0xE2,
                (KeyId::COMMA, false) => 0xBC,
                (KeyId::COMMA, true) => 0xBE,
                (KeyId::DOT, false) => 0xBE,
                (KeyId::DOT, true) => 0xBF,
                (KeyId::SLASH, false) => 0xBF,
                (KeyId::SLASH, true) => 0xDF,
                (KeyId::SPACE, _) => 0x20,
                (KeyId::BACKSPACE, _) => 0x08,
                (KeyId::ENTER, _) => 0x0D,
                (KeyId::TAB, _) => 0x09,
                (KeyId::ESCAPE, _) => 0x1B,
                (KeyId::DELETE, _) => 0x2E,
                (KeyId::PAGE_UP, _) => 0x21,
                (KeyId::PAGE_DOWN, _) => 0x22,
                (KeyId::HOME, _) => 0x24,
                (KeyId::END, _) => 0x23,
                (KeyId::UP, _) => 0x26,
                (KeyId::DOWN, _) => 0x28,
                (KeyId::LEFT, _) => 0x25,
                (KeyId::RIGHT, _) => 0x27,
                (KeyId::LCTRL, _) => 0xA2,
                (KeyId::RCTRL, _) => 0xA3,
                (KeyId::LGUI, _) => 0x5B,
                (KeyId::LALT, _) => 0xA4,
                (KeyId(n), _) if (67..=78).contains(&n) => 0x70 + (n as u32 - 67),
                _ => 0,
            },
        }
    }
}

pub fn phys_vk(layout: OsLayout, p: Phys) -> u32 {
    match p {
        Phys::Var => 0xA5,
        Phys::Shift => 0xA0,
        Phys::Caps => 0x14,
        Phys::Pos(id) => layout.resolve(id),
    }
}

// ---------------------------------------------------------------------------
// Ce que l'application au premier plan reçoit
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OsItem {
    Text(Vec<u16>),
    Key(u32, bool),
}

/// Retire ce qui n'a aucun effet pour l'application : relâchement d'une
/// touche qui n'était pas enfoncée, appui sur une touche déjà enfoncée.
pub fn normalize(items: &[OsItem]) -> Vec<OsItem> {
    let mut down: HashSet<u32> = HashSet::new();
    let mut out = Vec::new();
    for it in items {
        match it {
            OsItem::Text(_) => out.push(it.clone()),
            OsItem::Key(vk, true) => {
                if down.insert(*vk) {
                    out.push(it.clone());
                }
            }
            OsItem::Key(vk, false) => {
                if down.remove(vk) {
                    out.push(it.clone());
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Chaîne v0.1 : hook -> StateMachine d'origine
// ---------------------------------------------------------------------------

pub struct ChainV01 {
    sm: v01_oracle::StateMachine,
    layout: OsLayout,
    keys_down: HashSet<u32>,
    pub hud: Vec<String>,
    pub os: Vec<OsItem>,
    /// Occurrences du défaut v0.1 "Retour arrière bloqué" (voir feed).
    pub v01_stuck_backspace: usize,
}

impl ChainV01 {
    pub fn new(conf_path: &str, layout: OsLayout) -> Self {
        let cfg = v01_oracle::LayoutConfig::load_from_file(conf_path).expect("conf v0.1");
        v01_oracle::IPC.with(|v| v.borrow_mut().clear());
        Self { sm: v01_oracle::StateMachine::new(cfg), layout, keys_down: HashSet::new(), hud: Vec::new(), os: Vec::new(), v01_stuck_backspace: 0 }
    }

    fn os_down(&self, vk: u32) -> bool {
        normalize(&self.os).iter().fold(false, |acc, it| match it {
            OsItem::Key(v, d) if *v == vk => *d,
            _ => acc,
        })
    }

    /// `shift` / `caps` : état physique AVANT cet événement (le hook bas
    /// niveau voit l'état antérieur).
    pub fn feed(&mut self, p: Phys, down: bool, shift: bool, caps: bool) {
        let vk = phys_vk(self.layout, p);
        let is_repeat = if down { !self.keys_down.insert(vk) } else {
            self.keys_down.remove(&vk);
            false
        };
        let action = self.sm.process_key(vk, down, shift ^ caps, is_repeat);
        v01_oracle::IPC.with(|v| self.hud.extend(v.borrow_mut().drain(..)));
        use v01_oracle::Action;
        match action {
            // Écart assumé (journal, étape 1) : en état Armed, v0.1 avale le
            // relâchement de Retour arrière même quand l'appui était déjà
            // parti vers l'OS, qui garde alors la touche "enfoncée". En v0.2,
            // ce relâchement passe directement (appui non redirigé) ou est
            // restitué par l'Engine (appui restitué). On le restitue ici aussi
            // pour comparer le reste.
            Action::Swallow if vk == 0x08 && !down && self.os_down(0x08) => {
                self.v01_stuck_backspace += 1;
                self.os.push(OsItem::Key(vk, false));
            }
            Action::Swallow => {}
            Action::PassThrough => self.os.push(OsItem::Key(vk, down)),
            Action::CommitAndSwallow(u) => self.os.push(OsItem::Text(u)),
            Action::CommitAndPassThrough(u) => {
                self.os.push(OsItem::Text(u));
                self.os.push(OsItem::Key(vk, down));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Chaîne v0.2 : firmware -> trames -> Engine -> ordres
// ---------------------------------------------------------------------------

pub struct ChainV02 {
    pub fw: Firmware,
    pub engine: Engine,
    layout: OsLayout,
    pub hud: Vec<String>,
    pub os: Vec<OsItem>,
    /// Trames clavier -> software émises (pour vérifier M3).
    pub frames: Vec<DeviceFrame>,
}

impl ChainV02 {
    pub fn new(conf_path: &str, layout: OsLayout, host_alive: bool) -> Self {
        let l = conf::load_layout_file(std::path::Path::new(conf_path)).expect("conf v0.2");
        Self { fw: Firmware::new(host_alive), engine: Engine::new(l), layout, hud: Vec::new(), os: Vec::new(), frames: Vec::new() }
    }

    pub fn feed(&mut self, p: Phys, down: bool, shift: bool, caps: bool) {
        match self.fw.key(p, down, shift, caps) {
            FwOut::Pass => self.os.push(OsItem::Key(phys_vk(self.layout, p), down)),
            FwOut::Nothing => {}
            FwOut::Frame(bytes) => {
                let frame = DeviceFrame::decode(&bytes).expect("trame valide");
                self.frames.push(frame);
                let DeviceFrame::Event { event, .. } = frame else { return };
                let d = self.engine.handle(event, &self.layout);
                self.hud.extend(d.hud.iter().map(|m| m.ipc_line()));
                for o in d.orders {
                    match o {
                        Order::Write(u) => self.os.push(OsItem::Text(u)),
                        Order::Forward { id, down } => self.os.push(OsItem::Key(self.layout.resolve(id), down)),
                    }
                }
                for f in d.to_keyboard {
                    self.fw.host(&f.encode(0));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Clavier physique : suit Maj / Verr. Maj. et pilote les deux chaînes
// ---------------------------------------------------------------------------

pub struct Bench {
    pub v01: ChainV01,
    pub v02: ChainV02,
    pub down: HashSet<Phys>,
    shift: bool,
    caps: bool,
}

impl Bench {
    pub fn new(conf: &str, layout: OsLayout) -> Self {
        let path = binaries_dir().join(conf);
        let path = path.to_str().unwrap();
        Self { v01: ChainV01::new(path, layout), v02: ChainV02::new(path, layout, true), down: HashSet::new(), shift: false, caps: false }
    }

    pub fn press(&mut self, p: Phys) {
        assert!(self.down.insert(p), "{p:?} déjà enfoncée");
        self.v01.feed(p, true, self.shift, self.caps);
        self.v02.feed(p, true, self.shift, self.caps);
        match p {
            Phys::Shift => self.shift = true,
            Phys::Caps => self.caps = !self.caps,
            _ => {}
        }
    }

    pub fn release(&mut self, p: Phys) {
        assert!(self.down.remove(&p), "{p:?} pas enfoncée");
        self.v01.feed(p, false, self.shift, self.caps);
        self.v02.feed(p, false, self.shift, self.caps);
        if p == Phys::Shift {
            self.shift = false;
        }
    }

    pub fn tap(&mut self, p: Phys) {
        self.press(p);
        self.release(p);
    }

    pub fn release_all(&mut self) {
        let mut rest: Vec<Phys> = self.down.iter().copied().collect();
        rest.sort_by_key(|p| format!("{p:?}"));
        for p in rest {
            self.release(p);
        }
    }

    /// Texte final vu par l'application, touches comprises, normalisé.
    pub fn assert_equivalent(&self, context: &str) {
        if let Some(i) = first_diff(&self.v01.hud, &self.v02.hud) {
            panic!("HUD divergent à l'index {i}\nv0.1: {:?}\nv0.2: {:?}\n{context}", &self.v01.hud[i.saturating_sub(3)..], &self.v02.hud[i.saturating_sub(3)..]);
        }
        let (a, b) = (normalize(&self.v01.os), normalize(&self.v02.os));
        if let Some(i) = first_diff(&a, &b) {
            let lo = i.saturating_sub(3);
            panic!("Sortie OS divergente à l'index {i}\nv0.1: {:?}\nv0.2: {:?}\n{context}", &a[lo..(i + 3).min(a.len())], &b[lo..(i + 3).min(b.len())]);
        }
    }
}

fn first_diff<T: PartialEq>(a: &[T], b: &[T]) -> Option<usize> {
    let n = a.len().min(b.len());
    (0..n).find(|&i| a[i] != b[i]).or(if a.len() != b.len() { Some(n) } else { None })
}

/// Texte UTF-16 -> String, pour lire les résultats dans les tests nommés.
pub fn texts(items: &[OsItem]) -> String {
    items
        .iter()
        .filter_map(|i| match i {
            OsItem::Text(u) => Some(String::from_utf16_lossy(u)),
            _ => None,
        })
        .collect()
}
