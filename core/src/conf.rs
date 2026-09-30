//! ConfStore (D2 B5) : seul lecteur et écrivain des profils `.conf`.
//!
//! Remplace les deux parseurs de v0.1 (`LayoutConfig::load_from_file` du
//! moteur et `parse_conf`/`parse_entries`... du backend Tauri) par un seul
//! découpage de lignes ([`scan`]), projeté ensuite selon l'usage :
//! - [`parse_layout`] : ce que la machine à états utilise (un caractère par
//!   variante), mêmes règles que le moteur v0.1 ;
//! - [`parse_display`] : ce que le HUD affiche (la variante complète, sans
//!   marqueur `*` ni cercle pointillé), mêmes règles que `parse_conf` v0.1 ;
//! - [`read_perso`] / [`render_perso`] / [`merge_language`] : l'édition de
//!   `perso.conf` (provenance des variantes, fusion d'une langue).
//!
//! Format v1 (le seul existant, reconnu faute d'en-tête de version) :
//! ```text
//! lang: Français
//! min:
//! e = é, è, ê, ë
//! maj:
//! e = É, È, Ê, Ë*
//! ```
//! `*` en fin de variante : insérée par un chargement de langue. `◌` en tête :
//! confort d'écriture pour une marque combinante isolée.

use std::collections::HashMap;
use std::path::Path;

use crate::key::{conf_key_to_logical, LogicalKey};
use crate::machine::{CharUnits, Layout};

const BOM: char = '\u{FEFF}';
const DOTTED_CIRCLE: char = '\u{25CC}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfError {
    /// Fichier illisible (code de sortie 1 du moteur v0.1).
    Io(String),
    /// Ni section `min:`/`maj:` (code de sortie 2 du moteur v0.1).
    NoSection,
    /// Ni `lang:` ni section : fichier vide ou invalide (règle du backend v0.1).
    Empty,
}

impl std::fmt::Display for ConfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfError::Io(e) => write!(f, "{e}"),
            ConfError::NoSection => write!(f, ".conf file without a min: or maj: section."),
            ConfError::Empty => write!(f, "Empty or invalid .conf file."),
        }
    }
}

// ---------------------------------------------------------------------------
// Découpage unique
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Min,
    Maj,
}

/// Une ligne `clé = v1, v2, ...` rattachée à sa section.
#[derive(Debug, Clone)]
pub struct KeyLine {
    pub section: Section,
    pub key: char,
    /// Variantes brutes, déjà rognées, telles qu'écrites dans le fichier.
    pub raw_variants: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Scan {
    /// Valeur de la dernière ligne `lang:` non vide.
    pub language: Option<String>,
    pub saw_section: bool,
    pub lines: Vec<KeyLine>,
}

/// Découpe un `.conf` en lignes de touches. Les lignes vides, `lang:`, et
/// les lignes avant toute section sont écartées ici, comme les deux parseurs
/// de v0.1 le faisaient.
pub fn scan(content: &str) -> Scan {
    let mut out = Scan::default();
    let mut section: Option<Section> = None;
    for line in content.lines() {
        let line = line.trim_start_matches(BOM).trim();
        if line.is_empty() {
            continue;
        }
        if line == "min:" {
            section = Some(Section::Min);
            out.saw_section = true;
            continue;
        }
        if line == "maj:" {
            section = Some(Section::Maj);
            out.saw_section = true;
            continue;
        }
        if let Some(name) = line.strip_prefix("lang:") {
            let name = name.trim();
            if !name.is_empty() {
                out.language = Some(name.to_string());
            }
            continue;
        }
        let Some((key, values)) = line.split_once('=') else { continue };
        let Some(key) = key.trim().chars().next() else { continue };
        let Some(section) = section else { continue };
        out.lines.push(KeyLine {
            section,
            key,
            raw_variants: values.split(',').map(|v| v.trim().to_string()).collect(),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Projection moteur : un caractère par variante
// ---------------------------------------------------------------------------

/// Profil vu par la machine à états. Mêmes règles que le moteur v0.1 : seul
/// le premier caractère de chaque variante compte, un `◌` initial est sauté,
/// une ligne sans variante n'est pas insérée.
pub fn parse_layout(content: &str) -> Result<Layout, ConfError> {
    let scan = scan(content);
    if !scan.saw_section {
        return Err(ConfError::NoSection);
    }
    let mut layout = Layout::default();
    for line in &scan.lines {
        let variants: Vec<CharUnits> = line.raw_variants.iter().filter_map(|v| engine_variant(v)).collect();
        if variants.is_empty() {
            continue;
        }
        let map = match line.section {
            Section::Min => &mut layout.min_map,
            Section::Maj => &mut layout.maj_map,
        };
        map.insert(conf_key_to_logical(line.key), variants);
    }
    Ok(layout)
}

fn engine_variant(raw: &str) -> Option<CharUnits> {
    let mut chars = raw.chars();
    let first = chars.next();
    let c = match first {
        Some(DOTTED_CIRCLE) => chars.next().or(first),
        other => other,
    }?;
    let mut b = [0u16; 2];
    let utf16 = c.encode_utf16(&mut b);
    let mut units: CharUnits = [utf16[0], 0];
    if utf16.len() > 1 {
        units[1] = utf16[1];
    }
    Some(units)
}

pub fn load_layout_file(path: &Path) -> Result<Layout, ConfError> {
    let content = std::fs::read_to_string(path).map_err(|e| ConfError::Io(format!("Cannot read {}: {e}", path.display())))?;
    parse_layout(&content)
}

// ---------------------------------------------------------------------------
// Projection affichage : variante complète
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayConf {
    pub min_map: HashMap<LogicalKey, Vec<String>>,
    pub maj_map: HashMap<LogicalKey, Vec<String>>,
    pub language: String,
}

/// Profil vu par le HUD. Mêmes règles que `parse_conf` du backend v0.1.
/// `path` ne sert qu'à nommer la langue quand `lang:` est absente.
pub fn parse_display(content: &str, path: &str) -> Result<DisplayConf, ConfError> {
    let scan = scan(content);
    if scan.language.is_none() && !scan.saw_section {
        return Err(ConfError::Empty);
    }
    let mut out = DisplayConf::default();
    for line in &scan.lines {
        let variants: Vec<String> = line
            .raw_variants
            .iter()
            .map(|v| strip_leading_dotted_circle(strip_lang_marker(v).0.trim()))
            .filter(|v| !v.is_empty())
            .collect();
        if variants.is_empty() {
            continue;
        }
        let map = match line.section {
            Section::Min => &mut out.min_map,
            Section::Maj => &mut out.maj_map,
        };
        map.insert(conf_key_to_logical(line.key), variants);
    }
    out.language = scan.language.unwrap_or_else(|| fallback_language_name(path));
    Ok(out)
}

fn strip_leading_dotted_circle(v: &str) -> String {
    match v.strip_prefix(DOTTED_CIRCLE) {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => v.to_string(),
    }
}

/// Retire un marqueur de provenance `*` en fin de variante.
pub fn strip_lang_marker(v: &str) -> (String, bool) {
    match v.strip_suffix('*') {
        Some(rest) => (rest.to_string(), true),
        None => (v.to_string(), false),
    }
}

/// Nom lisible tiré du nom de fichier : `haitian_creole.conf` -> "Haitian Creole".
pub fn fallback_language_name(path: &str) -> String {
    let stem = Path::new(path)
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
// Édition de perso.conf
// ---------------------------------------------------------------------------

/// Provenance posée une seule fois à l'insertion et transportée avec la
/// variante : `from_lang` = insérée par un chargement de langue, donc
/// remplacée au prochain. Une variante posée ou déplacée par l'utilisateur
/// garde son marqueur tel quel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub char: String,
    pub from_lang: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersoEntry {
    pub key: String,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersoConfData {
    pub lang: String,
    pub min: Vec<PersoEntry>,
    pub maj: Vec<PersoEntry>,
}

/// Découpe brute : préambule (tout avant `min:`), puis lignes de chaque
/// section, sans rien interpréter.
pub struct ConfSections {
    pub preamble: Vec<String>,
    pub min_lines: Vec<String>,
    pub maj_lines: Vec<String>,
}

pub fn split_sections(content: &str) -> ConfSections {
    let mut s = ConfSections { preamble: Vec::new(), min_lines: Vec::new(), maj_lines: Vec::new() };
    let mut section = 0u8; // 0 = préambule, 1 = min, 2 = maj
    for line in content.lines() {
        match line.trim_start_matches(BOM).trim() {
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
            0 => s.preamble.push(line.to_string()),
            1 => s.min_lines.push(line.to_string()),
            _ => s.maj_lines.push(line.to_string()),
        }
    }
    s
}

fn preamble_language(preamble: &[String]) -> Option<String> {
    preamble
        .iter()
        .find_map(|l| l.trim_start_matches(BOM).trim().strip_prefix("lang:"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn normalize_key(raw: &str) -> Option<String> {
    let c = raw.trim().chars().next()?;
    Some(c.to_ascii_lowercase().to_string())
}

/// Lignes `clé = v1, v2*` d'une section -> entrées. Variantes gardées telles
/// qu'écrites (cercle pointillé compris), sauf le marqueur `*` porté par
/// [`Variant::from_lang`]. Un doublon sur la même ligne est écarté.
pub fn parse_entries(lines: &[String]) -> Vec<PersoEntry> {
    let mut out = Vec::new();
    for line in lines {
        let trimmed = line.trim_start_matches(BOM).trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, values)) = trimmed.split_once('=') else { continue };
        let Some(key) = normalize_key(key) else { continue };
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
    out
}

fn union_into(existing: &mut PersoEntry, variants: Vec<Variant>) {
    for v in variants {
        if !existing.variants.iter().any(|ev| ev.char == v.char) {
            existing.variants.push(v);
        }
    }
}

/// Fusionne les doublons de clé en unissant leurs variantes.
pub fn merge_entries(entries: Vec<PersoEntry>) -> Vec<PersoEntry> {
    let mut out: Vec<PersoEntry> = Vec::new();
    for entry in entries {
        match out.iter_mut().find(|e| e.key == entry.key) {
            Some(existing) => union_into(existing, entry.variants),
            None => out.push(entry),
        }
    }
    out
}

/// Entrées -> lignes. Une entrée vide n'est jamais écrite (le moteur ferait
/// un modulo par zéro sur une touche mappée sans variante).
pub fn entries_to_lines(entries: &[PersoEntry]) -> Vec<String> {
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

/// Retire tout ce qu'un chargement de langue précédent avait inséré.
pub fn strip_lang_variants(entries: &mut Vec<PersoEntry>) {
    for entry in entries.iter_mut() {
        entry.variants.retain(|v| !v.from_lang);
    }
    entries.retain(|e| !e.variants.is_empty());
}

/// Insère les entrées d'une langue en les marquant, sans dupliquer un
/// caractère déjà présent sur la même touche.
pub fn insert_lang_entries(target: &mut Vec<PersoEntry>, source: Vec<PersoEntry>) {
    for src in source {
        let variants: Vec<Variant> = src.variants.into_iter().map(|v| Variant { char: v.char, from_lang: true }).collect();
        match target.iter_mut().find(|e| e.key == src.key) {
            Some(existing) => union_into(existing, variants),
            None => target.push(PersoEntry { key: src.key, variants }),
        }
    }
}

/// Lecture structurée de `perso.conf` pour l'éditeur.
pub fn read_perso(content: &str, path: &str) -> PersoConfData {
    let s = split_sections(content);
    PersoConfData {
        lang: preamble_language(&s.preamble).unwrap_or_else(|| fallback_language_name(path)),
        min: merge_entries(parse_entries(&s.min_lines)),
        maj: merge_entries(parse_entries(&s.maj_lines)),
    }
}

/// Écriture de `perso.conf` depuis l'éditeur.
pub fn render_perso(data: &PersoConfData) -> String {
    let mut out = vec![format!("lang: {}", data.lang), "min:".to_string()];
    out.extend(entries_to_lines(&merge_entries(data.min.clone())));
    out.push("maj:".to_string());
    out.extend(entries_to_lines(&merge_entries(data.maj.clone())));
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// "Charger une langue" : retire de `perso.conf` ce qui vient du chargement
/// précédent, insère la langue source (marquée), et nomme le profil d'après
/// elle. Les variantes de l'utilisateur ne sont jamais touchées. Idempotent.
pub fn merge_language(perso: &str, source: &str, source_path: &str) -> String {
    let src = split_sections(source);
    let per = split_sections(perso);

    let mut min = merge_entries(parse_entries(&per.min_lines));
    let mut maj = merge_entries(parse_entries(&per.maj_lines));
    strip_lang_variants(&mut min);
    strip_lang_variants(&mut maj);
    insert_lang_entries(&mut min, merge_entries(parse_entries(&src.min_lines)));
    insert_lang_entries(&mut maj, merge_entries(parse_entries(&src.maj_lines)));

    let lang = parse_display(source, source_path)
        .map(|c| c.language)
        .unwrap_or_else(|_| fallback_language_name(source_path));
    let mut out = vec![format!("lang: {lang}"), "min:".to_string()];
    out.extend(entries_to_lines(&min));
    out.push("maj:".to_string());
    out.extend(entries_to_lines(&maj));
    let mut s = out.join("\n");
    s.push('\n');
    s
}

// ---------------------------------------------------------------------------
// Migration ponctuelle de l'ancien format à zones
// ---------------------------------------------------------------------------

// Ancien format : dans chaque section, les lignes avant la première ligne
// vide venaient de la langue, celles après étaient à l'utilisateur.
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
            Some(existing) => union_into(existing, e.variants),
            None => entries.push(e),
        }
    }
    entries
}

/// `Some(nouveau contenu)` si une migration a eu lieu ; `None` si le fichier
/// porte déjà un marqueur `*` (nouveau format) ou n'a aucune section.
pub fn migrate_zone_format(content: &str) -> Option<String> {
    if content.is_empty() || content.contains('*') {
        return None;
    }
    let s = split_sections(content);
    if s.min_lines.is_empty() && s.maj_lines.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    if let Some(lang) = preamble_language(&s.preamble) {
        out.push(format!("lang: {lang}"));
    }
    out.push("min:".to_string());
    out.extend(entries_to_lines(&migration_section(&s.min_lines)));
    out.push("maj:".to_string());
    out.extend(entries_to_lines(&migration_section(&s.maj_lines)));
    let mut new_content = out.join("\n");
    new_content.push('\n');
    Some(new_content)
}

// ---------------------------------------------------------------------------
// Fichiers
// ---------------------------------------------------------------------------

/// Écriture atomique : fichier temporaire voisin puis renommage, pour qu'un
/// lecteur (surveillance à chaud, moteur) ne voie jamais un fichier à moitié
/// écrit.
pub fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::logical;

    const FR: &str = "\u{FEFF}lang: Français\nmin:\ne = é, è, ê, ë\na = à, â, æ\n\nmaj:\ne = É, È, Ê, Ë\n";

    #[test]
    fn layout_takes_first_char_and_skips_dotted_circle() {
        let l = parse_layout("min:\n1 = ◌̄, ◌́\ne = é*, èx\n; = §\n↑ = ↑\n").unwrap();
        assert_eq!(l.min_map[&0x31], vec![[0x0304, 0], [0x0301, 0]]);
        assert_eq!(l.min_map[&0x45], vec![[0xE9, 0], [0xE8, 0]]);
        assert_eq!(l.min_map[&logical::OEM_1], vec![[0xA7, 0]]);
        assert_eq!(l.min_map[&logical::UP], vec![[0x2191, 0]]);
    }

    #[test]
    fn layout_keeps_surrogate_pairs() {
        let l = parse_layout("min:\nx = 𝔸\n").unwrap();
        assert_eq!(l.min_map[&0x58], vec![[0xD835, 0xDD38]]);
    }

    #[test]
    fn layout_errors_match_engine_exit_codes() {
        assert_eq!(parse_layout("lang: Vide\n"), Err(ConfError::NoSection));
        assert!(parse_layout("lang: English\nmin:\n").unwrap().min_map.is_empty());
        // Ligne sans variante : non insérée (sinon modulo par zéro).
        assert!(parse_layout("min:\nx =\ny = ,\n").unwrap().min_map.is_empty());
    }

    #[test]
    fn display_strips_marker_and_circle() {
        let d = parse_display("lang: Test\nmin:\n1 = ◌̄*, ◌, ab*\n", "x.conf").unwrap();
        assert_eq!(d.min_map[&0x31], vec!["\u{0304}", "◌", "ab"]);
        assert_eq!(d.language, "Test");
        assert_eq!(parse_display("min:\n", "haitian_creole.conf").unwrap().language, "Haitian Creole");
        assert_eq!(parse_display("\n\n", "x.conf"), Err(ConfError::Empty));
    }

    #[test]
    fn merge_language_is_idempotent_and_keeps_user_variants() {
        let perso = "lang: Perso\nmin:\ne = é*, €\n4 = ₿\nmaj:\n";
        let once = merge_language(perso, FR, "french.conf");
        let twice = merge_language(&once, FR, "french.conf");
        assert_eq!(once, twice);
        let data = read_perso(&once, "perso.conf");
        assert_eq!(data.lang, "Français");
        let e = data.min.iter().find(|e| e.key == "e").unwrap();
        assert!(e.variants.contains(&Variant { char: "€".into(), from_lang: false }));
        assert!(e.variants.contains(&Variant { char: "è".into(), from_lang: true }));
        assert!(data.min.iter().any(|e| e.key == "4"));
    }

    #[test]
    fn changing_language_replaces_only_marked_variants() {
        let with_fr = merge_language("lang: Perso\nmin:\ne = €\nmaj:\n", FR, "french.conf");
        let de = "lang: Deutsch\nmin:\na = ä\nmaj:\n";
        let data = read_perso(&merge_language(&with_fr, de, "german.conf"), "perso.conf");
        assert_eq!(data.lang, "Deutsch");
        let e = data.min.iter().find(|e| e.key == "e").unwrap();
        assert_eq!(e.variants, vec![Variant { char: "€".into(), from_lang: false }]);
        let a = data.min.iter().find(|e| e.key == "a").unwrap();
        assert_eq!(a.variants, vec![Variant { char: "ä".into(), from_lang: true }]);
    }

    #[test]
    fn render_then_read_round_trips() {
        let data = read_perso("lang: P\nmin:\ne = é*, €\ne = x\nmaj:\nE = É\n", "p.conf");
        assert_eq!(data.min.len(), 1);
        assert_eq!(read_perso(&render_perso(&data), "p.conf"), data);
    }

    #[test]
    fn zone_migration_marks_language_zone_only() {
        let old = "lang: Perso\nmin:\ne = é\n\n4 = €\nmaj:\n";
        let new = migrate_zone_format(old).unwrap();
        assert_eq!(new, "lang: Perso\nmin:\ne = é*\n4 = €\nmaj:\n");
        assert_eq!(migrate_zone_format(&new), None);
    }
}
