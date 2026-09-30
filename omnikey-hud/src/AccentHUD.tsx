import { useEffect, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { SPRING_DEPLOY, SPRING_REFLOW, FADE, FADE_IN } from "./motion";

// Langage "Dynamic Island" plutôt que "verre translucide web" : une île
// noire opaque (pas de blur derrière-fenêtre — backdrop-filter ne compose
// que le contenu de la page, jamais le bureau réel derrière une fenêtre
// Tauri transparente), qui se matérialise par flou+fondu plutôt que de
// surgir d'un point (scale 0 = Material, pas Apple), et se referme plus
// vite qu'elle ne s'ouvre, sans rebond à la sortie.
export type HudState =
  | { kind: "waiting"; language: string }
  // `marks` : diacritiques déjà empilées avant ce cycle-ci (mode Dead Keys),
  // affichées à gauche de la rangée pour ne jamais disparaître de la vue.
  | { kind: "variants"; variants: string[]; index: number; marks: string[] }
  // Mode Dead Keys : diacritiques déjà empilées (marks), en attente de la
  // touche de base — le "?" représente n'importe quelle touche à venir.
  | { kind: "armed"; marks: string[] }
  // Sélecteur de langue (AltGr+Espace) : la bulle "waiting" se déroule en
  // liste verticale, `index` pointe la langue actuellement mise en avant.
  // `committing` fige la sélection le temps du fondu de sortie (même
  // logique que la dissolution de commit des variantes, HudApp.tsx).
  | { kind: "language"; languages: string[]; index: number; committing: boolean }
  | null;

// U+25CC : caractère standard pour prévisualiser une marque combinante
// isolée (sans lettre de base), ex. "◌" + macron s'affiche "◌̄".
export const DOTTED_CIRCLE = "◌";

// Mêmes plages que is_combining_mark() côté moteur (main.rs) : une variante
// dont le premier point de code tombe ici est une marque seule, illisible
// sans cercle hôte — on préfixe systématiquement pour l'affichage. Exporté :
// KeyboardEditor.tsx en a besoin pour le même rendu, une seule définition.
export function isCombiningMark(v: string): boolean {
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

export function displayGlyph(v: string): string {
  return isCombiningMark(v) ? DOTTED_CIRCLE + v : v;
}

// Surlignage partagé (layoutId="hud-highlight") entre la tuile active du
// cycle et le slot "?" en attente. La couleur (bleu/orange) est deux
// remplissages superposés dont on croise juste l'opacité — Framer anime ça
// nativement sans se soucier de la couleur elle-même (pas besoin de lui
// faire interpoler une variable CSS), donc le changement de teinte devient
// un vrai fondu au lieu d'un cut net. L'ancre porte le layoutId (position
// uniquement, jamais la taille) et découpe les deux remplissages aux coins
// arrondis.
function Highlight({ color = "blue" }: { color?: "blue" | "orange" }) {
  return (
    <motion.div
      layoutId="hud-highlight"
      layout="position"
      className={"hud-highlight-anchor" + (color === "orange" ? " hud-highlight-glow-orange" : " hud-highlight-glow-blue")}
      transition={SPRING_REFLOW}
    >
      <motion.div
        className="hud-highlight-fill hud-highlight-fill-blue"
        initial={{ opacity: color === "blue" ? 1 : 0 }}
        animate={{ opacity: color === "blue" ? 1 : 0 }}
        transition={FADE}
      />
      <motion.div
        className="hud-highlight-fill hud-highlight-fill-orange"
        initial={{ opacity: color === "orange" ? 1 : 0 }}
        animate={{ opacity: color === "orange" ? 1 : 0 }}
        transition={FADE}
      />
    </motion.div>
  );
}

// Tuile des diacritiques déjà empilées (mode Dead Keys) — volontairement
// sans surlignement bleu : le bleu reste réservé à ce qui est réellement
// actif (le cycle en cours, ou le slot "?" en attente), pas à l'historique
// déjà figé des marques empilées.
function MarksTile({ marks }: { marks: string[] }) {
  return (
    <div className="hud-tile">
      <span className="hud-tile-char">
        {DOTTED_CIRCLE}
        {marks.join("")}
      </span>
    </div>
  );
}

// Springs/fondu partagés (motion.ts) — plus de constantes locales : c'est
// précisément le fait d'en redéfinir ici, légèrement différentes de celles
// de KeyboardEditor.tsx, qui avait fait dériver deux paires de doublons.
const MATERIALIZE_EXIT = {
  opacity: 0,
  scale: 0.85,
  filter: "blur(8px)",
  transition: FADE_IN,
} as const;

// Une entrée par touche à diacritique (voir buildReminderKeys, HudApp.tsx) :
// `label` est le caractère réel affiché en tête de colonne ("1" en min,
// "!" en maj via shiftedLabel — même touche physique, deux rappels
// distincts), `glyph` le cercle diacritique déjà composé avec la/les
// marque(s) de cette touche.
export type ReminderKey = { id: string; label: string; glyph: string };

export function AccentHUD({
  state,
  reminder,
  languageLabelWithAccents,
}: {
  state: HudState;
  reminder: ReminderKey[];
  languageLabelWithAccents: boolean;
}) {
  // Portées hors de l'AnimatePresence qui échange waiting/armed/variants :
  // les marques déjà empilées restent un seul et même élément persistant
  // (juste `layout` pour glisser latéralement), jamais démonté/remonté au
  // passage armed <-> variants — donc jamais le flou/fondu de matérialisation.
  const marks = state && (state.kind === "armed" || state.kind === "variants") ? state.marks : [];

  // Le réglage ne s'applique QUE quand un rappel diacritiques est déjà
  // affiché (seul cas où le nom de langue redevient redondant) — sans
  // diacritique à montrer, la pastille de langue reste le seul repère
  // disponible et s'affiche toujours, quel que soit ce réglage.
  const showLanguageLabel = languageLabelWithAccents || reminder.length === 0;

  // La fenêtre HUD est en ignore_cursor_events (main.rs) : impossible de
  // scroller à la souris, donc c'est à nous de garder la langue en cours de
  // sélection dans le cadre pendant le cycle (AltGr+Espace répété).
  const activeLangIndex = state && state.kind === "language" ? state.index : null;
  const activeLangRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (activeLangIndex !== null) {
      activeLangRef.current?.scrollIntoView({ block: "nearest" });
    }
  }, [activeLangIndex]);

  return (
    <div className="hud-anchor">
      <AnimatePresence>
        {state && (
          <motion.div
            key="hud"
            layout
            initial={{ opacity: 0, scale: 0.7, filter: "blur(10px)" }}
            animate={{ opacity: 1, scale: 1, filter: "blur(0px)" }}
            exit={MATERIALIZE_EXIT}
            transition={SPRING_DEPLOY}
            className="hud-panel"
            style={{ transformOrigin: "top center" }}
          >
          <div className="hud-panel-main">
            <AnimatePresence initial={false}>
              {marks.length > 0 && (
                <motion.div
                  key="marks-prefix"
                  layout
                  initial={{ opacity: 0, filter: "blur(6px)" }}
                  animate={{ opacity: 1, filter: "blur(0px)" }}
                  exit={{ opacity: 0, filter: "blur(6px)" }}
                  transition={{ layout: SPRING_REFLOW, opacity: FADE, filter: FADE }}
                  className="hud-marks-prefix"
                >
                  <MarksTile marks={marks} />
                  <span className="hud-armed-sep">+</span>
                </motion.div>
              )}
            </AnimatePresence>
            <AnimatePresence mode="popLayout" initial={false}>
              {state.kind === "waiting" ? (
                !showLanguageLabel ? null : (
                <motion.div
                  key="waiting"
                  layout
                  initial={{ opacity: 0, filter: "blur(6px)" }}
                  animate={{ opacity: 1, filter: "blur(0px)" }}
                  exit={{ opacity: 0, filter: "blur(6px)" }}
                  transition={FADE}
                  className="hud-waiting"
                >
                  {state.language}
                </motion.div>
                )
              ) : state.kind === "language" ? (
                // AltGr+Espace : la bulle de langue se déroule en liste
                // verticale, pré-positionnée sur la langue déjà active
                // (voir LANGSHOW côté Rust) — un nouvel Espace avance,
                // relâcher AltGr valide (committing fige puis dissout,
                // HudApp.tsx).
                <motion.div
                  key="langlist"
                  layout
                  initial={{ opacity: 0, filter: "blur(6px)" }}
                  animate={{ opacity: 1, filter: "blur(0px)" }}
                  exit={{ opacity: 0, filter: "blur(6px)" }}
                  transition={{ layout: SPRING_REFLOW, opacity: FADE, filter: FADE }}
                  className="hud-lang-list"
                >
                  {state.languages.map((name, i) => {
                    const active = i === state.index % state.languages.length;
                    return (
                      <motion.div
                        key={i}
                        ref={active ? activeLangRef : undefined}
                        layout
                        transition={SPRING_REFLOW}
                        className={"hud-lang-row" + (active ? " hud-lang-row-active" : "")}
                      >
                        {name}
                      </motion.div>
                    );
                  })}
                </motion.div>
              ) : (
                // "armed" et "variants" partagent CE bloc (même clé "row",
                // jamais démonté/remonté entre les deux) : seul le contenu
                // (placeholder "?" vs rangée de candidats) change dessous.
                // Sans ça, chaque bascule armed<->variants — donc chaque
                // sélection de diacritique — refaisait fondre l'opacité du
                // parent, ce qui délavait le surlignage partagé alors même
                // qu'il n'a lui-même aucune animation d'opacité.
                <motion.div
                  key="row"
                  layout
                  initial={{ opacity: 0, filter: "blur(6px)" }}
                  animate={{ opacity: 1, filter: "blur(0px)" }}
                  exit={{ opacity: 0, filter: "blur(6px)" }}
                  transition={{ layout: SPRING_REFLOW, opacity: FADE, filter: FADE }}
                  className="hud-variants"
                >
                  {state.kind === "armed" ? (
                    <motion.div
                      layout
                      transition={SPRING_REFLOW}
                      className="hud-tile hud-tile-active hud-tile-placeholder"
                    >
                      <Highlight color="orange" />
                      <motion.span layout="position" transition={SPRING_REFLOW} className="hud-tile-char">
                        ?
                      </motion.span>
                    </motion.div>
                  ) : (
                    state.variants.map((v, i) => {
                      const active = i === state.index % state.variants.length;
                      return (
                        <motion.div
                          key={i}
                          layout
                          transition={SPRING_REFLOW}
                          className={"hud-tile" + (active ? " hud-tile-active" : "")}
                        >
                          {active && <Highlight color={isCombiningMark(v) ? "orange" : "blue"} />}
                          <AnimatePresence mode="popLayout" initial={false}>
                            <motion.span
                              key={v}
                              layout="position"
                              initial={{ opacity: 0, scale: 0.8, filter: "blur(4px)" }}
                              animate={{ opacity: 1, scale: 1, filter: "blur(0px)" }}
                              exit={{ opacity: 0, scale: 0.8, filter: "blur(4px)" }}
                              transition={{ layout: SPRING_REFLOW, default: FADE }}
                              className="hud-tile-char"
                            >
                              {displayGlyph(v)}
                            </motion.span>
                          </AnimatePresence>
                        </motion.div>
                      );
                    })
                  )}
                </motion.div>
              )}
            </AnimatePresence>
          </div>
            {/* Masqué pendant la sélection de langue (AltGr+Espace) : la
               langue affichée est sur le point de changer, un rappel basé
               sur la langue encore active serait faux le temps du geste —
               et surtout, ce doit rester un seul et même élément (voir
               .hud-panel-main/.hud-reminder empilés dans LA MÊME bulle),
               pas une bulle séparée qui apparaît/disparaît à côté. */}
            {state.kind !== "language" && reminder.length > 0 && (
              <div className="hud-reminder">
                {reminder.map((r) => (
                  <div className="hud-reminder-item" key={r.id}>
                    <span className="hud-reminder-label">{r.label}</span>
                    <span className="hud-reminder-glyph">{r.glyph}</span>
                  </div>
                ))}
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
