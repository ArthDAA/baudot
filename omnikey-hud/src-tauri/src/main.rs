// Baudot Companion App v0.2 "Var" (by Lockfree), nom de code Omnikey.
// Lit le clavier Baudot par HID brut (aucun hook clavier), fait tourner la
// machine à états en mémoire (omnikey-core), écrit le caractère choisi et
// pilote le HUD, le tray, les profils .conf et le démarrage automatique.
// Messages HUD (lignes IPC héritées de v0.1) : HELD / SHOW:vk / CYCLE:idx /
// HOLD:vk:idx / POP / HIDE, plus LANGSHOW / LANGCYCLE:idx / LANGCOMMIT:idx /
// LANGCANCEL (Var+Espace, sélecteur de langue — voir handle_ipc).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, RunEvent, WebviewUrl, WebviewWindowBuilder,
    Wry,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, VK_CAPITAL, VK_SHIFT, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4,
    VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_UP,
    VK_DOWN, VK_LEFT, VK_RIGHT,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_NAME: &str = "Baudot Companion App";
// Nom de la valeur de démarrage automatique avant le renommage : retirée au
// lancement pour ne pas laisser d'entrée orpheline.
const LEGACY_APP_NAME: &str = "Omnikey";
const OMNIKEY_KEY: &str = r"Software\Lockfree\Omnikey";
const THEME_VALUE: &str = "Theme";
const PROFILE_VALUE: &str = "Profile";
const ACCENT_REMINDER_VALUE: &str = "AccentReminder";
// Taille de la bulle du HUD, en pourcentage de la taille d'origine.
const HUD_SCALE_VALUE: &str = "HudScale";
const HUD_SCALE_MIN: u32 = 100;
const HUD_SCALE_MAX: u32 = 150;
// Taille d'origine de la fenêtre HUD (tauri.conf.json), en pixels logiques.
const HUD_BASE_SIZE: (f64, f64) = (1000.0, 220.0);
const LANGUAGE_LABEL_WITH_ACCENTS_VALUE: &str = "LanguageLabelWithAccents";
const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

type ConfMap = HashMap<u32, Vec<String>>;

#[derive(Default)]
struct EngineState {
    min_map: Mutex<ConfMap>,
    maj_map: Mutex<ConfMap>,
    profile: Mutex<String>,
    language: Mutex<String>,
    theme_pref: Mutex<String>,
    autostart_item: Mutex<Option<CheckMenuItem<Wry>>>,
    // Édition à chaud : surveille le .conf actif et relance le moteur quand
    // il change sur disque. `watch_gen` sert d'anti-rebond (un save déclenche
    // souvent 2-3 évènements) : seul le dernier thread programmé qui voit
    // encore son numéro de génération après le délai déclenche le reload.
    conf_watcher: Mutex<Option<RecommendedWatcher>>,
    watch_gen: AtomicU64,
    last_self_write: Mutex<Option<Instant>>,
    // Mode Dead Keys : diacritiques empilées en attente d'une touche de base
    // (purement pour l'affichage HUD, l'empilement réel vit côté moteur).
    held_marks: Mutex<Vec<String>>,
    // Sélecteur de langue (AltGr+Espace) : la liste gelée au moment du
    // LANGSHOW (pour ne pas relire binaries/ à chaque LANGCYCLE) et l'index
    // de la langue courante dans cette liste — c'est ce départ qui permet à
    // LANGCYCLE de démarrer sur la langue déjà active plutôt que sur l'entrée
    // 0 de la liste. Vidé (None) dès que le geste se termine (LANGCOMMIT ou
    // LANGCANCEL), jamais laissé "pendant" entre deux gestes.
    lang_cycle: Mutex<Option<(Vec<Profile>, usize)>>,
    // v0.2 "Var" : machine à états en mémoire, alimentée par le clavier Var
    // (HID brut, sans hook). Toujours tenue à jour avec le profil actif ;
    // `var_keyboard` dit si un clavier Var parle en ce moment.
    core: Mutex<Option<omnikey_core::Engine>>,
    var_keyboard: AtomicBool,
    // Aperçu de la bulle pendant le réglage de sa taille : seul le dernier
    // aperçu programmé referme la bulle.
    hud_preview_gen: AtomicU64,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum HudEvent {
    Held { language: String },
    // `marks` : diacritiques déjà empilées (mode Dead Keys) au moment de ce
    // SHOW — vide la plupart du temps, non vide si on cycle une 2e touche
    // pendant qu'une 1re diacritique est déjà tenue.
    Show { variants: Vec<String>, marks: Vec<String> },
    Cycle { index: usize },
    Hold { marks: Vec<String> },
    Hide,
    // Sélecteur de langue (AltGr+Espace) : LangShow ouvre la liste (déjà
    // positionnée sur la langue active), LangCycle ne fait qu'avancer
    // l'index (liste déjà affichée, comme Cycle pour les variantes),
    // LangCommit fige la sélection avant que le frontend ne dissolve le HUD
    // (même logique que le dissolve de commit des variantes, HudApp.tsx).
    LangShow { languages: Vec<String>, index: usize },
    LangCycle { index: usize },
    LangCommit { index: usize },
    // État brut d'AltGr, indépendant de "kind" ci-dessus — voir le
    // commentaire dans process_key() (main.rs racine). Sert uniquement à
    // masquer le rappel diacritiques quand on quitte AltGr en mode Dead Keys
    // (State::Armed), sans toucher au reste du HUD.
    AltState { held: bool },
}

#[derive(Clone, Serialize)]
struct Profile {
    path: String,
    name: String,
    label: String,
}

// Nom réservé du profil personnel : jamais un des .conf de langue embarqués.
// Toujours ce qui tourne réellement (start_engine ne pointe jamais ailleurs
// que sur lui) — donc jamais listé parmi les profils sélectionnables : les
// "profils" de la liste ne sont que des sources à charger *dans* lui (voir
// load_language), pas des choix concurrents.
const PERSO_CONF_NAME: &str = "perso.conf";

// ---------------------------------------------------------------------------
// Parsing des .conf — mêmes règles que LayoutConfig::load_from_file du moteur
// (clé = premier caractère en majuscule → code VK), mais on conserve la chaîne
// complète de chaque variante pour l'affichage.
//
// Extension propre au frontend : une ligne optionnelle `lang: Nom` donne le
// libellé affiché par le HUD. Le moteur Rust (main.rs) l'ignore sans broncher
// (ni "min:"/"maj:" ni un '=', donc simplement sautée par son parseur).
// ---------------------------------------------------------------------------

struct ParsedConf {
    min_map: ConfMap,
    maj_map: ConfMap,
    language: String,
}

// U+25CC (cercle pointillé) en tête d'une variante n'est qu'un confort
// d'écriture pour représenter une marque combinante isolée dans le .conf
// (ex. "◌̄" au lieu de "̄" seul — illisible/quasi impossible à taper
// proprement dans un éditeur classique) : on le retire pour retomber sur
// exactement ce que le moteur (main.rs) voit et stocke lui-même. Sans ce
// retrait, held_marks/MarksTile afficheraient un cercle en trop (celui du
// .conf, plus celui déjà ajouté à l'affichage).
fn strip_leading_dotted_circle(v: &str) -> String {
    match v.strip_prefix('\u{25CC}') {
        // Rien après le cercle (variante "◌" toute seule) : comme le moteur,
        // on retombe sur le cercle lui-même plutôt que de perdre l'entrée.
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => v.to_string(),
    }
}

// Même table que key_char_to_vk() du moteur (main.rs) — doit rester
// identique : c'est elle qui permet de faire correspondre un SHOW:vk reçu de
// l'IPC à la bonne entrée de min_map/maj_map pour l'affichage du HUD.
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

fn parse_conf(path: &str) -> Result<ParsedConf, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {path}: {e}"))?;

    let mut min_map = ConfMap::new();
    let mut maj_map = ConfMap::new();
    let mut language: Option<String> = None;
    let mut section = "";
    // Un profil sans aucune variante (ex. english.conf : "lang:" + "min:" et
    // rien dessous) est légitime — voir LayoutConfig::load_from_file côté
    // moteur, qui applique le même critère. Seule l'absence des deux
    // marqueurs structurels ("lang:" et une section) signale un fichier
    // réellement malformé.
    let mut saw_section = false;

    for line in content.lines() {
        let line = line.trim_start_matches('\u{FEFF}').trim();
        if line.is_empty() {
            continue;
        }
        if line == "min:" {
            section = "min";
            saw_section = true;
            continue;
        }
        if line == "maj:" {
            section = "maj";
            saw_section = true;
            continue;
        }
        if let Some(name) = line.strip_prefix("lang:") {
            let name = name.trim();
            if !name.is_empty() {
                language = Some(name.to_string());
            }
            continue;
        }
        if let Some((key, values)) = line.split_once('=') {
            let Some(key_char) = key.trim().chars().next() else {
                continue;
            };
            let vk = key_char_to_vk(key_char);
            let variants: Vec<String> = values
                .split(',')
                // Le marqueur de provenance `*` (voir Variant) n'a de sens
                // que pour l'éditeur — cette carte ne sert qu'à l'affichage
                // HUD (variants_for), qui doit voir le caractère nu.
                .map(|v| strip_leading_dotted_circle(strip_lang_marker(v.trim()).0.trim()))
                .filter(|v| !v.is_empty())
                .collect();
            if variants.is_empty() {
                continue;
            }
            match section {
                "min" => {
                    min_map.insert(vk, variants);
                }
                "maj" => {
                    maj_map.insert(vk, variants);
                }
                _ => {}
            }
        }
    }

    if language.is_none() && !saw_section {
        return Err("Empty or invalid .conf file.".into());
    }
    Ok(ParsedConf {
        min_map,
        maj_map,
        language: language.unwrap_or_else(|| fallback_language_name(path)),
    })
}

// ---------------------------------------------------------------------------
// Découpe brute d'un .conf en préambule ("lang: ...") + lignes de chaque
// section — utilisée par load_language() et par la migration ponctuelle de
// l'ancien format à zones (voir plus bas). Un profil de langue embarqué
// (binaries/*.conf) n'a pas de notion de provenance propre : tout son
// contenu est repris tel quel puis marqué à l'insertion (voir Variant).
// ---------------------------------------------------------------------------

struct ConfSections {
    preamble: Vec<String>, // tout avant "min:" (dont "lang: ...")
    min_lines: Vec<String>,
    maj_lines: Vec<String>,
}

fn split_conf_sections(content: &str) -> ConfSections {
    let mut preamble = Vec::new();
    let mut min_lines = Vec::new();
    let mut maj_lines = Vec::new();
    let mut section = 0u8; // 0 = préambule, 1 = min, 2 = maj
    for line in content.lines() {
        match line.trim_start_matches('\u{FEFF}').trim() {
            "min:" => {
                section = 1;
                continue;
            }
            "maj:" => {
                section = 2;
                continue;
            }
            _ => {}
        }
        match section {
            0 => preamble.push(line.to_string()),
            1 => min_lines.push(line.to_string()),
            _ => maj_lines.push(line.to_string()),
        }
    }
    ConfSections { preamble, min_lines, maj_lines }
}

// ---------------------------------------------------------------------------
// Éditeur visuel (clavier + drag-and-drop) — lecture/écriture structurée de
// perso.conf, en JSON plutôt qu'en texte : le frontend ne reparse jamais lui-
// même un .conf, il ne fait que restituer/muter ce que le Rust lui a donné
// (voir la discussion sur l'audit : une seule source de vérité pour le
// format, jamais un 2e parseur dupliqué côté TS).
// ---------------------------------------------------------------------------

// Provenance d'une variante, posée UNE SEULE FOIS au moment de l'insertion
// et transportée avec elle (jamais recalculée depuis sa position) :
// `from_lang = true` signifie "insérée par un chargement de langue", donc
// jetable au prochain chargement (voir load_language). Une variante posée
// à la main par l'utilisateur — y compris en déplaçant une tuile marquée
// vers une nouvelle touche, ce qui NE remet PAS son marqueur à zéro — reste
// permanente. Sérialisée dans le .conf comme un simple suffixe `*` en fin
// de variante (voir parse_entries/entries_to_lines) : le moteur (main.rs à
// la racine) ignore déjà tout ce qui suit le premier caractère utile,
// aucune modification de sa part n'est nécessaire.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct Variant {
    char: String,
    from_lang: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct PersoEntry {
    key: String,
    variants: Vec<Variant>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PersoConfData {
    lang: String,
    min: Vec<PersoEntry>,
    maj: Vec<PersoEntry>,
}

fn normalize_key(raw: &str) -> Option<String> {
    let c = raw.trim().chars().next()?;
    Some(c.to_ascii_lowercase().to_string())
}

// Retire un marqueur de provenance `*` en fin de variante — voir Variant.
// Le `◌` U+25CC en tête (marque combinante isolée, voir
// strip_leading_dotted_circle) reste géré séparément par l'appelant : les
// deux sont indépendants et combinables ("◌̄*" est valide).
fn strip_lang_marker(v: &str) -> (String, bool) {
    match v.strip_suffix('*') {
        Some(rest) => (rest.to_string(), true),
        None => (v.to_string(), false),
    }
}

// Découpe des lignes "clé = v1, v2*, ..." d'une section déjà isolée (pas de
// min:/maj: à gérer ici, split_conf_sections s'en est déjà chargé) en
// entrées (clé, variantes). Variantes gardées verbatim, dotted circle
// compris — l'éditeur ne connaît pas les règles d'affichage du HUD,
// seulement ce qui est écrit dans le fichier — à l'exception du marqueur de
// provenance, retiré ici et porté par Variant::from_lang.
fn parse_entries(lines: &[String]) -> Vec<PersoEntry> {
    let mut out = Vec::new();
    for line in lines {
        let trimmed = line.trim_start_matches('\u{FEFF}').trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, values)) = trimmed.split_once('=') {
            let Some(key) = normalize_key(key) else {
                continue;
            };
            let mut variants: Vec<Variant> = Vec::new();
            for v in values.split(',') {
                let (char, from_lang) = strip_lang_marker(v.trim());
                if char.is_empty() {
                    continue;
                }
                if !variants.iter().any(|existing| existing.char == char) {
                    variants.push(Variant { char, from_lang });
                }
            }
            if !variants.is_empty() {
                out.push(PersoEntry { key, variants });
            }
        }
    }
    out
}

// Fusionne les doublons de clé en unissant leurs variantes (une même touche
// ne peut légitimement apparaître qu'une fois dans le fichier final) — ne
// touche plus à aucune notion de zone, supprimée (voir Variant).
fn merge_entries(entries: Vec<PersoEntry>) -> Vec<PersoEntry> {
    let mut out: Vec<PersoEntry> = Vec::new();
    for entry in entries {
        match out.iter_mut().find(|e| e.key == entry.key) {
            Some(existing) => {
                for v in entry.variants {
                    if !existing.variants.iter().any(|ev| ev.char == v.char) {
                        existing.variants.push(v);
                    }
                }
            }
            None => out.push(entry),
        }
    }
    out
}

fn mark_self_write(app: &AppHandle) {
    *app.state::<EngineState>().last_self_write.lock().unwrap() = Some(Instant::now());
}

fn entries_to_lines(entries: &[PersoEntry]) -> Vec<String> {
    // Une entrée sans variante (touche vidée dans l'éditeur) ne doit jamais
    // être écrite : le moteur fait `index % variants.len()` dès qu'une touche
    // mappée est pressée (CyclingVariants), donc une clé présente mais vide
    // ferait planter le process sur une division par zéro au premier essai.
    entries
        .iter()
        .filter(|e| !e.variants.is_empty())
        .map(|e| {
            let variants = e
                .variants
                .iter()
                .map(|v| if v.from_lang { format!("{}*", v.char) } else { v.char.clone() })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} = {variants}", e.key)
        })
        .collect()
}

#[tauri::command]
fn read_perso_conf(app: AppHandle) -> Result<PersoConfData, String> {
    let path = perso_conf_path(&app)?;
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let sections = split_conf_sections(&content);

    let lang = sections
        .preamble
        .iter()
        .find_map(|l| l.trim_start_matches('\u{FEFF}').trim().strip_prefix("lang:"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_language_name(&path.to_string_lossy()));

    let min = merge_entries(parse_entries(&sections.min_lines));
    let maj = merge_entries(parse_entries(&sections.maj_lines));

    Ok(PersoConfData { lang, min, maj })
}

#[tauri::command]
fn save_perso_conf(app: AppHandle, data: PersoConfData) -> Result<(), String> {
    let path = perso_conf_path(&app)?;
    let data = PersoConfData {
        lang: data.lang,
        min: merge_entries(data.min),
        maj: merge_entries(data.maj),
    };

    let mut out = vec![format!("lang: {}", data.lang)];
    out.push("min:".to_string());
    out.extend(entries_to_lines(&data.min));
    out.push("maj:".to_string());
    out.extend(entries_to_lines(&data.maj));

    let mut new_content = out.join("\n");
    new_content.push('\n');
    mark_self_write(&app);
    std::fs::write(&path, new_content)
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;

    activate_profile(&app, &path.to_string_lossy())?;
    let _ = app.emit("perso-changed", ());
    Ok(())
}

fn fallback_language_name(path: &str) -> String {
    let stem = PathBuf::from(path)
        .file_stem()
        .map(|s| s.to_string_lossy().replace(['_', '-'], " "))
        .unwrap_or_default();
    if stem.is_empty() {
        return "Profile".to_string();
    }
    stem.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Résolution des chemins (moteur + profils)
// ---------------------------------------------------------------------------

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

fn search_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = exe_dir() {
        dirs.push(d.join("files"));
        dirs.push(d);
    }
    if let Ok(d) = app.path().resource_dir() {
        dirs.push(d.join("binaries"));
    }
    // En dev (cargo run/tauri dev), on retombe sur la racine du projet hud_v2
    // où vivent french.conf et le moteur fraîchement compilé.
    #[cfg(debug_assertions)]
    {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        dirs.push(root.join("files"));
        dirs.push(root);
    }
    dirs.retain(|d| d.is_dir());
    dirs
}

// ---------------------------------------------------------------------------
// Migration unique de l'ancien format à zones vers le format à marqueurs `*`
// (voir Variant). Un perso.conf écrit par l'ancien code n'a jamais de
// marqueur ; sa présence signale donc un fichier déjà au nouveau format
// (qu'il vienne d'un vrai chargement de langue ou d'une migration
// précédente), auquel cas on ne touche à rien — cette logique ne s'exécute
// donc utilement qu'une fois par fichier, à la première résolution de
// perso_conf_path() qui suit la mise à jour. La coupe "1re ligne vide =
// frontière de zone" n'existe plus que localement ici, pour cet usage
// ponctuel : le reste du fichier n'a plus aucune notion de zone (voir
// item 2 du brief).
// ---------------------------------------------------------------------------

fn migration_zone_split(lines: &[String]) -> (Vec<String>, Vec<String>) {
    let mut lang_zone = Vec::new();
    let mut perso_zone = Vec::new();
    let mut past_blank = false;
    for line in lines {
        if !past_blank && line.trim().is_empty() {
            past_blank = true;
            continue;
        }
        if past_blank {
            perso_zone.push(line.clone());
        } else {
            lang_zone.push(line.clone());
        }
    }
    (lang_zone, perso_zone)
}

// Zone langue -> variantes marquées, zone perso -> non marquées, puis fusion
// (une clé pouvait apparaître dans les deux zones sous l'ancien format).
fn migration_section(lines: &[String]) -> Vec<PersoEntry> {
    let (lang_zone, perso_zone) = migration_zone_split(lines);
    let mut entries = merge_entries(parse_entries(&lang_zone));
    for e in entries.iter_mut() {
        for v in e.variants.iter_mut() {
            v.from_lang = true;
        }
    }
    for e in merge_entries(parse_entries(&perso_zone)) {
        match entries.iter_mut().find(|ex| ex.key == e.key) {
            Some(existing) => {
                for v in e.variants {
                    if !existing.variants.iter().any(|ev| ev.char == v.char) {
                        existing.variants.push(v);
                    }
                }
            }
            None => entries.push(e),
        }
    }
    entries
}

// Some(nouveau_contenu) si une migration a eu lieu, None si le fichier est
// déjà au nouveau format (marqueur présent) ou vide/sans section.
fn migrate_zone_format(content: &str) -> Option<String> {
    if content.is_empty() || content.contains('*') {
        return None;
    }
    let sections = split_conf_sections(content);
    if sections.min_lines.is_empty() && sections.maj_lines.is_empty() {
        return None;
    }

    let lang = sections
        .preamble
        .iter()
        .find_map(|l| l.trim_start_matches('\u{FEFF}').trim().strip_prefix("lang:"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let mut out = Vec::new();
    if let Some(lang) = lang {
        out.push(format!("lang: {lang}"));
    }
    out.push("min:".to_string());
    out.extend(entries_to_lines(&migration_section(&sections.min_lines)));
    out.push("maj:".to_string());
    out.extend(entries_to_lines(&migration_section(&sections.maj_lines)));
    let mut new_content = out.join("\n");
    new_content.push('\n');
    Some(new_content)
}

// perso.conf est le seul fichier que l'app modifie elle-même : il doit donc
// vivre dans un dossier garanti inscriptible par l'utilisateur courant,
// jamais dans l'arbre d'installation (<exe>/... ou les ressources bundlées),
// qui ne reste accessible en écriture que par défaut du bundler NSIS
// (installation per-user) — une bascule en perMachine ou un dossier protégé
// suffit à casser l'écriture.
//
// Au premier accès sans fichier au nouvel emplacement, on amorce son contenu
// (copie, jamais de référence directe) :
// - en priorité depuis l'ancien emplacement inscriptible (<exe>/files),
//   pour ne pas perdre un vrai perso.conf déjà personnalisé par l'usager ;
// - sinon depuis le perso.conf par défaut livré en ressource (lecture
//   seule, binaries/perso.conf), qui ne doit jamais être utilisé tel quel
//   comme cible d'écriture.
fn perso_conf_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Cannot locate the configuration folder: {e}"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let target = dir.join(PERSO_CONF_NAME);
    if !target.is_file() {
        let legacy = exe_dir()
            .map(|d| d.join("files").join(PERSO_CONF_NAME))
            .filter(|p| p.is_file());
        let seed = legacy.or_else(|| {
            app.path()
                .resource_dir()
                .ok()
                .map(|d| d.join("binaries").join(PERSO_CONF_NAME))
                .filter(|p| p.is_file())
        });
        if let Some(seed) = seed {
            let _ = std::fs::copy(&seed, &target);
        }
    }
    // Migration ponctuelle ancien format -> marqueurs (voir
    // migrate_zone_format) : no-op silencieux si déjà au nouveau format.
    // S'applique aussi au contenu tout juste amorcé ci-dessus (le seed,
    // legacy ou ressource, n'a jamais de marqueur non plus).
    if let Ok(content) = std::fs::read_to_string(&target) {
        if let Some(migrated) = migrate_zone_format(&content) {
            mark_self_write(app);
            let _ = std::fs::write(&target, migrated);
        }
    }
    Ok(target)
}

fn list_conf_files(app: &AppHandle) -> Vec<Profile> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in search_dirs(app) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("conf") {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // perso.conf n'est jamais "un profil parmi d'autres" à choisir —
            // c'est ce qui tourne déjà en permanence, jamais listé (voir
            // PERSO_CONF_NAME).
            if name == PERSO_CONF_NAME {
                continue;
            }
            if seen.insert(name.clone()) {
                let path_str = path.to_string_lossy().into_owned();
                let label = parse_conf(&path_str)
                    .map(|c| c.language)
                    .unwrap_or_else(|_| fallback_language_name(&path_str));
                out.push(Profile {
                    path: path_str,
                    name,
                    label,
                });
            }
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

// ---------------------------------------------------------------------------
// Langues sélectionnées pour le cycle AltGr+Espace (LanguageBank.tsx) — un
// sous-ensemble curé et ORDONNÉ des profils de list_conf_files (l'ordre fixe
// l'ordre de cycle). Persisté à part de perso.conf : ce n'est pas une langue
// chargée, juste la liste de ce qui est proposé au clavier. Même exigence
// d'inscriptibilité que perso.conf (voir perso_conf_path) : app_config_dir(),
// pas l'arbre d'installation. Jamais bundlé en ressource (ce fichier n'a
// aucune existence avant que l'usager ne compose un premier cycle), donc pas
// de repli "seed depuis les ressources" à prévoir ici, juste la migration
// depuis l'ancien emplacement inscriptible s'il en reste un.
const SELECTED_LANGUAGES_FILE: &str = "selected_languages.json";

fn selected_languages_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Cannot locate the configuration folder: {e}"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let target = dir.join(SELECTED_LANGUAGES_FILE);
    if !target.is_file() {
        let legacy = exe_dir()
            .map(|d| d.join("files").join(SELECTED_LANGUAGES_FILE))
            .filter(|p| p.is_file());
        if let Some(legacy) = legacy {
            let _ = std::fs::copy(&legacy, &target);
        }
    }
    Ok(target)
}

// Silencieux par construction (jamais de fichier au tout premier lancement,
// JSON parfois corrompu à la main) : une liste vide retombe simplement sur
// "rien de curé encore", jamais une erreur bloquante.
fn read_selected_language_names(app: &AppHandle) -> Vec<String> {
    let Ok(path) = selected_languages_path(app) else { return Vec::new(); };
    let Ok(content) = std::fs::read_to_string(&path) else { return Vec::new(); };
    serde_json::from_str(&content).unwrap_or_default()
}

fn write_selected_language_names(app: &AppHandle, names: &[String]) -> Result<(), String> {
    let path = selected_languages_path(app)?;
    let content = serde_json::to_string_pretty(names)
        .map_err(|e| format!("Cannot serialize: {e}"))?;
    std::fs::write(&path, content).map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

// Résout les noms de fichiers persistés vers de vrais Profile à jour (donc
// avec le bon `label`, au cas où un .conf aurait changé de lang: entre deux
// lancements) — toute entrée dont le fichier a disparu est silencieusement
// abandonnée plutôt que de garder un fantôme dans la liste.
fn resolve_selected_languages(app: &AppHandle) -> Vec<Profile> {
    let names = read_selected_language_names(app);
    let all = list_conf_files(app);
    names
        .iter()
        .filter_map(|n| all.iter().find(|p| &p.name == n))
        .cloned()
        .collect()
}

#[tauri::command]
fn list_selected_languages(app: AppHandle) -> Vec<Profile> {
    resolve_selected_languages(&app)
}

#[tauri::command]
fn save_selected_languages(app: AppHandle, names: Vec<String>) -> Result<(), String> {
    write_selected_language_names(&app, &names)
}

// ---------------------------------------------------------------------------
// Profil actif
// ---------------------------------------------------------------------------

// Active un profil : le Core l'utilise pour les variantes, le HUD pour
// l'affichage, et il est surveillé pour le rechargement à chaud.
fn activate_profile(app: &AppHandle, config_path: &str) -> Result<(), String> {
    let conf_abs = std::path::absolute(config_path)
        .map_err(|e| format!("Invalid path: {e}"))?
        .to_string_lossy()
        .into_owned();

    let parsed = parse_conf(&conf_abs)?;
    let layout = omnikey_core::conf::load_layout_file(std::path::Path::new(&conf_abs)).map_err(|e| e.to_string())?;

    let state = app.state::<EngineState>();
    *state.min_map.lock().unwrap() = parsed.min_map;
    *state.maj_map.lock().unwrap() = parsed.maj_map;
    *state.language.lock().unwrap() = parsed.language;
    {
        let mut engine = state.core.lock().unwrap();
        match engine.as_mut() {
            Some(e) => e.set_layout(layout),
            None => *engine = Some(omnikey_core::Engine::new(layout)),
        }
    }
    *state.profile.lock().unwrap() = conf_abs.clone();

    watch_conf_file(app, &conf_abs);
    // Best-effort : le prochain démarrage repart sur ce profil, quel que
    // soit son nom de fichier. Une écriture registre ratée n'empêche pas
    // le Core de tourner normalement.
    if let Err(e) = write_last_profile(&conf_abs) {
        eprintln!("Impossible de mémoriser le profil actif : {e}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// v0.2 "Var" : clavier lu par HID brut (D2 B3), Core en mémoire (B4),
// écriture Windows (B7), mêmes événements HUD qu'en v0.1 (B6).
// ---------------------------------------------------------------------------

// Fermeture de l'app : remet le Core au repos et relâche côté Windows toute
// touche restituée encore enfoncée. Le clavier, lui, cesse de rediriger de
// lui-même faute de réponse à son signal de vie (M8).
fn shutdown_core(app: &AppHandle) {
    let state = app.state::<EngineState>();
    let decision = state.core.lock().unwrap().as_mut().map(|e| e.reset());
    if let Some(d) = decision {
        omnikey_inject_windows::execute(&d.orders);
    }
}

struct VarSink {
    app: AppHandle,
}

impl VarSink {
    // Ordres d'abord, HUD ensuite : même ordre qu'en v0.1, où le moteur
    // injectait avant que sa ligne IPC n'arrive ici. Le verrou du Core est
    // déjà relâché : handle_ipc peut recharger le profil (LANGCOMMIT).
    fn apply(&self, d: omnikey_core::Decision) -> Vec<omnikey_core::HostFrame> {
        omnikey_inject_windows::execute(&d.orders);
        for msg in &d.hud {
            handle_ipc(&self.app, &msg.ipc_line());
        }
        d.to_keyboard
    }

    fn with_core(&self, f: impl FnOnce(&mut omnikey_core::Engine) -> omnikey_core::Decision) -> Option<omnikey_core::Decision> {
        let state = self.app.state::<EngineState>();
        let mut guard = state.core.lock().unwrap();
        guard.as_mut().map(f)
    }
}

impl omnikey_hid::Sink for VarSink {
    fn connected(&mut self, variant: &'static str) {
        eprintln!("Clavier Baudot connecté (K3 Max {variant}).");
        self.app.state::<EngineState>().var_keyboard.store(true, Ordering::SeqCst);
    }

    fn incoming(&mut self, msg: omnikey_hid::Incoming) -> Vec<omnikey_core::HostFrame> {
        let decision = match msg {
            omnikey_hid::Incoming::Event(ev) => self.with_core(|e| e.handle(ev, &omnikey_inject_windows::WindowsResolver)),
            omnikey_hid::Incoming::SequenceGap => self.with_core(|e| e.reset()),
            omnikey_hid::Incoming::VersionMismatch(v) => {
                eprintln!("Clavier Var : version de protocole {v} non prise en charge.");
                None
            }
        };
        decision.map(|d| self.apply(d)).unwrap_or_default()
    }

    fn disconnected(&mut self) {
        eprintln!("Clavier Baudot déconnecté.");
        if let Some(d) = self.with_core(|e| e.reset()) {
            self.apply(d);
        }
        self.app.state::<EngineState>().var_keyboard.store(false, Ordering::SeqCst);
    }
}

// Édition à chaud : quand le .conf actif est modifié sur disque, on relance
// le moteur dessus (mêmes étapes qu'un clic sur le profil dans Settings),
// ce qui le fait relire par le moteur *et* reparser côté frontend.
fn watch_conf_file(app: &AppHandle, conf_path: &str) {
    let target = PathBuf::from(conf_path);
    let Some(dir) = target.parent().map(PathBuf::from) else {
        return;
    };
    let Some(file_name) = target.file_name().map(|n| n.to_os_string()) else {
        return;
    };

    let state = app.state::<EngineState>();
    // Invalide tout rechargement en attente pour l'ancien fichier surveillé.
    state.watch_gen.fetch_add(1, Ordering::SeqCst);

    let watcher_app = app.clone();
    let conf_path = conf_path.to_string();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
            return;
        }
        if !event.paths.iter().any(|p| p.file_name() == Some(file_name.as_os_str())) {
            return;
        }

        let state = watcher_app.state::<EngineState>();
        let self_write = state
            .last_self_write
            .lock()
            .unwrap()
            .map_or(false, |t| t.elapsed() < Duration::from_millis(1500));
        if self_write {
            return;
        }
        let my_gen = state.watch_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let app = watcher_app.clone();
        let conf_path = conf_path.clone();
        std::thread::spawn(move || {
            // Anti-rebond : un simple Ctrl+S déclenche souvent 2-3 évènements
            // (écriture + renommage atomique selon l'éditeur). On attend que
            // plus rien ne bouge pendant 300 ms avant de relancer le moteur.
            std::thread::sleep(Duration::from_millis(300));
            let state = app.state::<EngineState>();
            if state.watch_gen.load(Ordering::SeqCst) != my_gen {
                return; // un évènement plus récent a pris le relais
            }
            if let Err(e) = activate_profile(&app, &conf_path) {
                eprintln!("Rechargement à chaud du .conf échoué : {e}");
            } else {
                let _ = app.emit("perso-changed", ());
            }
        });
    });

    let Ok(mut watcher) = watcher else { return };
    if watcher.watch(&dir, RecursiveMode::NonRecursive).is_err() {
        return;
    }
    // Remplacer l'ancien watcher le fait "drop" — il arrête de surveiller
    // tout seul, pas besoin de l'arrêter explicitement.
    *state.conf_watcher.lock().unwrap() = Some(watcher);
}

fn handle_ipc(app: &AppHandle, cmd: &str) {
    if cmd == "HELD" {
        position_hud(app);
        let language = app.state::<EngineState>().language.lock().unwrap().clone();
        emit_hud(app, HudEvent::Held { language });
    } else if let Some(vk) = cmd.strip_prefix("SHOW:") {
        if let Ok(vk) = vk.parse::<u32>() {
            if let Some(variants) = variants_for(app, vk) {
                let marks = app.state::<EngineState>().held_marks.lock().unwrap().clone();
                emit_hud(app, HudEvent::Show { variants, marks });
            }
        }
    } else if let Some(idx) = cmd.strip_prefix("CYCLE:") {
        if let Ok(index) = idx.parse::<usize>() {
            emit_hud(app, HudEvent::Cycle { index });
        }
    } else if let Some(rest) = cmd.strip_prefix("HOLD:") {
        if let Some((vk_str, idx_str)) = rest.split_once(':') {
            if let (Ok(vk), Ok(idx)) = (vk_str.parse::<u32>(), idx_str.parse::<usize>()) {
                if let Some(variants) = variants_for(app, vk) {
                    if !variants.is_empty() {
                        let mark = variants[idx % variants.len()].clone();
                        let state = app.state::<EngineState>();
                        let marks = {
                            let mut held = state.held_marks.lock().unwrap();
                            held.push(mark);
                            held.clone()
                        };
                        emit_hud(app, HudEvent::Hold { marks });
                    }
                }
            }
        }
    } else if cmd == "POP" {
        // Backspace côté moteur a dépilé la dernière marque : on reflète
        // juste la même liste, pas besoin de relire quoi que ce soit.
        let state = app.state::<EngineState>();
        let marks = {
            let mut held = state.held_marks.lock().unwrap();
            held.pop();
            held.clone()
        };
        emit_hud(app, HudEvent::Hold { marks });
    } else if cmd == "HIDE" {
        app.state::<EngineState>().held_marks.lock().unwrap().clear();
        emit_hud(app, HudEvent::Hide);
    } else if cmd == "LANGSHOW" {
        // Gèle la liste maintenant (pas à chaque LANGCYCLE) et démarre sur la
        // langue actuellement active plutôt que sur la 1re de la liste — la
        // bulle affichée ("Français") se déroule *depuis* elle-même.
        // Priorité à la liste curée (LanguageBank.tsx) ; tant qu'elle est
        // vide (rien glissé dedans encore), on retombe sur le catalogue
        // complet plutôt que de rendre le raccourci muet dès l'installation.
        let mut profiles = resolve_selected_languages(app);
        if profiles.is_empty() {
            profiles = list_conf_files(app);
        }
        let current = app.state::<EngineState>().language.lock().unwrap().clone();
        let start = profiles.iter().position(|p| p.label == current).unwrap_or(0);
        let languages: Vec<String> = profiles.iter().map(|p| p.label.clone()).collect();
        *app.state::<EngineState>().lang_cycle.lock().unwrap() = Some((profiles, start));
        emit_hud(app, HudEvent::LangShow { languages, index: start });
    } else if let Some(raw) = cmd.strip_prefix("LANGCYCLE:") {
        if let Ok(raw) = raw.parse::<usize>() {
            let state = app.state::<EngineState>();
            let guard = state.lang_cycle.lock().unwrap();
            if let Some((profiles, start)) = guard.as_ref() {
                if !profiles.is_empty() {
                    let index = (start + raw) % profiles.len();
                    drop(guard);
                    emit_hud(app, HudEvent::LangCycle { index });
                }
            }
        }
    } else if let Some(raw) = cmd.strip_prefix("LANGCOMMIT:") {
        if let Ok(raw) = raw.parse::<usize>() {
            let picked = {
                let state = app.state::<EngineState>();
                let mut guard = state.lang_cycle.lock().unwrap();
                guard.take().and_then(|(profiles, start)| {
                    if profiles.is_empty() {
                        None
                    } else {
                        let index = (start + raw) % profiles.len();
                        Some((index, profiles[index].clone()))
                    }
                })
            };
            match picked {
                Some((index, profile)) => {
                    emit_hud(app, HudEvent::LangCommit { index });
                    // Même chemin que "Charger une langue" dans Settings :
                    // fusionne la zone langue du profil choisi dans
                    // perso.conf (zone perso préservée) et relance le moteur.
                    if let Err(e) = load_language(app.clone(), profile.path) {
                        eprintln!("Changement de langue (AltGr+Espace) échoué : {e}");
                    }
                }
                None => emit_hud(app, HudEvent::Hide),
            }
        }
    } else if cmd == "LANGCANCEL" {
        app.state::<EngineState>().lang_cycle.lock().unwrap().take();
        emit_hud(app, HudEvent::Hide);
    } else if let Some(raw) = cmd.strip_prefix("ALT:") {
        emit_hud(app, HudEvent::AltState { held: raw == "1" });
    }
}

fn emit_hud(app: &AppHandle, ev: HudEvent) {
    let _ = app.emit_to("hud", "hud", ev);
}

// Le moteur n'envoie que le code VK ; on relit l'état Shift/CapsLock pour
// choisir la table min/maj, comme lui au moment du keydown. Purement visuel :
// le caractère injecté vient du moteur, pas d'ici.
fn variants_for(app: &AppHandle, vk: u32) -> Option<Vec<String>> {
    let shift_down = unsafe { (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 };
    let caps_on = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };
    let is_shifted = shift_down ^ caps_on;

    let state = app.state::<EngineState>();
    let min = state.min_map.lock().unwrap();
    let maj = state.maj_map.lock().unwrap();
    let (first, second) = if is_shifted { (&maj, &min) } else { (&min, &maj) };
    first
        .get(&vk)
        .or_else(|| second.get(&vk))
        .filter(|v| !v.is_empty())
        .cloned()
}

// ---------------------------------------------------------------------------
// Fenêtres — HUD centré en haut du moniteur actif (get_active_monitor_center)
// ---------------------------------------------------------------------------

fn position_hud(app: &AppHandle) {
    let Some(win) = app.get_webview_window("hud") else {
        return;
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return;
        }
        let mon_w = mi.rcMonitor.right - mi.rcMonitor.left;
        let win_w = win.outer_size().map(|s| s.width as i32).unwrap_or(1000);
        let x = mi.rcMonitor.left + (mon_w - win_w) / 2;
        // Fenêtre collée au bord supérieur de l'écran : l'écart visible au-dessus
        // de l'île vient uniquement de la marge de .hud-anchor (place de l'ombre).
        let y = mi.rcMonitor.top;
        let _ = win.set_position(PhysicalPosition::new(x, y));
        let _ = win.set_always_on_top(true);
    }
}

fn open_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("Baudot Companion App - Settings")
        .inner_size(580.0, 680.0)
        .min_inner_size(480.0, 460.0)
        .resizable(true)
        .maximizable(false)
        .center()
        .build();
    if let Ok(win) = built {
        let _ = win.set_icon(taskbar_icon());
    }
}

// ---------------------------------------------------------------------------
// Démarrage automatique (registre HKCU\...\Run, comme frontend.py)
// ---------------------------------------------------------------------------

fn autostart_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" --startup-silent", exe.display()))
}

fn autostart_enabled() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(APP_NAME))
        .is_ok()
}

// Démarrage automatique actif mais pointant vers un autre exécutable (build
// de développement, ancienne installation) : on le recale sur celui-ci.
fn repair_autostart_path() {
    let current = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(APP_NAME));
    if let (Ok(current), Some(expected)) = (current, autostart_command()) {
        if current != expected {
            let _ = set_autostart_registry(true);
        }
    }
}

// Démarrage automatique enregistré sous l'ancien nom : on le transfère sous
// le nouveau (même réglage pour l'utilisateur, plus d'entrée orpheline).
fn migrate_legacy_autostart() {
    let legacy = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(LEGACY_APP_NAME))
        .is_ok();
    if legacy {
        let _ = set_autostart_registry(true);
    }
}

fn set_autostart_registry(enabled: bool) -> Result<(), String> {
    let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey(RUN_KEY)
        .map_err(|e| format!("Registry not accessible: {e}"))?;
    // Entrée d'avant le renommage : toujours retirée, qu'on active ou non.
    let _ = key.delete_value(LEGACY_APP_NAME);
    if enabled {
        let cmd = autostart_command().ok_or("Executable path not found.")?;
        key.set_value(APP_NAME, &cmd)
            .map_err(|e| format!("Cannot write to the registry: {e}"))?;
    } else {
        match key.delete_value(APP_NAME) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Cannot delete from the registry: {e}")),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Thème (registre HKCU\Software\Lockfree\Omnikey) — "system" | "light" | "dark".
// "system" se résout via la préférence claire/sombre de Windows.
// ---------------------------------------------------------------------------

fn read_theme_preference() -> String {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(OMNIKEY_KEY)
        .and_then(|k| k.get_value::<String, _>(THEME_VALUE))
        .unwrap_or_else(|_| "system".to_string())
}

fn write_theme_preference(preference: &str) -> Result<(), String> {
    let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey(OMNIKEY_KEY)
        .map_err(|e| format!("Registry not accessible: {e}"))?;
    key.set_value(THEME_VALUE, &preference.to_string())
        .map_err(|e| format!("Cannot write to the registry: {e}"))
}

// Dernier profil actif (registre HKCU\Software\Lockfree\Omnikey\Profile) —
// remplace la convention "un fichier nommé default.conf" : le nom de fichier
// du profil de départ n'a plus besoin d'être figé, l'app se souvient juste
// du dernier choisi dans Settings.
fn read_last_profile() -> Option<String> {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(OMNIKEY_KEY)
        .and_then(|k| k.get_value::<String, _>(PROFILE_VALUE))
        .ok()
}

fn write_last_profile(path: &str) -> Result<(), String> {
    let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey(OMNIKEY_KEY)
        .map_err(|e| format!("Registry not accessible: {e}"))?;
    key.set_value(PROFILE_VALUE, &path.to_string())
        .map_err(|e| format!("Cannot write to the registry: {e}"))
}

// Rappel de diacritiques (registre HKCU\Software\Lockfree\Omnikey\AccentReminder)
// — activé par défaut (get_value échoue tant que la clé n'a jamais été
// écrite : c'est le seul cas qui doit retomber sur `true`, jamais sur `0`).
fn read_accent_reminder_preference() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(OMNIKEY_KEY)
        .and_then(|k| k.get_value::<u32, _>(ACCENT_REMINDER_VALUE))
        .map(|v| v != 0)
        .unwrap_or(true)
}

fn write_accent_reminder_preference(enabled: bool) -> Result<(), String> {
    let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey(OMNIKEY_KEY)
        .map_err(|e| format!("Registry not accessible: {e}"))?;
    key.set_value(ACCENT_REMINDER_VALUE, &(enabled as u32))
        .map_err(|e| format!("Cannot write to the registry: {e}"))
}

// ---------------------------------------------------------------------------
// Taille de la bulle : zoom de la fenêtre HUD (tout grossit d'un bloc, rendu
// inchangé) et fenêtre agrandie d'autant pour ne rien couper.
// ---------------------------------------------------------------------------

fn read_hud_scale() -> u32 {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(OMNIKEY_KEY)
        .and_then(|k| k.get_value::<u32, _>(HUD_SCALE_VALUE))
        .map(|v| v.clamp(HUD_SCALE_MIN, HUD_SCALE_MAX))
        .unwrap_or(HUD_SCALE_MIN)
}

fn write_hud_scale(percent: u32) -> Result<(), String> {
    let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey(OMNIKEY_KEY)
        .map_err(|e| format!("Registry not accessible: {e}"))?;
    key.set_value(HUD_SCALE_VALUE, &percent)
        .map_err(|e| format!("Cannot write to the registry: {e}"))
}

fn apply_hud_scale(app: &AppHandle, percent: u32) {
    let Some(hud) = app.get_webview_window("hud") else { return };
    let scale = percent as f64 / 100.0;
    let _ = hud.set_size(tauri::LogicalSize::new(HUD_BASE_SIZE.0 * scale, HUD_BASE_SIZE.1 * scale));
    let _ = hud.set_zoom(scale);
}

// Montre la bulle un instant à sa nouvelle taille, sauf si un vrai geste Var
// est en cours (on ne touche jamais au HUD pendant une saisie).
fn preview_hud(app: &AppHandle) {
    let idle = |app: &AppHandle| {
        let state = app.state::<EngineState>();
        let core = state.core.lock().unwrap();
        core.as_ref().map_or(true, |e| e.machine().state() == omnikey_core::machine::State::Idle)
    };
    if !idle(app) {
        return;
    }
    let state = app.state::<EngineState>();
    let gen = state.hud_preview_gen.fetch_add(1, Ordering::SeqCst) + 1;
    position_hud(app);
    let language = state.language.lock().unwrap().clone();
    emit_hud(app, HudEvent::Held { language });
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1200));
        let state = app.state::<EngineState>();
        if state.hud_preview_gen.load(Ordering::SeqCst) == gen && idle(&app) {
            emit_hud(&app, HudEvent::Hide);
        }
    });
}

#[tauri::command]
fn get_hud_scale() -> u32 {
    read_hud_scale()
}

#[tauri::command]
fn set_hud_scale(app: AppHandle, percent: u32) -> Result<(), String> {
    let percent = percent.clamp(HUD_SCALE_MIN, HUD_SCALE_MAX);
    write_hud_scale(percent)?;
    apply_hud_scale(&app, percent);
    preview_hud(&app);
    Ok(())
}

// Affichage du nom de langue (pastille "waiting") quand le rappel
// diacritiques est déjà visible pour cette langue — activé par défaut (même
// logique unwrap_or(true) que ci-dessus). N'a aucun effet quand il n'y a
// aucun diacritique à rappeler : le nom de langue reste alors toujours
// affiché (seul repère disponible dans ce cas), voir AccentHUD.tsx.
fn read_language_label_with_accents_preference() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(OMNIKEY_KEY)
        .and_then(|k| k.get_value::<u32, _>(LANGUAGE_LABEL_WITH_ACCENTS_VALUE))
        .map(|v| v != 0)
        .unwrap_or(true)
}

fn write_language_label_with_accents_preference(enabled: bool) -> Result<(), String> {
    let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey(OMNIKEY_KEY)
        .map_err(|e| format!("Registry not accessible: {e}"))?;
    key.set_value(LANGUAGE_LABEL_WITH_ACCENTS_VALUE, &(enabled as u32))
        .map_err(|e| format!("Cannot write to the registry: {e}"))
}

fn system_prefers_light() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(PERSONALIZE_KEY)
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v != 0)
        .unwrap_or(false)
}

fn resolve_effective_theme(preference: &str) -> &'static str {
    match preference {
        "light" => "light",
        "dark" => "dark",
        _ => {
            if system_prefers_light() {
                "light"
            } else {
                "dark"
            }
        }
    }
}

fn sync_autostart_menu(app: &AppHandle, enabled: bool) {
    let state = app.state::<EngineState>();
    let guard = state.autostart_item.lock().unwrap();
    if let Some(item) = guard.as_ref() {
        let _ = item.set_checked(enabled);
    }
}

// ---------------------------------------------------------------------------
// Commandes exposées à la fenêtre Paramètres
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_profiles(app: AppHandle) -> Vec<Profile> {
    list_conf_files(&app)
}

// Retire de chaque touche toute variante marquée (posée par un chargement
// de langue précédent), puis les touches devenues vides — étapes 1-2 de
// load_language. Ce que l'utilisateur a placé lui-même (non marqué) n'est
// jamais touché, même déplacé depuis sa touche d'origine : la provenance
// voyage avec la variante, pas avec la position (voir Variant).
fn strip_lang_variants(entries: &mut Vec<PersoEntry>) {
    for entry in entries.iter_mut() {
        entry.variants.retain(|v| !v.from_lang);
    }
    entries.retain(|e| !e.variants.is_empty());
}

// Insère les entrées d'un profil source (fraîchement parsé, donc toutes non
// marquées) en les marquant à leur tour, sans dupliquer un caractère déjà
// présent — marqué ou non — sur la même touche : la redondance entre
// touches DIFFÉRENTES reste légitime, mais pas sur une même touche — étapes
// 3-4 de load_language.
fn insert_lang_entries(target: &mut Vec<PersoEntry>, source: Vec<PersoEntry>) {
    for src_entry in source {
        let variants: Vec<Variant> = src_entry
            .variants
            .into_iter()
            .map(|v| Variant { char: v.char, from_lang: true })
            .collect();
        match target.iter_mut().find(|e| e.key == src_entry.key) {
            Some(existing) => {
                for v in variants {
                    if !existing.variants.iter().any(|ev| ev.char == v.char) {
                        existing.variants.push(v);
                    }
                }
            }
            None => target.push(PersoEntry { key: src_entry.key, variants }),
        }
    }
}

// "Charger une langue" (clic sur un profil de la liste, ou AltGr+Espace) :
// ne bascule jamais dessus directement — retire de perso.conf tout ce qui
// vient d'un chargement précédent, insère le profil source à sa place
// (marqué à son tour), puis active perso.conf (qui tourne déjà en
// permanence par ailleurs). Voir strip_lang_variants/insert_lang_entries.
#[tauri::command]
fn load_language(app: AppHandle, path: String) -> Result<(), String> {
    let perso_path = perso_conf_path(&app)?;

    let source = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {path}: {e}"))?;
    let perso = std::fs::read_to_string(&perso_path).unwrap_or_default();

    let source_sections = split_conf_sections(&source);
    let perso_sections = split_conf_sections(&perso);

    let mut min = merge_entries(parse_entries(&perso_sections.min_lines));
    let mut maj = merge_entries(parse_entries(&perso_sections.maj_lines));
    strip_lang_variants(&mut min);
    strip_lang_variants(&mut maj);

    let src_min = merge_entries(parse_entries(&source_sections.min_lines));
    let src_maj = merge_entries(parse_entries(&source_sections.maj_lines));
    insert_lang_entries(&mut min, src_min);
    insert_lang_entries(&mut maj, src_maj);

    // Toujours remplacée par le nom de la langue source qu'on fusionne ici,
    // jamais conservée telle quelle : sinon un perso.conf créé avant cette
    // fonctionnalité (ou tout juste créé, "lang:" absente) garderait pour
    // toujours son premier nom (voire "Personnalisé"), même après avoir
    // chargé une vraie langue par la suite — le `lang:` de perso.conf doit
    // refléter la DERNIÈRE langue chargée, pas la première.
    let source_lang = parse_conf(&path).map(|c| c.language).unwrap_or_else(|_| fallback_language_name(&path));
    let mut out = vec![format!("lang: {source_lang}")];
    out.push("min:".to_string());
    out.extend(entries_to_lines(&min));
    out.push("maj:".to_string());
    out.extend(entries_to_lines(&maj));

    let mut new_content = out.join("\n");
    new_content.push('\n');
    mark_self_write(&app);
    std::fs::write(&perso_path, new_content)
        .map_err(|e| format!("Cannot write {}: {e}", perso_path.display()))?;

    activate_profile(&app, &perso_path.to_string_lossy())?;
    let _ = app.emit("perso-changed", ());
    Ok(())
}

#[tauri::command]
fn get_autostart() -> bool {
    autostart_enabled()
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    set_autostart_registry(enabled)?;
    sync_autostart_menu(&app, enabled);
    Ok(())
}

#[tauri::command]
fn get_theme_preference(app: AppHandle) -> String {
    app.state::<EngineState>().theme_pref.lock().unwrap().clone()
}

#[tauri::command]
fn get_effective_theme(app: AppHandle) -> String {
    let preference = app.state::<EngineState>().theme_pref.lock().unwrap().clone();
    resolve_effective_theme(&preference).to_string()
}

#[tauri::command]
fn set_theme_preference(app: AppHandle, preference: String) -> Result<(), String> {
    write_theme_preference(&preference)?;
    *app.state::<EngineState>().theme_pref.lock().unwrap() = preference.clone();
    let effective = resolve_effective_theme(&preference).to_string();
    let _ = app.emit("theme", effective);
    Ok(())
}

#[tauri::command]
fn get_accent_reminder_enabled() -> bool {
    read_accent_reminder_preference()
}

// Diffusé aux deux fenêtres (comme "theme") : si Settings et le HUD sont
// ouverts en même temps, basculer le réglage doit masquer/montrer la barre
// tout de suite, pas seulement au prochain lancement du HUD.
#[tauri::command]
fn set_accent_reminder_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    write_accent_reminder_preference(enabled)?;
    let _ = app.emit("accent-reminder-changed", enabled);
    Ok(())
}

#[tauri::command]
fn get_language_label_with_accents_enabled() -> bool {
    read_language_label_with_accents_preference()
}

#[tauri::command]
fn set_language_label_with_accents_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    write_language_label_with_accents_preference(enabled)?;
    let _ = app.emit("language-label-with-accents-changed", enabled);
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    shutdown_core(&app);
    app.exit(0);
}

// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Icône selon le thème de la barre des tâches (réglage Windows distinct de
// celui des applications) : logo noir sur barre claire, blanc sur barre sombre.
// ---------------------------------------------------------------------------

const TRAY_ICON_LIGHT_TASKBAR: &[u8] = include_bytes!("../icons/tray-light.png");
const TRAY_ICON_DARK_TASKBAR: &[u8] = include_bytes!("../icons/tray-dark.png");

fn taskbar_is_light() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(PERSONALIZE_KEY)
        .and_then(|k| k.get_value::<u32, _>("SystemUsesLightTheme"))
        .map(|v| v != 0)
        .unwrap_or(false)
}

fn taskbar_icon() -> tauri::image::Image<'static> {
    let bytes = if taskbar_is_light() { TRAY_ICON_LIGHT_TASKBAR } else { TRAY_ICON_DARK_TASKBAR };
    tauri::image::Image::from_bytes(bytes).expect("icône embarquée")
}

// Icône de la zone de notification et du bouton de la fenêtre de
// configuration dans la barre des tâches.
fn apply_taskbar_icon(app: &AppHandle) {
    let icon = taskbar_icon();
    if let Some(tray) = app.tray_by_id("omnikey-tray") {
        let _ = tray.set_icon(Some(icon.clone()));
    }
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.set_icon(icon);
    }
}

// Windows prévient dès qu'une valeur de la clé Personalize change (thème
// clair/sombre) : pas de scrutation, le fil dort entre deux changements.
fn watch_taskbar_theme(app: AppHandle) {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY,
        REG_NOTIFY_CHANGE_LAST_SET,
    };
    std::thread::spawn(move || {
        let path: Vec<u16> = PERSONALIZE_KEY.encode_utf16().chain(std::iter::once(0)).collect();
        let mut key = HKEY::default();
        unsafe {
            if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), 0, KEY_NOTIFY, &mut key).is_err() {
                return;
            }
            let mut light = taskbar_is_light();
            while RegNotifyChangeKeyValue(key, false, REG_NOTIFY_CHANGE_LAST_SET, None, false).is_ok() {
                let now = taskbar_is_light();
                if now != light {
                    light = now;
                    apply_taskbar_icon(&app);
                }
            }
            let _ = RegCloseKey(key);
        }
    });
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let autostart_item = CheckMenuItem::with_id(
        app,
        "autostart",
        "Launch at Windows startup",
        true,
        autostart_enabled(),
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&autostart_item, &quit_item])?;

    let state = app.state::<EngineState>();
    *state.autostart_item.lock().unwrap() = Some(autostart_item.clone());

    // Clic gauche = ouvre directement l'app (plus besoin de passer par le
    // menu pour l'action principale) ; clic droit = menu (autostart/quitter),
    // comportement par défaut de TrayIconBuilder quand show_menu_on_left_click
    // est désactivé.
    TrayIconBuilder::with_id("omnikey-tray")
        .icon(taskbar_icon())
        .tooltip("Baudot Companion App - by Lockfree")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                open_settings(tray.app_handle());
            }
        })
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "autostart" => {
                // muda a déjà basculé la coche : on applique l'état affiché.
                let enabled = autostart_item.is_checked().unwrap_or(false);
                if let Err(e) = set_autostart_registry(enabled) {
                    let _ = autostart_item.set_checked(!enabled);
                    app.dialog()
                        .message(e)
                        .title("Baudot Companion App - Error")
                        .kind(MessageDialogKind::Error)
                        .show(|_| {});
                }
            }
            "quit" => {
                shutdown_core(app);
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

// Ordre de résolution du profil de démarrage :
// 1. Dernier profil actif mémorisé (registre) — tant que le fichier existe
//    toujours à cet emplacement.
// 2. "french.conf" par convention (profil d'origine de l'app).
// 3. "default.conf", au cas où un profil garderait encore cet ancien nom.
// 4. Premier profil trouvé, faute de mieux (ex : tout premier lancement
//    sur une machine où ni le registre ni ces deux noms n'existent).
fn find_default_conf(app: &AppHandle) -> Option<String> {
    if let Some(last) = read_last_profile() {
        if std::path::Path::new(&last).is_file() {
            return Some(last);
        }
    }

    let profiles = list_conf_files(app);
    profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("french.conf"))
        .or_else(|| profiles.iter().find(|p| p.name.eq_ignore_ascii_case("default.conf")))
        .or_else(|| profiles.first())
        .map(|p| p.path.clone())
}

fn main() {
    let launched_by_startup = std::env::args().any(|a| a.starts_with("--startup"));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(EngineState::default())
        .invoke_handler(tauri::generate_handler![
            list_profiles,
            get_autostart,
            set_autostart,
            get_theme_preference,
            get_effective_theme,
            set_theme_preference,
            get_accent_reminder_enabled,
            set_accent_reminder_enabled,
            get_hud_scale,
            set_hud_scale,
            get_language_label_with_accents_enabled,
            set_language_label_with_accents_enabled,
            quit_app,
            load_language,
            read_perso_conf,
            save_perso_conf,
            list_selected_languages,
            save_selected_languages,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // La fenêtre HUD est un overlay pur : jamais de focus, jamais de clic.
            if let Some(hud) = app.get_webview_window("hud") {
                let _ = hud.set_ignore_cursor_events(true);
            }
            apply_hud_scale(app.handle(), read_hud_scale());

            *handle.state::<EngineState>().theme_pref.lock().unwrap() = read_theme_preference();

            setup_tray(&handle)?;
            watch_taskbar_theme(handle.clone());

            // Parité avec ensure_startup_default_on() : au premier lancement
            // manuel, l'autostart est activé par défaut.
            // Un build de développement ne touche jamais au démarrage
            // automatique : sinon `tauri dev` l'enregistre sur l'exe de debug,
            // qui se relance ensuite à chaque démarrage de Windows à la place
            // de l'app installée (et en plus d'elle : frappe en double).
            if !cfg!(debug_assertions) {
                migrate_legacy_autostart();
                if !launched_by_startup && !autostart_enabled() {
                    let _ = set_autostart_registry(true);
                    sync_autostart_menu(&handle, true);
                }
                repair_autostart_path();
            }

            let Some(conf) = find_default_conf(&handle) else {
                handle
                    .dialog()
                    .message("No .conf file found next to the application.")
                    .title("Baudot Companion App - Error")
                    .kind(MessageDialogKind::Error)
                    .blocking_show();
                handle.exit(1);
                return Ok(());
            };

            if let Err(e) = activate_profile(&handle, &conf) {
                handle
                    .dialog()
                    .message(e)
                    .title("Baudot Companion App - Error")
                    .kind(MessageDialogKind::Error)
                    .blocking_show();
                handle.exit(1);
                return Ok(());
            }

            // v0.2 : attend un clavier Var en permanence. Dès sa première
            // trame, il remplace le moteur v0.1 (voir VarSink).
            let var_app = handle.clone();
            std::thread::spawn(move || omnikey_hid::run(&mut VarSink { app: var_app }));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("échec du démarrage de Tauri")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                shutdown_core(app);
            }
        });
}

