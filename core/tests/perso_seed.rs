//! Le modèle perso.conf livré, une fois converti au premier lancement.

use omnikey_core::conf::{migrate_zone_format, read_perso};

#[test]
fn bundled_perso_seed_keeps_user_keys_first_and_marks_language_accents() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../omnikey-hud/src-tauri/binaries/perso.conf");
    let seed = std::fs::read_to_string(path).unwrap();
    let migrated = migrate_zone_format(&seed).expect("le modèle est à l'ancien format");
    let data = read_perso(&migrated, path);

    let variants = |section: &[omnikey_core::conf::PersoEntry], key: &str| {
        section
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.variants.iter().map(|v| (v.char.clone(), v.from_lang)).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    // Accents de la langue : marqués, donc remplacés au prochain chargement de langue.
    assert_eq!(variants(&data.min, "1"), vec![("\u{25cc}\u{301}".to_string(), true)]);
    // Touche de l'utilisateur : € reste en tête, non marqué.
    assert_eq!(variants(&data.maj, "4")[0], ("€".to_string(), false));
}
