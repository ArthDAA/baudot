//! Définition de "identique à v0.1" (D1 M12, AM3 ; D2 B4, B10).
//!
//! Chaque test pilote le même clavier physique à travers deux chaînes :
//! - v0.1 : hook -> `StateMachine` d'origine (copie verbatim de main.rs) ;
//! - v0.2 : modèle du firmware -> trames -> `Engine` -> ordres.
//! Le HUD doit recevoir exactement les mêmes messages, et l'application au
//! premier plan exactement le même texte et les mêmes touches.

mod common;

use common::firmware::Phys;
use common::{binaries_dir, texts, Bench, OsLayout};
use omnikey_core::protocol::DeviceFrame;
use omnikey_core::{conf, BaudotEvent, KeyId};

const VAR: Phys = Phys::Var;
const SHIFT: Phys = Phys::Shift;
const CAPS: Phys = Phys::Caps;
fn k(id: KeyId) -> Phys {
    Phys::Pos(id)
}

// ---------------------------------------------------------------------------
// L'oracle est bien v0.1
// ---------------------------------------------------------------------------

#[test]
fn oracle_is_verbatim_copy_of_v01() {
    let root = env!("CARGO_MANIFEST_DIR");
    let main = std::fs::read_to_string(format!("{root}/../main.rs")).unwrap();
    let oracle = std::fs::read_to_string(format!("{root}/tests/common/v01_oracle.rs")).unwrap();
    let start = main.find("// Une variante = un seul").unwrap();
    let end = main.find("thread_local! {").unwrap();
    let copy = oracle.split("// ORACLE-BEGIN\n").nth(1).unwrap();
    let norm = |s: &str| s.replace("\r\n", "\n");
    assert_eq!(norm(copy), norm(&main[start..end]), "tests/common/v01_oracle.rs a dérivé de main.rs");
}

#[test]
fn every_bundled_conf_gives_the_same_layout_as_v01() {
    let mut n = 0;
    for entry in std::fs::read_dir(binaries_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("conf") {
            continue;
        }
        let old = common::v01_oracle::LayoutConfig::load_from_file(path.to_str().unwrap()).unwrap();
        let new = conf::load_layout_file(&path).unwrap();
        assert_eq!(new.min_map, old.min_map, "{}", path.display());
        assert_eq!(new.maj_map, old.maj_map, "{}", path.display());
        n += 1;
    }
    assert_eq!(n, 39, "38 langues + perso.conf attendues");
}

// ---------------------------------------------------------------------------
// Séquences de référence nommées
// ---------------------------------------------------------------------------

fn run(conf: &str, layout: OsLayout, script: impl FnOnce(&mut Bench)) -> Bench {
    let mut b = Bench::new(conf, layout);
    script(&mut b);
    b.release_all();
    b.assert_equivalent(conf);
    b
}

#[test]
fn french_cycle_and_commit() {
    let b = run("french.conf", OsLayout::Us, |b| {
        b.press(VAR);
        b.tap(k(KeyId::E));
        b.tap(k(KeyId::E));
        b.release(VAR); // è
        b.press(VAR);
        b.press(SHIFT);
        b.tap(k(KeyId::E));
        b.release(SHIFT);
        b.release(VAR); // É
    });
    assert_eq!(texts(&b.v02.os), "èÉ");
}

#[test]
fn french_chained_keys_under_one_hold() {
    let b = run("french.conf", OsLayout::Us, |b| {
        b.press(VAR);
        b.tap(k(KeyId::E)); // é
        b.tap(k(KeyId::A)); // commit é, cycle a
        b.tap(k(KeyId::T)); // commit à, T sans variante : restituée
        b.release(VAR);
    });
    assert_eq!(texts(&b.v02.os), "éà");
}

#[test]
fn enter_during_cycle_commits_before_enter() {
    let b = run("french.conf", OsLayout::Us, |b| {
        b.press(VAR);
        b.tap(k(KeyId::E));
        b.tap(k(KeyId::ENTER));
        b.release(VAR);
    });
    let os = common::normalize(&b.v02.os);
    assert_eq!(os[0], common::OsItem::Text(vec![0xE9]));
    assert_eq!(os[1], common::OsItem::Key(0x0D, true));
}

#[test]
fn pinyin_dead_key_then_base_letter_after_release() {
    // Var + 1 (macron) relâché : armé ; la lettre suivante, tapée SANS Var,
    // reçoit la marque. C'est le cas qui a motivé l'armement (AM1).
    let b = run("mandarin_pinyin.conf", OsLayout::Us, |b| {
        b.press(VAR);
        b.tap(k(KeyId::N1));
        b.release(VAR);
        b.tap(k(KeyId::A));
        b.tap(k(KeyId::B)); // plus armé : frappe normale
    });
    assert_eq!(texts(&b.v02.os), "a\u{0304}");
    assert!(!b.v02.fw.armed);
}

#[test]
fn vietnamese_stacked_marks_and_backspace_pop() {
    let b = run("vietnamese.conf", OsLayout::Us, |b| {
        b.press(VAR);
        b.tap(k(KeyId::N1));
        b.tap(k(KeyId::N2)); // empile la 1re marque, cycle la 2e
        b.release(VAR); // empile la 2e : armé
        b.tap(k(KeyId::BACKSPACE)); // dépile
        b.tap(k(KeyId::LEFT)); // flèche : passe, reste armé (comme v0.1)
        b.tap(k(KeyId::O));
    });
    assert!(texts(&b.v02.os).starts_with('o'));
}

#[test]
fn armed_survives_reopening_var_without_selection() {
    run("mandarin_pinyin.conf", OsLayout::Us, |b| {
        b.press(VAR);
        b.tap(k(KeyId::N2));
        b.release(VAR); // armé
        b.press(VAR); // retour en ModifierHeld, marques conservées
        b.release(VAR); // toujours armé
        b.tap(k(KeyId::E));
    });
}

#[test]
fn language_cycle_commit_and_cancel() {
    run("french.conf", OsLayout::Us, |b| {
        b.press(VAR);
        b.tap(k(KeyId::SPACE));
        b.tap(k(KeyId::SPACE));
        b.tap(k(KeyId::SPACE));
        b.release(VAR);
        b.press(VAR);
        b.tap(k(KeyId::SPACE));
        b.tap(k(KeyId::ENTER)); // annule le sélecteur
        b.tap(k(KeyId::E));
        b.release(VAR);
    });
}

#[test]
fn azerty_os_layout_keeps_v01_key_identity() {
    // Sous Windows Français, la position QWERTY "Q" produit la touche A :
    // v0.1 y voyait VK_A, donc les variantes de "a".
    let b = run("french.conf", OsLayout::Fr, |b| {
        b.press(VAR);
        b.tap(k(KeyId::Q));
        b.release(VAR);
    });
    assert_eq!(texts(&b.v02.os), "à");
}

#[test]
fn caps_lock_selects_uppercase() {
    let b = run("french.conf", OsLayout::Us, |b| {
        b.tap(CAPS);
        b.press(VAR);
        b.tap(k(KeyId::C));
        b.release(VAR);
        b.tap(CAPS);
    });
    assert_eq!(texts(&b.v02.os), "Ç");
}

// ---------------------------------------------------------------------------
// Séquences aléatoires
// ---------------------------------------------------------------------------

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

const POOL: &[KeyId] = &[
    KeyId::E, KeyId::A, KeyId::U, KeyId::O, KeyId::I, KeyId::C, KeyId::Y, KeyId::Q, KeyId::T, KeyId::S,
    KeyId::N1, KeyId::N2, KeyId::N3, KeyId::N4, KeyId::N5, KeyId::SEMICOLON, KeyId::NONUS_HASH, KeyId::SLASH,
    KeyId::SPACE, KeyId::BACKSPACE, KeyId::ENTER, KeyId::UP, KeyId::LEFT, KeyId::LCTRL, KeyId::F1,
];

fn fuzz(conf: &str, layout: OsLayout, seed: u64, runs: usize) {
    let mut rng = Lcg(seed);
    for run in 0..runs {
        let mut b = Bench::new(conf, layout);
        let mut log = Vec::new();
        let len = 1 + rng.below(60);
        for _ in 0..len {
            let p = match rng.below(20) {
                0..=4 => VAR,
                5 | 6 => SHIFT,
                7 => CAPS,
                _ => k(POOL[rng.below(POOL.len() as u32) as usize]),
            };
            if p == CAPS {
                if b.down.contains(&CAPS) {
                    continue;
                }
                b.tap(CAPS);
                log.push(format!("tap {p:?}"));
            } else if b.down.contains(&p) {
                b.release(p);
                log.push(format!("up {p:?}"));
            } else {
                b.press(p);
                log.push(format!("down {p:?}"));
            }
        }
        b.release_all();
        b.assert_equivalent(&format!("{conf} {layout:?} run {run}:\n{}", log.join("\n")));
    }
}

#[test]
fn fuzz_french_us() {
    fuzz("french.conf", OsLayout::Us, 1, 4000);
}

#[test]
fn fuzz_french_fr() {
    fuzz("french.conf", OsLayout::Fr, 2, 4000);
}

#[test]
fn fuzz_pinyin_us() {
    fuzz("mandarin_pinyin.conf", OsLayout::Us, 3, 4000);
}

#[test]
fn fuzz_vietnamese_fr() {
    fuzz("vietnamese.conf", OsLayout::Fr, 4, 4000);
}

#[test]
fn fuzz_perso_us() {
    fuzz("perso.conf", OsLayout::Us, 5, 4000);
}

// ---------------------------------------------------------------------------
// Garanties du clavier (M2, M3, M8) sur le modèle du firmware
// ---------------------------------------------------------------------------

#[test]
fn no_frame_outside_var_or_arming_except_pending_releases() {
    let mut rng = Lcg(9);
    for _ in 0..2000 {
        let mut b = Bench::new("mandarin_pinyin.conf", OsLayout::Us);
        for _ in 0..40 {
            let p = if rng.below(4) == 0 { VAR } else { k(POOL[rng.below(POOL.len() as u32) as usize]) };
            let (held, armed) = (b.v02.fw.held, b.v02.fw.armed);
            let before = b.v02.frames.len();
            if b.down.contains(&p) { b.release(p) } else { b.press(p) }
            for f in &b.v02.frames[before..] {
                if let DeviceFrame::Event { event: BaudotEvent::KeyDown { id, .. }, .. } = f {
                    assert!(held || (armed && id.is_armable()), "KeyDown {id:?} hors Var et hors armement");
                }
            }
        }
        b.release_all();
    }
}

#[test]
fn without_software_var_is_neutral_and_nothing_is_swallowed() {
    let path = binaries_dir().join("french.conf");
    let mut c = common::ChainV02::new(path.to_str().unwrap(), OsLayout::Us, false);
    for p in [VAR, k(KeyId::E), SHIFT] {
        c.feed(p, true, false, false);
    }
    for p in [SHIFT, k(KeyId::E), VAR] {
        c.feed(p, false, false, false);
    }
    assert!(c.frames.is_empty());
    assert!(c.hud.is_empty());
    // E et Maj passent normalement, Var n'émet rien (M2).
    assert_eq!(c.os.len(), 4);
}

#[test]
fn losing_the_software_mid_gesture_releases_var() {
    let path = binaries_dir().join("french.conf");
    let mut c = common::ChainV02::new(path.to_str().unwrap(), OsLayout::Us, true);
    c.feed(VAR, true, false, false);
    c.fw.host_lost();
    c.feed(k(KeyId::E), true, false, false);
    c.feed(k(KeyId::E), false, false, false);
    c.feed(VAR, false, false, false);
    assert_eq!(c.frames.len(), 1, "seul le VarDown d'avant la perte");
    assert_eq!(c.os.len(), 2, "E passe normalement");
}
