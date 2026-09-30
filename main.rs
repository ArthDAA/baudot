use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::process::exit;
use std::env;
use std::io::Write; // Requis pour le flush()
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VK_RMENU, VK_SPACE, VK_BACK, VIRTUAL_KEY, GetAsyncKeyState, GetKeyState, VK_SHIFT, VK_LSHIFT,
    VK_RSHIFT, VK_CAPITAL, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7,
    VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_UP, VK_DOWN, VK_LEFT, VK_RIGHT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

const INJECTED_MAGIC_VALUE: usize = 0xDEADBEEF;

// Canal vers le thread dédié à l'écriture stdout — le hook WH_KEYBOARD_LL ne
// doit jamais faire d'I/O bloquante (thread lancé dans main()).
static IPC_TX: std::sync::OnceLock<std::sync::mpsc::Sender<String>> = std::sync::OnceLock::new();

// Macro pour envoyer un message IPC : dépose seulement dans le canal, ne
// bloque jamais (le hook doit rester sous LowLevelHooksTimeout).
macro_rules! ipc_send {
    ($($arg:tt)*) => {
        if let Some(tx) = IPC_TX.get() {
            let _ = tx.send(format!($($arg)*));
        }
    };
}

// Une variante = un seul `char` Rust (une valeur scalaire Unicode), qui tient
// en 1 unité UTF-16 (Plan Multilingue de Base) ou 2 (paire de substituts,
// ex. la plupart des sinogrammes rares/CJK Ext.). Jamais plus de 2 : on ne
// prend que `chars().next()` de chaque variante, donc pas de cluster à gérer.
// `units[1] == 0` signifie "pas de second substitut" (0 n'est l'encodage
// d'aucun caractère injectable ici).
pub type CharUnits = [u16; 2];

// Table de correspondance caractère de clé -> VK Windows, pour les touches
// qui ne suivent pas la convention ASCII-majuscule=VK (valable uniquement
// pour A-Z/0-9) : la ponctuation OEM (VK_OEM_* ne coïncident pas avec leur
// code ASCII, ex. ';' vaut 0x3B mais VK_OEM_1 vaut 0xBA) et les flèches
// directionnelles (identifiées dans le .conf par leur propre glyphe, faute
// de caractère "normal" à taper dessus). Tout le reste retombe sur le hack
// ASCII existant.
fn key_char_to_vk(c: char) -> u32 {
    match c {
        ';' => VK_OEM_1.0 as u32,
        '=' => VK_OEM_PLUS.0 as u32,
        ',' => VK_OEM_COMMA.0 as u32,
        '-' => VK_OEM_MINUS.0 as u32,
        '.' => VK_OEM_PERIOD.0 as u32,
        '/' => VK_OEM_2.0 as u32,
        '`' => VK_OEM_3.0 as u32,
        '[' => VK_OEM_4.0 as u32,
        '\\' => VK_OEM_5.0 as u32,
        ']' => VK_OEM_6.0 as u32,
        '\'' => VK_OEM_7.0 as u32,
        '\u{2191}' => VK_UP.0 as u32,    // ↑
        '\u{2193}' => VK_DOWN.0 as u32,  // ↓
        '\u{2190}' => VK_LEFT.0 as u32,  // ←
        '\u{2192}' => VK_RIGHT.0 as u32, // →
        _ => c.to_ascii_uppercase() as u32,
    }
}

#[derive(Debug, Clone)]
pub struct LayoutConfig {
    pub min_map: HashMap<u32, Vec<CharUnits>>,
    pub maj_map: HashMap<u32, Vec<CharUnits>>,
}

impl LayoutConfig {
    pub fn load_from_file(path: &str) -> Result<Self, u8> {
        let mut config = Self { min_map: HashMap::new(), maj_map: HashMap::new() };

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Err(1),
        };

        let mut current_section = "";
        // Un profil sans aucune variante (juste "lang:" + "min:") est légitime
        // (ex. english.conf : la frappe passe alors intégralement en
        // PassThrough). Seule l'absence de toute section reconnue signale un
        // fichier réellement malformé.
        let mut saw_section = false;
        for line in content.lines() {
            let line = line.trim_start_matches('\u{FEFF}').trim();
            if line.is_empty() { continue; }

            if line == "min:" { current_section = "min"; saw_section = true; continue; }
            if line == "maj:" { current_section = "maj"; saw_section = true; continue; }

            if let Some((key_str, val_str)) = line.split_once('=') {
                if let Some(key_char) = key_str.trim().chars().next() {
                    let vk_code = key_char_to_vk(key_char);
                    let mut variants = Vec::new();
                    for part in val_str.split(',') {
                        let mut chars = part.trim().chars();
                        // U+25CC (cercle pointillé) en tête d'une variante est
                        // un pur confort d'écriture pour les marques
                        // combinantes isolées (ex. "◌̄" au lieu de juste "̄",
                        // illisible/impossible à taper proprement dans un
                        // éditeur classique) : on le saute et on garde le
                        // vrai caractère qui suit.
                        let first = chars.next();
                        let c = match first {
                            Some('\u{25CC}') => chars.next().or(first),
                            other => other,
                        };
                        if let Some(c) = c {
                            let mut b = [0u16; 2];
                            let utf16 = c.encode_utf16(&mut b);
                            if !utf16.is_empty() {
                                let mut units: CharUnits = [0, 0];
                                units[0] = utf16[0];
                                if utf16.len() > 1 { units[1] = utf16[1]; }
                                variants.push(units);
                            }
                        }
                    }
                    // Une ligne du type "x =" ou "x = ," produit un Vec vide :
                    // ne pas l'insérer (sinon contains_key répond vrai mais
                    // tout modulo sur variants.len() panique plus tard).
                    if !variants.is_empty() {
                        if current_section == "min" { config.min_map.insert(vk_code, variants); }
                        else if current_section == "maj" { config.maj_map.insert(vk_code, variants); }
                    }
                }
            }
        }

        if !saw_section { return Err(2); }
        Ok(config)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    ModifierHeld,
    CyclingVariants { active_vk: u32, index: usize, is_shifted: bool },
    // Mode Dead Keys : une ou plusieurs diacritiques combinantes sont
    // empilées (StateMachine::held_marks) et attendent la touche de base.
    // Aucun modificateur n'a besoin de rester enfoncé ici — n'importe quelle
    // touche alphanumérique "normale" sert de base à la composition.
    Armed,
    // AltGr + Espace : sélecteur de langue. `index` est un compteur brut,
    // incrémenté à chaque nouvel appui sur Espace — le moteur ne connaît pas
    // la liste des langues (elle vit côté frontend, seul à lire binaries/) et
    // ne fait donc aucun modulo lui-même, contrairement à index dans
    // CyclingVariants. C'est le frontend qui referme la boucle (index %
    // langues.len()) à réception de LANGCYCLE.
    CyclingLanguage { index: usize },
}

// Le payload d'un commit n'est plus un seul caractère : en mode Dead Keys il
// porte la touche de base *et* toutes les diacritiques empilées à injecter
// ensemble (voir StateMachine::take_commit_units).
#[derive(Debug, Clone)]
pub enum Action { PassThrough, Swallow, CommitAndSwallow(Vec<u16>), CommitAndPassThrough(Vec<u16>) }

fn is_case_modifier_key(vk_code: u32) -> bool {
    vk_code == VK_SHIFT.0 as u32
        || vk_code == VK_LSHIFT.0 as u32
        || vk_code == VK_RSHIFT.0 as u32
        || vk_code == VK_CAPITAL.0 as u32
}

// Une variante est traitée comme une diacritique "Dead Key" (à empiler plutôt
// qu'à injecter seule) si son unique unité tombe dans un des blocs Unicode de
// marques combinantes — tous dans le Plan Multilingue de Base, donc units[1]
// est forcément 0 pour une vraie marque (sinon ce n'est pas une marque seule).
fn is_combining_mark(units: CharUnits) -> bool {
    units[1] == 0
        && matches!(units[0],
            0x0300..=0x036F   // Combining Diacritical Marks
            | 0x1AB0..=0x1AFF // Combining Diacritical Marks Extended
            | 0x1DC0..=0x1DFF // Combining Diacritical Marks Supplement
            | 0x20D0..=0x20FF // Combining Diacritical Marks for Symbols
            | 0xFE20..=0xFE2F // Combining Half Marks
        )
}

// Touche de base en mode Armed : uniquement A-Z/0-9 (même convention VK =
// ASCII majuscule que le reste du moteur) — tout le reste (flèches, etc.)
// n'est pas une base valide, on laisse l'état armé attendre.
fn plain_char_units(vk_code: u32, is_shifted: bool) -> Option<Vec<u16>> {
    let c = match vk_code {
        0x41..=0x5A => {
            let upper = char::from_u32(vk_code)?;
            if is_shifted { upper } else { upper.to_ascii_lowercase() }
        }
        0x30..=0x39 => char::from_u32(vk_code)?,
        _ => return None,
    };
    let mut b = [0u16; 2];
    Some(c.encode_utf16(&mut b).to_vec())
}

pub struct StateMachine { state: State, config: LayoutConfig, held_marks: Vec<CharUnits> }

impl StateMachine {
    pub fn new(config: LayoutConfig) -> Self { Self { state: State::Idle, config, held_marks: Vec::new() } }

    // Caractère normal à committer : s'il y a des diacritiques en attente
    // (posées via une lettre "de base" alors qu'un Dead Key était déjà
    // empilé, cf. State::Armed), elles sont ajoutées à la suite et vidées.
    fn take_commit_units(&mut self, units: CharUnits) -> Vec<u16> {
        let mut out = Vec::with_capacity(2 + self.held_marks.len());
        out.push(units[0]);
        if units[1] != 0 { out.push(units[1]); }
        for m in self.held_marks.drain(..) {
            out.push(m[0]); // une marque combinante tient toujours en 1 unité
        }
        out
    }

    pub fn process_key(&mut self, vk_code: u32, is_down: bool, is_shifted: bool, is_repeat: bool) -> Action {
        let is_alt = vk_code == VK_RMENU.0 as u32;

        // Signal brut, indépendant de self.state : contrairement à HELD/HIDE
        // (pilotés par la logique métier ci-dessous, volontairement silencieux
        // par ex. en State::Armed pour ne pas perturber l'affichage des
        // diacritiques empilées — voir le commentaire dans ce bloc), le HUD a
        // besoin de savoir si AltGr est PHYSIQUEMENT tenu à cet instant précis,
        // même quand ça n'entraîne aucune transition d'état ici (ex. relâché
        // pendant un Dead Key armé) — sert uniquement à masquer le rappel
        // diacritiques (pas le HUD lui-même) côté frontend.
        if is_alt && !is_repeat {
            ipc_send!("ALT:{}", if is_down { 1 } else { 0 });
        }

        match self.state {
            State::Idle => {
                if is_alt {
                    if is_down { 
                        ipc_send!("HELD");
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
                            ipc_send!("HIDE");
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
                    if vk_code == VK_SPACE.0 as u32 {
                        if is_repeat { return Action::Swallow; }
                        self.held_marks.clear();
                        ipc_send!("LANGSHOW");
                        self.state = State::CyclingLanguage { index: 0 };
                        return Action::Swallow;
                    }

                    let map = if is_shifted { &self.config.maj_map } else { &self.config.min_map };
                    if map.contains_key(&vk_code) {
                        ipc_send!("SHOW:{}", vk_code);
                        self.state = State::CyclingVariants { active_vk: vk_code, index: 0, is_shifted };
                        return Action::Swallow;
                    }
                }
                Action::PassThrough
            }
            State::CyclingLanguage { index } => {
                if is_alt {
                    if !is_down {
                        ipc_send!("LANGCOMMIT:{}", index);
                        self.state = State::Idle;
                    }
                    return Action::Swallow;
                }

                if is_down {
                    if is_case_modifier_key(vk_code) {
                        // Comme dans CyclingVariants : Maj/Verr.Maj. ne doit
                        // ni avancer le cycle ni l'annuler.
                        return Action::PassThrough;
                    }

                    if vk_code == VK_SPACE.0 as u32 {
                        if !is_repeat {
                            let new_index = index + 1;
                            ipc_send!("LANGCYCLE:{}", new_index);
                            self.state = State::CyclingLanguage { index: new_index };
                        }
                        return Action::Swallow;
                    }

                    // Toute autre touche annule le sélecteur de langue (rien
                    // n'est encore validé) et repasse en ModifierHeld normal :
                    // la frappe suit son cours, comme si AltGr seul était tenu.
                    ipc_send!("LANGCANCEL");
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
                                // Dead Key : on empile au lieu d'injecter, et on
                                // reste armé pour la prochaine touche (avec ou
                                // sans AltGr) au lieu de fermer le HUD.
                                self.held_marks.push(units);
                                ipc_send!("HOLD:{}:{}", active_vk, idx);
                                self.state = State::Armed;
                                return Action::Swallow;
                            }
                            let commit = self.take_commit_units(units);
                            ipc_send!("HIDE");
                            self.state = State::Idle;
                            return Action::CommitAndSwallow(commit);
                        }
                        ipc_send!("HIDE");
                        self.state = State::Idle;
                    }
                    return Action::Swallow;
                }

                if is_down {
                    if is_case_modifier_key(vk_code) {
                        // Maj/Verr.Maj. sont des modificateurs, pas des lettres :
                        // ils ne doivent ni valider le cycle en cours ni fermer le
                        // HUD. Le changement de casse n'est pris en compte qu'à la
                        // prochaine frappe de la lettre (cf. ci-dessous), pas ici.
                        return Action::PassThrough;
                    }

                    // Minuscule et majuscule sont deux lettres à part entière :
                    // ce n'est un "vrai" cycle sur la même lettre que si la casse
                    // n'a pas changé depuis l'ouverture. Sinon (Maj/Verr.Maj. a
                    // basculé entre-temps, même si la touche physique est la
                    // même), on traite l'appui comme une nouvelle lettre.
                    if vk_code == active_vk && is_shifted == current_is_shifted {
                        if is_repeat { return Action::Swallow; }
                        let new_index = index + 1;
                        ipc_send!("CYCLE:{}", new_index);
                        self.state = State::CyclingVariants { active_vk, index: new_index, is_shifted: current_is_shifted };
                        return Action::Swallow;
                    }

                    if vk_code == active_vk && is_repeat {
                        // Auto-répétition matérielle de la même touche pendant un
                        // flip de casse : on ignore plutôt que de spammer des
                        // commits, en attendant un vrai relâchement/ré-appui.
                        return Action::Swallow;
                    }

                    let map_current = if current_is_shifted { &self.config.maj_map } else { &self.config.min_map };
                    // Garde défensive : une entrée vide ne devrait plus jamais être
                    // insérée par le parseur, mais on referme proprement plutôt que
                    // de paniquer sur variants.len()==0 si c'était le cas.
                    let variants_current = match map_current.get(&active_vk).filter(|v| !v.is_empty()) {
                        Some(v) => v,
                        None => {
                            ipc_send!("HIDE");
                            self.state = State::Idle;
                            return Action::PassThrough;
                        }
                    };
                    let idx_current = index % variants_current.len();
                    let units_current = variants_current[idx_current];

                    let map_new = if is_shifted { &self.config.maj_map } else { &self.config.min_map };
                    let starts_new_cycle = map_new.contains_key(&vk_code);

                    if is_combining_mark(units_current) {
                        // Cascade vers une nouvelle touche pendant qu'on empile
                        // des Dead Keys : on empile, sans jamais rien injecter.
                        self.held_marks.push(units_current);
                        ipc_send!("HOLD:{}:{}", active_vk, idx_current);
                        if starts_new_cycle {
                            ipc_send!("SHOW:{}", vk_code);
                            self.state = State::CyclingVariants { active_vk: vk_code, index: 0, is_shifted };
                            return Action::Swallow;
                        }
                        self.state = State::ModifierHeld;
                        return Action::PassThrough;
                    }

                    let commit = self.take_commit_units(units_current);
                    if starts_new_cycle {
                        ipc_send!("SHOW:{}", vk_code);
                        self.state = State::CyclingVariants { active_vk: vk_code, index: 0, is_shifted };
                        return Action::CommitAndSwallow(commit);
                    } else {
                        ipc_send!("HIDE");
                        self.state = State::ModifierHeld;
                        return Action::CommitAndPassThrough(commit);
                    }
                }

                if !is_down && vk_code == active_vk { return Action::Swallow; }
                Action::PassThrough
            }
            State::Armed => {
                if is_alt {
                    // Pas de "HELD" ici : ça ferait basculer le HUD sur la
                    // pastille de langue et effacerait visuellement les
                    // diacritiques déjà empilées, alors que held_marks, lui,
                    // ne bouge pas. On laisse l'affichage armé tel quel
                    // jusqu'au prochain SHOW/HOLD/HIDE réel.
                    if is_down {
                        self.state = State::ModifierHeld;
                    }
                    return Action::Swallow;
                }

                // Backspace dépile juste la dernière diacritique empilée
                // (ex. Macron + Cédille alors qu'on voulait Tilde) au lieu
                // d'effacer un vrai caractère — toujours avalé, même en
                // auto-répétition, pour ne jamais laisser fuiter un vrai
                // Backspace vers l'appli pendant qu'on compose.
                if vk_code == VK_BACK.0 as u32 {
                    if is_down && !is_repeat {
                        self.held_marks.pop();
                        if self.held_marks.is_empty() {
                            ipc_send!("HIDE");
                            self.state = State::Idle;
                        } else {
                            ipc_send!("POP");
                        }
                    }
                    return Action::Swallow;
                }

                if is_down && !is_repeat {
                    if let Some(base_units) = plain_char_units(vk_code, is_shifted) {
                        let mut units = base_units;
                        for m in &self.held_marks {
                            units.push(m[0]); // une marque combinante = 1 unité
                        }
                        self.held_marks.clear();
                        ipc_send!("HIDE");
                        self.state = State::Idle;
                        return Action::CommitAndSwallow(units);
                    }
                }
                Action::PassThrough
            }
        }
    }
}

thread_local! {
    static HOOK_HANDLE: RefCell<Option<HHOOK>> = const { RefCell::new(None) };
    static STATE_MACHINE: RefCell<Option<StateMachine>> = const { RefCell::new(None) };
    // Touches actuellement enfoncées. Un simple "dernière touche" ne suffit
    // pas : maintenir 'e' en répétition puis appuyer sur Shift fait perdre
    // la trace de 'e' (devenue la nouvelle "dernière touche" = Shift), donc
    // le KEYDOWN répété suivant de 'e' n'est plus reconnu comme un repeat.
    static KEYS_DOWN: RefCell<HashSet<u32>> = RefCell::new(HashSet::new());
}

// Injecte une séquence d'unités UTF-16 — une simple paire de substituts, ou
// une lettre de base suivie de N diacritiques empilées (mode Dead Keys).
// Tous les KEYDOWN d'abord, puis tous les KEYUP, dans un seul SendInput :
// c'est cette atomicité qui permet à l'appli réceptrice de tout recomposer
// au lieu de recevoir des fragments orphelins.
fn inject_units(units: &[u16]) {
    let n = units.len();
    if n == 0 { return; }
    let mut inputs = vec![INPUT::default(); n * 2];
    for i in 0..n {
        inputs[i] = INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: units[i], dwFlags: KEYEVENTF_UNICODE, time: 0, dwExtraInfo: INJECTED_MAGIC_VALUE } } };
    }
    for i in 0..n {
        inputs[n + i] = INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: units[i], dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP, time: 0, dwExtraInfo: INJECTED_MAGIC_VALUE } } };
    }
    unsafe { let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32); }
}

unsafe extern "system" fn keyboard_hook_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= 0 {
        let kbd_struct = *(l_param.0 as *const KBDLLHOOKSTRUCT);
        if kbd_struct.dwExtraInfo == INJECTED_MAGIC_VALUE { return CallNextHookEx(None, n_code, w_param, l_param); }

        let msg = w_param.0 as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let vk_code = kbd_struct.vkCode;

        let mut is_repeat = false;
        KEYS_DOWN.with(|k| {
            let mut down = k.borrow_mut();
            if is_down {
                is_repeat = !down.insert(vk_code);
            } else {
                down.remove(&vk_code);
            }
        });

        let is_shift_down = (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
        let is_caps_on = (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0;
        let is_shifted = is_shift_down ^ is_caps_on;

        let action = STATE_MACHINE.with(|sm| {
            if let Some(ref mut state_machine) = *sm.borrow_mut() { state_machine.process_key(vk_code, is_down, is_shifted, is_repeat) } else { Action::PassThrough }
        });

        match action {
            Action::Swallow => return LRESULT(1),
            Action::CommitAndSwallow(c) => { inject_units(&c); return LRESULT(1); },
            Action::CommitAndPassThrough(c) => { inject_units(&c); },
            Action::PassThrough => {}
        }
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let config_path = if args.len() > 1 { &args[1] } else { "default.conf" };
    let config = match LayoutConfig::load_from_file(config_path) { Ok(c) => c, Err(code) => exit(code as i32), };
    let state_machine = StateMachine::new(config);
    STATE_MACHINE.with(|sm| *sm.borrow_mut() = Some(state_machine));

    // Thread dédié : seul lui touche stdout. Le hook ne fait que déposer dans
    // le canal (non bornant), jamais d'écriture ni de flush lui-même.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let _ = IPC_TX.set(tx);
    std::thread::spawn(move || {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        for msg in rx {
            let _ = writeln!(lock, "{msg}");
            let _ = lock.flush();
        }
    });

    unsafe {
        let h_instance = match GetModuleHandleW(None) { Ok(h) => h, Err(_) => exit(3), };
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), h_instance, 0) { Ok(h) => h, Err(_) => exit(3), };
        HOOK_HANDLE.with(|h| *h.borrow_mut() = Some(hook));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        HOOK_HANDLE.with(|h| { if let Some(hook) = h.borrow_mut().take() { let _ = UnhookWindowsHookEx(hook); } });
    }
}
