import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AccentHUD, DOTTED_CIRCLE, isCombiningMark, type HudState, type ReminderKey } from "./AccentHUD";
import { KEYBOARD_ROWS, shiftedLabel } from "./keyboardLayout";

// Miroir minimal de PersoConfData/PersoEntry/Variant (src-tauri/main.rs) —
// le HUD n'a besoin que de lire, jamais d'éditer : pas de raison de
// dépendre des types plus riches de KeyboardEditor.tsx.
type Variant = { char: string; from_lang: boolean };
type PersoEntry = { key: string; variants: Variant[] };
type PersoConfData = { lang: string; min: PersoEntry[]; maj: PersoEntry[] };

// Les .conf stockent déjà une marque isolée préfixée du cercle pointillé
// ("◌̄", 2 points de code — voir strip_leading_dotted_circle côté Rust) : le
// premier point de code n'est alors PAS combinant (c'est ◌ lui-même), donc
// isCombiningMark(v.char) tel quel renvoie false et ratait ces entrées.
// On retire un éventuel préfixe déjà présent avant de tester — gère aussi
// bien "◌̄" (chargé d'un .conf) que "̄" nu (déposé à la main via la banque).
function markPortion(v: string): string | null {
  const bare = v.startsWith(DOTTED_CIRCLE) ? v.slice(DOTTED_CIRCLE.length) : v;
  return isCombiningMark(bare) ? bare : null;
}

// Barre de rappel des diacritiques : une entrée par touche (min ET maj
// comptent séparément, voir Brief) qui porte au moins une variante
// combinante — le "1" de min: et le "!" que la même touche physique
// représente sous Maj (via shiftedLabel, même correspondance que
// KeyboardEditor.tsx) apparaissent donc comme deux rappels distincts.
// Ordre physique des touches (gauche à droite, haut en bas) : le rappel suit
// le clavier, pas l'ordre des lignes de perso.conf, qui change dès qu'on
// déplace une variante dans l'éditeur.
const KEY_ORDER = new Map(KEYBOARD_ROWS.flatMap((row) => row.keys).map((k, i) => [k.key, i]));
const keyRank = (key: string) => KEY_ORDER.get(key) ?? Number.MAX_SAFE_INTEGER;

function buildReminderKeys(data: PersoConfData): ReminderKey[] {
  const out: ReminderKey[] = [];
  const collect = (entries: PersoEntry[], section: "min" | "maj") => {
    for (const e of [...entries].sort((a, b) => keyRank(a.key) - keyRank(b.key))) {
      const marks = e.variants.map((v) => markPortion(v.char)).filter((m): m is string => m !== null);
      if (marks.length === 0) continue;
      out.push({
        id: `${section}-${e.key}`,
        label: section === "maj" ? shiftedLabel(e.key) : e.key,
        glyph: DOTTED_CIRCLE + marks.join(""),
      });
    }
  };
  collect(data.min, "min");
  collect(data.maj, "maj");
  return out;
}

// Événements émis par le backend Rust, relais direct de l'IPC stdout du
// moteur lockfree_core (HELD / SHOW:vk / CYCLE:idx / HOLD:vk:idx / HIDE,
// plus LANGSHOW / LANGCYCLE:idx / LANGCOMMIT:idx / LANGCANCEL pour le
// sélecteur de langue AltGr+Espace — voir handle_ipc côté Rust).
const COMMIT_DISSOLVE_MS = 180;

type HudEvent =
  | { kind: "held"; language: string }
  | { kind: "show"; variants: string[]; marks: string[] }
  | { kind: "cycle"; index: number }
  | { kind: "hide" }
  | { kind: "hold"; marks: string[] }
  | { kind: "langshow"; languages: string[]; index: number }
  | { kind: "langcycle"; index: number }
  | { kind: "langcommit"; index: number }
  // Reflet brut de l'état physique d'AltGr — indépendant de `state.kind`
  // (voir process_key(), main.rs racine) : sert uniquement à masquer le
  // rappel diacritiques en mode Dead Keys (state.kind === "armed") une fois
  // AltGr relâché, sans toucher au HUD principal (qui reste affiché).
  | { kind: "altstate"; held: boolean };

export function HudApp() {
  const [state, setState] = useState<HudState>(null);
  const [reminderKeys, setReminderKeys] = useState<ReminderKey[]>([]);
  const [reminderEnabled, setReminderEnabled] = useState(true);
  const [languageLabelWithAccents, setLanguageLabelWithAccents] = useState(true);
  const [altHeld, setAltHeld] = useState(true);
  const dissolveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Rechargé à chaque changement de .conf (perso-changed, même évènement que
  // KeyboardEditor.tsx) — la barre de rappel doit rester en phase avec la
  // config active sans jamais redémarrer la fenêtre HUD.
  useEffect(() => {
    const load = () => invoke<PersoConfData>("read_perso_conf").then((d) => setReminderKeys(buildReminderKeys(d)));
    load();
    const unlisten = listen("perso-changed", load);
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    invoke<boolean>("get_accent_reminder_enabled").then(setReminderEnabled);
    const unlisten = listen<boolean>("accent-reminder-changed", (e) => setReminderEnabled(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    invoke<boolean>("get_language_label_with_accents_enabled").then(setLanguageLabelWithAccents);
    const unlisten = listen<boolean>("language-label-with-accents-changed", (e) =>
      setLanguageLabelWithAccents(e.payload),
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<HudEvent>("hud", (e) => {
      if (dissolveTimer.current !== null) {
        clearTimeout(dissolveTimer.current);
        dissolveTimer.current = null;
      }
      const ev = e.payload;
      switch (ev.kind) {
        case "held":
          setState({ kind: "waiting", language: ev.language });
          break;
        case "show":
          setState({ kind: "variants", variants: ev.variants, index: 0, marks: ev.marks });
          break;
        case "cycle":
          setState((s) =>
            s && s.kind === "variants" ? { ...s, index: ev.index } : s,
          );
          break;
        case "hold":
          setState({ kind: "armed", marks: ev.marks });
          break;
        case "langshow":
          setState({ kind: "language", languages: ev.languages, index: ev.index, committing: false });
          break;
        case "langcycle":
          setState((s) =>
            s && s.kind === "language" ? { ...s, index: ev.index } : s,
          );
          break;
        case "langcommit":
          setState((s) =>
            s && s.kind === "language" ? { ...s, index: ev.index, committing: true } : s,
          );
          dissolveTimer.current = setTimeout(() => {
            setState(null);
            dissolveTimer.current = null;
          }, COMMIT_DISSOLVE_MS);
          break;
        case "hide":
          setState(null);
          break;
        case "altstate":
          setAltHeld(ev.held);
          break;
      }
    });
    return () => {
      if (dissolveTimer.current !== null) {
        clearTimeout(dissolveTimer.current);
      }
      unlisten.then((f) => f());
    };
  }, []);

  // "armed" (Dead Keys) est le seul état qui peut survivre au relâchement
  // d'AltGr (voir process_key(), main.rs racine — le HUD principal reste
  // affiché exprès) : c'est là, et seulement là, que le rappel doit
  // disparaître une fois AltGr relâché. Les autres états (waiting/variants)
  // n'existent de toute façon jamais sans AltGr tenu.
  const showReminder = reminderEnabled && !(state?.kind === "armed" && !altHeld);

  return (
    <AccentHUD
      state={state}
      reminder={showReminder ? reminderKeys : []}
      languageLabelWithAccents={languageLabelWithAccents}
    />
  );
}
