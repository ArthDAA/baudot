//! Écriture dans l'application active sous Windows (D2 B7) et résolution
//! position -> touche logique selon la disposition active.
//!
//! Aucune lecture de frappe ici : on n'écrit que sur ordre du Core.

#![cfg(windows)]

use omnikey_core::{KeyId, KeyResolver, LogicalKey, Order};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, MapVirtualKeyExW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_UNICODE,
    MAPVK_VSC_TO_VK_EX, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Même marqueur que le moteur v0.1 : une source hook (D2 B8) reconnaît
/// ainsi nos propres injections et les laisse passer sans les retraiter.
pub const INJECTED_MAGIC_VALUE: usize = 0xDEADBEEF;

/// Disposition de la fenêtre au premier plan, comme Windows l'appliquerait à
/// une frappe physique. C'est ce que le hook v0.1 recevait sous forme de VK.
pub struct WindowsResolver;

impl KeyResolver for WindowsResolver {
    fn resolve(&self, id: KeyId) -> LogicalKey {
        let Some((sc, extended)) = id.pc_scancode() else { return 0 };
        let code = if extended { 0xE000 | sc as u32 } else { sc as u32 };
        unsafe {
            let tid = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let hkl = GetKeyboardLayout(tid);
            MapVirtualKeyExW(code, MAPVK_VSC_TO_VK_EX, hkl)
        }
    }
}

fn key_input(scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: INJECTED_MAGIC_VALUE },
        },
    }
}

/// Texte : tous les appuis puis tous les relâchements dans un seul
/// SendInput, pour que l'application recompose base + marques d'un bloc
/// (identique à `inject_units` de v0.1).
fn write_units(units: &[u16]) {
    if units.is_empty() {
        return;
    }
    let mut inputs: Vec<INPUT> = units.iter().map(|&u| key_input(u, KEYEVENTF_UNICODE)).collect();
    inputs.extend(units.iter().map(|&u| key_input(u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Touche restituée par son scancode : Windows la traite comme une frappe
/// physique (disposition, Maj tenue, auto-complétion...), comme le
/// PassThrough de v0.1.
fn forward(id: KeyId, down: bool) {
    let Some((sc, extended)) = id.pc_scancode() else { return };
    let mut flags = KEYEVENTF_SCANCODE;
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    unsafe {
        SendInput(&[key_input(sc, flags)], std::mem::size_of::<INPUT>() as i32);
    }
}

/// Exécute les ordres du Core dans l'ordre donné.
pub fn execute(orders: &[Order]) {
    for o in orders {
        match o {
            Order::Write(units) => write_units(units),
            Order::Forward { id, down } => forward(*id, *down),
        }
    }
}
