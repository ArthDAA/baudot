// Disposition physique QWERTY ANSI — uniquement les touches réellement
// adressables par le moteur (voir key_char_to_vk() dans main.rs et
// src-tauri/main.rs) : A-Z, 0-9, les 11 touches OEM et les 4 flèches.
// Tout le reste (F1-F12, pavé numérique) n'existe pas dans le moteur — mais
// Tab/Verr.Maj/Maj/Entrée/Retour/Espace/Ctrl/Alt sont quand même dessinées
// (voir `ghost` ci-dessous) : purement décoratives, jamais adressables par
// le logiciel, juste là pour que le clavier visuel ressemble à un vrai
// clavier plutôt qu'à une grille de touches flottantes.
export type KeyDef = {
  key: string;
  label?: string;
  units?: number;
  ghost?: boolean;
  // Glyphe isolé (⌫ ↵ ⇧) plutôt qu'un mot (Tab, Ctrl, Alt) : agrandi en
  // CSS (.kbd-key-ghost-icon), sinon perdu dans une touche large.
  ghostIcon?: boolean;
  // Verr. Maj. : pas décorative comme les autres touches de bordure — une
  // vraie bascule min/maj avec LED (KeyboardEditor.tsx, renderToggleKey), le
  // "vrai bouton" à la place de l'ancien toggle min/maj hors-clavier.
  toggle?: boolean;
};

// Volontairement compact : la fenêtre reprend la largeur étroite de l'ancien
// affichage (liste des langues, ~420-460px) plutôt que de s'étaler — la
// rangée la plus large (QWERTYUIOP + [ ] \, décalée par la largeur de Tab)
// doit tenir dedans sans être coupée par overflow:hidden.
export const UNIT_PX = 28;
// Écart interne à une touche multi-unités (ex. Tab, Entrée) : la moitié de
// l'écart entre deux touches distinctes, pour qu'une touche de 1.5u ait
// l'air de "manger" une demi-touche + son écart plutôt que de flotter.
export const GAP_PX = 4;
// Écart flex réel entre deux touches d'une même rangée — DOIT rester
// synchronisé avec .kbd-row{gap} dans styles.css (deux valeurs séparées :
// GAP_PX ci-dessus ne concerne que l'intérieur d'une touche large).
export const ROW_GAP_PX = 6;

export function keyWidth(units = 1): number {
  return units * UNIT_PX + (units - 1) * GAP_PX;
}

function contentWidth(keys: { units?: number }[]): number {
  return keys.reduce((sum, k) => sum + keyWidth(k.units), 0);
}

function gapsWidth(itemCount: number): number {
  return Math.max(0, itemCount - 1) * ROW_GAP_PX;
}

// Correspondance Maj standard QWERTY US — sert uniquement à l'affichage du
// libellé du bouton (KeyboardEditor.tsx, section "maj"), pas à la logique
// de touche : le moteur adresse toujours la même touche physique via
// key_char_to_vk() quelle que soit la casse affichée ici.
const SHIFT_MAP: Record<string, string> = {
  "`": "~",
  "1": "!",
  "2": "@",
  "3": "#",
  "4": "$",
  "5": "%",
  "6": "^",
  "7": "&",
  "8": "*",
  "9": "(",
  "0": ")",
  "-": "_",
  "=": "+",
  "[": "{",
  "]": "}",
  "\\": "|",
  ";": ":",
  "'": '"',
  ",": "<",
  ".": ">",
  "/": "?",
};

export function shiftedLabel(key: string): string {
  if (/^[a-z]$/.test(key)) return key.toUpperCase();
  return SHIFT_MAP[key] ?? key;
}

const ROW0_REAL: KeyDef[] = [
  { key: "`" },
  { key: "1" }, { key: "2" }, { key: "3" }, { key: "4" }, { key: "5" },
  { key: "6" }, { key: "7" }, { key: "8" }, { key: "9" }, { key: "0" },
  { key: "-" }, { key: "=" },
];

const ROW1_REAL: KeyDef[] = [
  { key: "q" }, { key: "w" }, { key: "e" }, { key: "r" }, { key: "t" },
  { key: "y" }, { key: "u" }, { key: "i" }, { key: "o" }, { key: "p" },
  { key: "[" }, { key: "]" }, { key: "\\", units: 1.5 },
];

const ROW2_REAL: KeyDef[] = [
  { key: "a" }, { key: "s" }, { key: "d" }, { key: "f" }, { key: "g" },
  { key: "h" }, { key: "j" }, { key: "k" }, { key: "l" },
  { key: ";" }, { key: "'" },
  // ↑ seule au-dessus, alignée à peu près sur ↓ de la rangée du dessous —
  // vrai T inversé, pas ↓→↑ groupées ici et ← livrée seule en dessous.
  { key: "↑" },
];

const ROW3_REAL: KeyDef[] = [
  { key: "z" }, { key: "x" }, { key: "c" }, { key: "v" }, { key: "b" },
  { key: "n" }, { key: "m" }, { key: "," }, { key: "." }, { key: "/" },
  { key: "←" }, { key: "↓" }, { key: "→" },
];

const TAB: KeyDef = { key: "tab", label: "Tab", units: 1.5, ghost: true };
// Verr. Maj., pas Maj. : c'est elle qui bascule (comme la vraie touche, avec
// sa LED) — Maj. reste décorative (SHIFT_L plus bas), elle ne fait que
// s'appuyer sans jamais rester enfoncée sur un vrai clavier.
const CAPSLOCK: KeyDef = { key: "capslock", units: 1.75, toggle: true };
const CTRL_L: KeyDef = { key: "ctrlL", label: "Ctrl", units: 1.25, ghost: true };
const ALT_L: KeyDef = { key: "altL", label: "Alt", units: 1.25, ghost: true };
const ALT_R: KeyDef = { key: "altR", label: "Alt", units: 1.25, ghost: true };
const CTRL_R: KeyDef = { key: "ctrlR", label: "Ctrl", units: 1.25, ghost: true };

// Largeur de référence : Tab + la rangée QWERTY (13 touches réelles), seule
// rangée où aucune largeur n'est "à deviner". Toutes les autres touches de
// bordure (Retour, Entrée, Maj gauche, Espace...) sont résolues ci-dessous
// pour retomber PILE sur cette même largeur totale — des valeurs choisies à
// la main dérivaient de quelques px d'une rangée à l'autre, visible à
// l'écran (rectangle pas net).
const TARGET_WIDTH = keyWidth(TAB.units) + contentWidth(ROW1_REAL) + gapsWidth(1 + ROW1_REAL.length);

// Inverse de keyWidth() : combien d'unités pour occuper exactement `px`,
// une fois les autres touches et écarts de la rangée déjà comptés.
function fillerUnits(fixedKeys: KeyDef[], totalItemCount: number): number {
  const remainingPx = TARGET_WIDTH - contentWidth(fixedKeys) - gapsWidth(totalItemCount);
  return (remainingPx + GAP_PX) / (UNIT_PX + GAP_PX);
}

const BACKSPACE: KeyDef = {
  key: "backspace",
  label: "⌫",
  ghost: true,
  ghostIcon: true,
  units: fillerUnits(ROW0_REAL, ROW0_REAL.length + 1),
};

const ENTER: KeyDef = {
  key: "enter",
  label: "↵",
  ghost: true,
  ghostIcon: true,
  units: fillerUnits([CAPSLOCK, ...ROW2_REAL], 2 + ROW2_REAL.length),
};

const SHIFT_L: KeyDef = {
  key: "shiftL",
  label: "⇧",
  ghost: true,
  ghostIcon: true,
  units: fillerUnits(ROW3_REAL, 1 + ROW3_REAL.length),
};

const SPACE: KeyDef = {
  key: "space",
  label: "",
  ghost: true,
  units: fillerUnits([CTRL_L, ALT_L, ALT_R, CTRL_R], 5),
};

export const KEYBOARD_ROWS: { offset: number; keys: KeyDef[] }[] = [
  { offset: 0, keys: [...ROW0_REAL, BACKSPACE] },
  { offset: 0, keys: [TAB, ...ROW1_REAL] },
  { offset: 0, keys: [CAPSLOCK, ...ROW2_REAL, ENTER] },
  { offset: 0, keys: [SHIFT_L, ...ROW3_REAL] },
  // Rangée d'espace : entièrement décorative (voir `ghost` plus haut),
  // n'existe dans aucune rangée du moteur — juste pour fermer le rectangle.
  { offset: 0, keys: [CTRL_L, ALT_L, SPACE, ALT_R, CTRL_R] },
];
