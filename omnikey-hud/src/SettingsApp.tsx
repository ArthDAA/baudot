import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { KeyboardEditor } from "./KeyboardEditor";
import { CharacterBank } from "./CharacterBank";
import { LanguageBank } from "./LanguageBank";
import { LockfreeLogo } from "./LockfreeLogo";

type ThemePreference = "system" | "light" | "dark";

const THEME_LABELS: Record<ThemePreference, string> = {
  system: "System",
  light: "Light",
  dark: "Dark",
};

// Une seule page, comme l'ancien affichage — le clavier visuel prend juste
// la place qu'occupait la liste des langues (juste sous le header), qui
// redescend en section compacte plus bas plutôt que de vivre dans un onglet
// séparé et une fenêtre deux fois plus large.
export function SettingsApp() {
  const [autostart, setAutostart] = useState(false);
  const [theme, setTheme] = useState<ThemePreference>("system");
  const [accentReminder, setAccentReminder] = useState(true);
  const [languageLabelWithAccents, setLanguageLabelWithAccents] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    setAutostart(await invoke<boolean>("get_autostart"));
    setTheme(await invoke<ThemePreference>("get_theme_preference"));
    setAccentReminder(await invoke<boolean>("get_accent_reminder_enabled"));
    setLanguageLabelWithAccents(await invoke<boolean>("get_language_label_with_accents_enabled"));
  };

  useEffect(() => {
    refresh().catch((e) => setError(String(e)));
  }, []);

  const toggleAutostart = async () => {
    const next = !autostart;
    try {
      await invoke("set_autostart", { enabled: next });
      setAutostart(next);
    } catch (e) {
      setError(String(e));
    }
  };

  const changeTheme = async (preference: ThemePreference) => {
    try {
      await invoke("set_theme_preference", { preference });
      setTheme(preference);
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleAccentReminder = async () => {
    const next = !accentReminder;
    try {
      await invoke("set_accent_reminder_enabled", { enabled: next });
      setAccentReminder(next);
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleLanguageLabelWithAccents = async () => {
    const next = !languageLabelWithAccents;
    try {
      await invoke("set_language_label_with_accents_enabled", { enabled: next });
      setLanguageLabelWithAccents(next);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="settings">
      <header className="settings-header">
        <h1>Baudot Companion App</h1>
      </header>

      {error && <div className="settings-error">{error}</div>}

      <KeyboardEditor />

      <CharacterBank />

      <LanguageBank />

      <div className="settings-row">
        <p className="settings-label">Appearance</p>
        <div className="theme-switch">
          {(Object.keys(THEME_LABELS) as ThemePreference[]).map((pref) => (
            <button
              key={pref}
              className={"theme-option" + (theme === pref ? " theme-option-active" : "")}
              onClick={() => changeTheme(pref)}
            >
              {THEME_LABELS[pref]}
            </button>
          ))}
        </div>
      </div>

      <div className="settings-row">
        <label className="switch-label">
          <input type="checkbox" checked={autostart} onChange={toggleAutostart} />
          <span>Launch at Windows startup</span>
        </label>
      </div>

      <div className="settings-row">
        <label className="switch-label">
          <input type="checkbox" checked={accentReminder} onChange={toggleAccentReminder} />
          <span>Show diacritics reminder in the HUD</span>
        </label>
        <label className="switch-label switch-label-sub">
          <input
            type="checkbox"
            checked={languageLabelWithAccents}
            onChange={toggleLanguageLabelWithAccents}
            disabled={!accentReminder}
          />
          <span>Also show the language name while the reminder is visible</span>
        </label>
      </div>

      <div className="settings-actions">
        <button className="btn-quit" onClick={() => invoke("quit_app")}>
          Quit Baudot Companion App
        </button>
      </div>

      <footer className="settings-footer">
        by <LockfreeLogo className="settings-footer-logo" />
      </footer>
    </div>
  );
}
