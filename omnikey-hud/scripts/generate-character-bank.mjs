// Régénère src/characterBankData.ts à partir de DEUX sources fusionnées :
// 1. Les profils de langue embarqués (src-tauri/binaries/*.conf) — tous les
//    caractères qui y apparaissent comme variante, moins l'alphabet et les
//    chiffres (déjà atteignables directement au clavier physique).
// 2. Des jeux de symboles curés à la main ci-dessous (CURATED_SETS) —
//    indices/exposants, flèches, monnaies, mathématiques/logique étendus —
//    qui ne viennent d'aucun .conf et n'ont donc pas de langue associée.
// À relancer si de nouvelles langues/caractères sont ajoutés aux profils,
// ou si CURATED_SETS change.
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url)); // omnikey-hud/scripts
const binariesDir = join(here, "..", "src-tauri", "binaries");
const outFile = join(here, "..", "src", "characterBankData.ts");

// Mêmes plages que isCombiningMark() (src/AccentHUD.tsx) / is_combining_mark()
// (main.rs) — une seule définition de "qu'est-ce qu'une marque combinante"
// existe déjà dans le code TS ; celle-ci est une copie figée pour ce script
// Node autonome, à garder synchronisée si les plages changent là-bas.
function isCombiningMark(v) {
  const cp = v.codePointAt(0);
  if (cp === undefined) return false;
  return (
    (cp >= 0x0300 && cp <= 0x036f) ||
    (cp >= 0x1ab0 && cp <= 0x1aff) ||
    (cp >= 0x1dc0 && cp <= 0x1dff) ||
    (cp >= 0x20d0 && cp <= 0x20ff) ||
    (cp >= 0xfe20 && cp <= 0xfe2f)
  );
}

// Lettre de base via décomposition NFD (ex. 'é' -> 'e' + accent combinant) :
// l'usage prévu pour `flags` depuis le début (cocher "Français" + "E" pour
// isoler "É"/"é"). Ignorée quand la lettre n'est pas vraiment décomposable
// (ß, œ, æ, đ, ø, ł, ð, þ, ı...) — normalize() n'y change rien, on ne force
// pas une fausse parenté.
function baseLetterFlag(char) {
  const decomposed = char.normalize("NFD").replace(/[̀-ͯ]/g, "");
  const base = decomposed[0];
  return base && /[a-zA-Z]/.test(base) && decomposed.length === 1 ? base.toUpperCase() : null;
}

function stripDottedCircle(v) {
  return v.startsWith("◌") && v.length > 1 ? v.slice(1) : v;
}

function fallbackLanguageName(filename) {
  const stem = filename.replace(/\.conf$/, "").replace(/[_-]/g, " ");
  return stem.replace(/\b\w/g, (c) => c.toUpperCase());
}

// Même règle de parsing que main.rs (parse_conf) : lang:/min:/maj:, clé =
// valeurs séparées par des virgules — volontairement une lecture de surface
// ici (on ne fait que collecter des caractères, pas router min/maj par VK).
function parseConf(path, filename) {
  const content = readFileSync(path, "utf8").replace(/^﻿/, "");
  let lang = null;
  const chars = new Set();
  for (const raw of content.split("\n")) {
    const line = raw.trim();
    if (!line || line === "min:" || line === "maj:") continue;
    const langMatch = line.match(/^lang:\s*(.*)$/);
    if (langMatch) {
      if (langMatch[1].trim()) lang = langMatch[1].trim();
      continue;
    }
    const eq = line.indexOf("=");
    if (eq === -1) continue;
    for (let v of line.slice(eq + 1).split(",")) {
      v = stripDottedCircle(v.trim());
      if (v) chars.add(v);
    }
  }
  return { lang: lang ?? fallbackLanguageName(filename), chars };
}

// Symboles hors-langue, groupés par catégorie — "Indice"/"Exposant" plutôt
// que de laisser les chiffres en 0-9 normaux (déjà sur le clavier), les
// flèches et monnaies déjà manuellement testées par l'utilisateur dans
// perso.conf, et un choix (pas exhaustif) de math/logique étendus.
// Certains symboles ont un double sens (⇒/⇔ sont autant des flèches que des
// connecteurs logiques) : plusieurs flags, pas de catégorie exclusive.
const CURATED_SETS = {
  Subscript: [..."₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎"],
  Superscript: [..."⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁿ"],
  Arrow: [..."←↑→↓↔↕↖↗↘↙↩↪↦⇐⇑⇒⇓⇔⇕⇦⇧⇨⇩"],
  Currency: [..."¢£¤¥€₠₡₦₨₩₪₫₭₮₱₲₴₵₸₹₺₼₽₾₿"],
  Math: [..."±×÷√∛∜∞∑∏∫∬∮∂∆∇°′″≈≠≡≢≤≥≪≫∝∴∵⌈⌉⌊⌋"],
  Logic: [..."¬∧∨⊕⊻∀∃∄∈∉∋⊂⊃⊆⊇∪∩∅⊥⊤⊢⊨⇒⇔"],
  // Marques combinantes isolées de base, en plus des 8 déjà présentes via
  // vietnamese.conf (grave/aigu/circonflexe/tilde/brève/crochet/corne/point
  // souscrit — celles-là viennent du .conf, pas d'ici). Choisies parce
  // qu'une lettre précomposée du bank les porte déjà (Ä→tréma, Ç→cédille,
  // Ā→macron, Ą→ogonek, Č→caron, Å→rond en chef, Ė→point en chef,
  // Ő→double accent aigu) mais qu'aucune n'existait encore isolée,
  // droppable sur n'importe quelle touche — nommé "Diacritic" à dessein
  // (même flag que l'auto-détection isCombiningMark ci-dessous, une seule
  // bulle de filtre unifiée, pas une catégorie séparée en plus).
  Diacritic: [
    "̈", // tréma / diaeresis — Ä Ë Ï Ö Ü Ÿ
    "̧", // cédille — Ç Ş Ģ Ķ Ļ Ņ
    "̄", // macron — Ā Ē Ī Ō Ū
    "̨", // ogonek — Ą Ę Į Ų
    "̌", // caron / háček — Č Š Ž Ě Ň Ř
    "̊", // rond en chef — Å Ů
    "̇", // point en chef — Ė Ż Ṅ
    "̋", // double accent aigu — Ő Ű
    "̦", // virgule souscrite — Ș Ț (distincte de la cédille malgré l'allure)
  ],
};

const files = readdirSync(binariesDir).filter((f) => f.endsWith(".conf") && f !== "perso.conf");
const byChar = new Map(); // char -> Set(flags — langues pour les .conf, catégories pour CURATED_SETS)

for (const f of files) {
  const { lang, chars } = parseConf(join(binariesDir, f), f);
  for (const c of chars) {
    if (/^[a-zA-Z0-9]$/.test(c)) continue; // déjà sur le clavier de base
    if (!byChar.has(c)) byChar.set(c, new Set());
    byChar.get(c).add(lang);
  }
}

for (const [category, chars] of Object.entries(CURATED_SETS)) {
  for (const c of chars) {
    if (!byChar.has(c)) byChar.set(c, new Set());
    byChar.get(c).add(category);
  }
}

const entries = [...byChar.entries()].sort((a, b) => a[0].codePointAt(0) - b[0].codePointAt(0));

const lines = entries.map(([char, langs]) => {
  const flags = [...langs].sort();
  const base = baseLetterFlag(char);
  if (base) flags.push(base);
  // Garde anti-doublon : les marques du nouveau CURATED_SETS.Diacritique
  // portent déjà "Diacritic" via le Set (byChar) au moment où `flags` est
  // construit plus haut — sans ce test, elles l'auraient deux fois.
  if (isCombiningMark(char) && !flags.includes("Diacritic")) flags.push("Diacritic");
  return `  { char: ${JSON.stringify(char)}, flags: ${JSON.stringify(flags)} },`;
});

const header = `// Généré par scripts/generate-character-bank.mjs, fusion de deux sources :
// les 38 profils de langue embarqués (src-tauri/binaries/*.conf — tous les
// caractères qui y apparaissent comme variante, moins l'alphabet et les
// chiffres, déjà atteignables directement au clavier physique) et des jeux
// de symboles curés à la main (CURATED_SETS dans le script : indices,
// exposants, flèches, monnaies, mathématiques/logique étendus — rien de
// tout ça ne vient d'un .conf, donc pas de langue associée). Chaque entrée
// porte : les langues et/ou catégories qui la concernent, sa lettre de base
// si elle est décomposable via Unicode NFD (ex. "É" porte "E" — permet un
// filtre soustractif du type "Français" + "E" pour isoler "É"/"é"), et
// "Diacritic" si c'est une marque combinante isolée (même détection que
// isCombiningMark(), AccentHUD.tsx).
//
// Régénérer si de nouvelles langues/caractères sont ajoutés aux profils, ou
// si CURATED_SETS change : node scripts/generate-character-bank.mjs
export type BankChar = { char: string; flags: string[] };

export const CHARACTER_BANK: BankChar[] = [
${lines.join("\n")}
];
`;

writeFileSync(outFile, header, "utf8");
console.log(`${entries.length} caractères (${files.length} profils analysés) -> ${outFile}`);
