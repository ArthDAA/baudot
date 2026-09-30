//! Identité des touches.
//!
//! Deux notions distinctes :
//! - [`KeyId`] : une POSITION physique sur le K3 Max ISO, telle que le clavier
//!   l'envoie dans une trame. Le clavier ne connaît aucune disposition.
//! - [`LogicalKey`] : la touche au sens de v0.1, c'est-à-dire ce que la
//!   disposition active de l'OS fait de cette position. v0.1 recevait
//!   directement ce code de Windows (code VK) ; on garde exactement les mêmes
//!   valeurs pour que les `.conf` et la machine à états se comportent à
//!   l'identique. Ce ne sont que des nombres : le cœur n'appelle jamais l'OS.
//!
//! La traduction position -> touche logique est faite par la plateforme
//! (voir `engine::KeyResolver`).

/// Position physique ISO (Baudot Key ID). 0 = aucune touche.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct KeyId(pub u8);

impl KeyId {
    // Rangée des chiffres
    pub const GRAVE: KeyId = KeyId(1);
    pub const N1: KeyId = KeyId(2);
    pub const N2: KeyId = KeyId(3);
    pub const N3: KeyId = KeyId(4);
    pub const N4: KeyId = KeyId(5);
    pub const N5: KeyId = KeyId(6);
    pub const N6: KeyId = KeyId(7);
    pub const N7: KeyId = KeyId(8);
    pub const N8: KeyId = KeyId(9);
    pub const N9: KeyId = KeyId(10);
    pub const N0: KeyId = KeyId(11);
    pub const MINUS: KeyId = KeyId(12);
    pub const EQUAL: KeyId = KeyId(13);
    // Rangée du haut
    pub const Q: KeyId = KeyId(14);
    pub const W: KeyId = KeyId(15);
    pub const E: KeyId = KeyId(16);
    pub const R: KeyId = KeyId(17);
    pub const T: KeyId = KeyId(18);
    pub const Y: KeyId = KeyId(19);
    pub const U: KeyId = KeyId(20);
    pub const I: KeyId = KeyId(21);
    pub const O: KeyId = KeyId(22);
    pub const P: KeyId = KeyId(23);
    pub const LBRACKET: KeyId = KeyId(24);
    pub const RBRACKET: KeyId = KeyId(25);
    // Rangée du milieu
    pub const A: KeyId = KeyId(26);
    pub const S: KeyId = KeyId(27);
    pub const D: KeyId = KeyId(28);
    pub const F: KeyId = KeyId(29);
    pub const G: KeyId = KeyId(30);
    pub const H: KeyId = KeyId(31);
    pub const J: KeyId = KeyId(32);
    pub const K: KeyId = KeyId(33);
    pub const L: KeyId = KeyId(34);
    pub const SEMICOLON: KeyId = KeyId(35);
    pub const QUOTE: KeyId = KeyId(36);
    pub const NONUS_HASH: KeyId = KeyId(37);
    // Rangée du bas
    pub const NONUS_BACKSLASH: KeyId = KeyId(38);
    pub const Z: KeyId = KeyId(39);
    pub const X: KeyId = KeyId(40);
    pub const C: KeyId = KeyId(41);
    pub const V: KeyId = KeyId(42);
    pub const B: KeyId = KeyId(43);
    pub const N: KeyId = KeyId(44);
    pub const M: KeyId = KeyId(45);
    pub const COMMA: KeyId = KeyId(46);
    pub const DOT: KeyId = KeyId(47);
    pub const SLASH: KeyId = KeyId(48);
    // Espace, édition, navigation
    pub const SPACE: KeyId = KeyId(49);
    pub const BACKSPACE: KeyId = KeyId(50);
    pub const ENTER: KeyId = KeyId(51);
    pub const TAB: KeyId = KeyId(52);
    pub const ESCAPE: KeyId = KeyId(53);
    pub const DELETE: KeyId = KeyId(54);
    pub const PAGE_UP: KeyId = KeyId(55);
    pub const PAGE_DOWN: KeyId = KeyId(56);
    pub const HOME: KeyId = KeyId(57);
    pub const END: KeyId = KeyId(58);
    pub const UP: KeyId = KeyId(59);
    pub const DOWN: KeyId = KeyId(60);
    pub const LEFT: KeyId = KeyId(61);
    pub const RIGHT: KeyId = KeyId(62);
    // Modificateurs (hors Maj et Verr. Maj., qui ne sont jamais redirigés)
    pub const LCTRL: KeyId = KeyId(63);
    pub const RCTRL: KeyId = KeyId(64);
    pub const LGUI: KeyId = KeyId(65);
    pub const LALT: KeyId = KeyId(66);
    // F1 = 67 ... F12 = 78
    pub const F1: KeyId = KeyId(67);
    pub const F12: KeyId = KeyId(78);

    /// Plus grand identifiant valide.
    pub const MAX: u8 = 78;

    pub fn is_valid(self) -> bool {
        (1..=Self::MAX).contains(&self.0)
    }

    /// Positions que le clavier redirige pendant l'armement : le bloc
    /// alphanumérique (1-48, où la disposition de l'OS peut placer une lettre
    /// ou un chiffre) et Retour arrière. Doit rester identique à
    /// `baudot_key_armable()` dans le firmware.
    pub fn is_armable(self) -> bool {
        (1..=48).contains(&self.0) || self == Self::BACKSPACE
    }

    /// Scancode PC (jeu 1) de la position, et s'il est étendu (préfixe E0).
    /// Donnée de plateforme pour les adaptateurs PC : résolution de la
    /// disposition et restitution des touches. Aucune trame ne le transporte.
    pub fn pc_scancode(self) -> Option<(u16, bool)> {
        let sc = match self.0 {
            1 => (0x29, false),
            2..=11 => (0x02 + (self.0 as u16 - 2), false), // 1 ... 0
            12 => (0x0C, false),
            13 => (0x0D, false),
            14..=23 => (0x10 + (self.0 as u16 - 14), false), // Q ... P
            24 => (0x1A, false),
            25 => (0x1B, false),
            26..=34 => (0x1E + (self.0 as u16 - 26), false), // A ... L
            35 => (0x27, false),
            36 => (0x28, false),
            37 => (0x2B, false),
            38 => (0x56, false),
            39..=45 => (0x2C + (self.0 as u16 - 39), false), // Z ... M
            46 => (0x33, false),
            47 => (0x34, false),
            48 => (0x35, false),
            49 => (0x39, false),
            50 => (0x0E, false),
            51 => (0x1C, false),
            52 => (0x0F, false),
            53 => (0x01, false),
            54 => (0x53, true),
            55 => (0x49, true),
            56 => (0x51, true),
            57 => (0x47, true),
            58 => (0x4F, true),
            59 => (0x48, true),
            60 => (0x50, true),
            61 => (0x4B, true),
            62 => (0x4D, true),
            63 => (0x1D, false),
            64 => (0x1D, true),
            65 => (0x5B, true),
            66 => (0x38, false),
            67..=76 => (0x3B + (self.0 as u16 - 67), false), // F1 ... F10
            77 => (0x57, false),
            78 => (0x58, false),
            _ => return None,
        };
        Some(sc)
    }
}

/// Touche logique au sens de v0.1 : mêmes valeurs que les codes VK Windows
/// que le moteur v0.1 recevait de son hook.
pub type LogicalKey = u32;

/// Constantes nommées des touches logiques utilisées par la machine à états.
pub mod logical {
    use super::LogicalKey;

    pub const BACKSPACE: LogicalKey = 0x08;
    pub const SHIFT: LogicalKey = 0x10;
    pub const CAPS_LOCK: LogicalKey = 0x14;
    pub const SPACE: LogicalKey = 0x20;
    pub const LEFT: LogicalKey = 0x25;
    pub const UP: LogicalKey = 0x26;
    pub const RIGHT: LogicalKey = 0x27;
    pub const DOWN: LogicalKey = 0x28;
    pub const LSHIFT: LogicalKey = 0xA0;
    pub const RSHIFT: LogicalKey = 0xA1;
    /// Alt droite en v0.1, c'est-à-dire Var en v0.2. Aucune position de la
    /// table ne se résout vers cette valeur : Var a sa propre trame.
    pub const VAR: LogicalKey = 0xA5;
    pub const OEM_1: LogicalKey = 0xBA;
    pub const OEM_PLUS: LogicalKey = 0xBB;
    pub const OEM_COMMA: LogicalKey = 0xBC;
    pub const OEM_MINUS: LogicalKey = 0xBD;
    pub const OEM_PERIOD: LogicalKey = 0xBE;
    pub const OEM_2: LogicalKey = 0xBF;
    pub const OEM_3: LogicalKey = 0xC0;
    pub const OEM_4: LogicalKey = 0xDB;
    pub const OEM_5: LogicalKey = 0xDC;
    pub const OEM_6: LogicalKey = 0xDD;
    pub const OEM_7: LogicalKey = 0xDE;
}

/// Nom de touche d'un `.conf` -> touche logique. Transposition exacte de
/// `key_char_to_vk()` de v0.1 : la ponctuation OEM et les flèches ont leur
/// propre correspondance, tout le reste suit la convention "majuscule ASCII =
/// code de touche" (valable pour A-Z et 0-9).
pub fn conf_key_to_logical(c: char) -> LogicalKey {
    use logical::*;
    match c {
        ';' => OEM_1,
        '=' => OEM_PLUS,
        ',' => OEM_COMMA,
        '-' => OEM_MINUS,
        '.' => OEM_PERIOD,
        '/' => OEM_2,
        '`' => OEM_3,
        '[' => OEM_4,
        '\\' => OEM_5,
        ']' => OEM_6,
        '\'' => OEM_7,
        '\u{2191}' => UP,
        '\u{2193}' => DOWN,
        '\u{2190}' => LEFT,
        '\u{2192}' => RIGHT,
        _ => c.to_ascii_uppercase() as u32,
    }
}
