import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { motion, AnimatePresence, type PanInfo } from "framer-motion";
import { resolveTarget, dispatchDragOver, dispatchDrop, type DropTarget } from "./tileDrag";
import {
  ASSIGNMENTS_EVENT,
  dispatchPeek,
  type AssignmentMap,
  type AssignmentsDetail,
} from "./assignments";
import { SPRING_REFLOW, SPRING_DEPLOY, PRESS } from "./motion";
import { logDrag, logDrop } from "./dragLog";
import { CHARACTER_BANK } from "./characterBankData";
import { useResizableHeight } from "./useResizableHeight";
import { useResizableWidth } from "./useResizableWidth";

// Noms des catégories curées (scripts/generate-character-bank.mjs,
// CURATED_SETS) — chacune EST le flag filtrable, contrairement à
// "Alphabet"/"Languages" ci-dessous qui sont des groupes rassemblant
// plusieurs flags distincts (une lettre de base, un nom de langue).
const CURATED_CATEGORY_NAMES = ["Subscript", "Superscript", "Arrow", "Currency", "Math", "Logic"];
const isBaseLetterFlag = (f: string) => /^[A-Z]$/.test(f);

// "Alphabet" et "Languages" sont des groupes repliables (voir openGroup dans
// le composant) — les identités de lettre de base et les noms de langue
// seraient beaucoup trop nombreux pour rester à plat, contrairement à
// Diacritique et aux catégories curées qui restent des bulles isolées.
const { letterFlags, languageFlags, categoryFlags, hasDiacritique } = (() => {
  const seen = new Set<string>();
  for (const c of CHARACTER_BANK) for (const f of c.flags) seen.add(f);
  const letterFlags = [...seen].filter(isBaseLetterFlag).sort();
  const categoryFlags = CURATED_CATEGORY_NAMES.filter((f) => seen.has(f));
  const languageFlags = [...seen]
    .filter((f) => f !== "Diacritic" && !isBaseLetterFlag(f) && !CURATED_CATEGORY_NAMES.includes(f))
    .sort();
  return { letterFlags, languageFlags, categoryFlags, hasDiacritique: seen.has("Diacritic") };
})();

function pointOf(e: unknown, info: PanInfo): { x: number; y: number } {
  const ev = e as { clientX?: number; clientY?: number } | null;
  if (ev && typeof ev.clientX === "number" && typeof ev.clientY === "number") {
    return { x: ev.clientX, y: ev.clientY };
  }
  return { x: info.point.x - window.scrollX, y: info.point.y - window.scrollY };
}

export function CharacterBank() {
  const [draggingChar, setDraggingChar] = useState<string | null>(null);
  // Vrai dès que le geste banque survole une touche du clavier — sert à
  // effacer la puce le temps qu'elle y est (voir style plus bas) : c'est
  // KeyboardEditor qui prend le relais visuel à ce moment-là (un proxy
  // éphémère, suivi en continu, voir tileDrag.ts/KeyboardEditor.tsx) —
  // sinon les deux représentations seraient visibles en même temps.
  const [overKeyboard, setOverKeyboard] = useState(false);
  // Représentation visuelle du drag, portée hors de .bank-grid (voir le
  // rendu portalé plus bas) : la puce d'origine, elle, reste invisible tout
  // le geste — sinon Framer la déplace en `transform` À L'INTÉRIEUR de la
  // grille, qui la coupait net dès qu'elle sortait du cadre borné
  // (max-height + overflow-y:auto), et un `overflow:visible` "pendant un
  // drag" (essayé avant) débornait TOUTE la grille, pas seulement la puce
  // tenue. Nul avant tout survol (drag pas commencé) et dès qu'on survole le
  // clavier (overKeyboard, KeyboardEditor.tsx prend le relais visuel).
  const [dragVisual, setDragVisual] = useState<{ x: number; y: number; diacritic: boolean } | null>(
    null,
  );
  const [assignments, setAssignments] = useState<AssignmentMap>({});
  // Aucun flag coché = pas de filtre (tout s'affiche). Sinon, intersection
  // stricte (every, pas some) : cocher "Latin" + "N" doit isoler les
  // caractères qui portent LES DEUX à la fois, jamais l'un ou l'autre —
  // voir la note sur ALL_FLAGS/flags plus haut.
  const [selectedFlags, setSelectedFlags] = useState<Set<string>>(new Set());
  // Un seul groupe ouvert à la fois — au clic, sa liste de flags apparaît
  // dans une 2e colonne à côté de .bank-flags (voir .bank-flags-secondary),
  // pas en accordéon déroulé sous le bouton comme "Charger une langue".
  const [openGroup, setOpenGroup] = useState<"Alphabet" | "Languages" | null>(null);
  // Hauteur des 3 colonnes (flags, flags secondaires, grille) — une seule
  // valeur partagée, tirable depuis la bordure du haut de la section (voir
  // .section-resize-handle plus bas) : aucun changement visuel par défaut,
  // juste la possibilité de l'ajuster.
  const { height: bankHeight, resizeHandleProps } = useResizableHeight("bank-height", 140);
  // Largeur de .bank-flags / .bank-flags-secondary — tirable depuis la
  // bordure entre elles et .bank-grid (curseur ↔, voir
  // .section-col-resize-handle), même principe que le redimensionnement
  // vertical juste au-dessus.
  const { width: flagsWidth, resizeHandleProps: colResizeHandleProps } = useResizableWidth(
    "bank-flags-width",
    148,
  );
  const toggleFlag = (flag: string) => {
    setSelectedFlags((prev) => {
      const next = new Set(prev);
      if (next.has(flag)) next.delete(flag);
      else next.add(flag);
      return next;
    });
  };
  // "Alphabet"/"Languages" sont des méta-flags : aucun caractère ne les porte
  // jamais littéralement dans son tableau flags (voir groupTrigger plus
  // bas — ce sont des groupes, pas des flags réels côté données), donc un
  // simple c.flags.includes(f) ne matcherait jamais. Cocher "Alphabet"
  // isole plutôt tout ce qui porte AU MOINS une lettre de base (n'importe
  // laquelle), "Languages" tout ce qui porte au moins une langue.
  const matchesFlag = (c: (typeof CHARACTER_BANK)[number], flag: string): boolean => {
    if (flag === "Alphabet") return letterFlags.some((f) => c.flags.includes(f));
    if (flag === "Languages") return languageFlags.some((f) => c.flags.includes(f));
    return c.flags.includes(flag);
  };
  const visibleChars =
    selectedFlags.size === 0
      ? CHARACTER_BANK
      : CHARACTER_BANK.filter((c) => [...selectedFlags].every((f) => matchesFlag(c, f)));
  const targetRef = useRef<DropTarget | null>(null);

  useEffect(() => {
    const handler = (e: Event) => {
      setAssignments((e as CustomEvent<AssignmentsDetail>).detail.assignments);
    };
    window.addEventListener(ASSIGNMENTS_EVENT, handler);
    return () => window.removeEventListener(ASSIGNMENTS_EVENT, handler);
  }, []);

  const handleDrag = (char: string, e: unknown, info: PanInfo) => {
    const { x, y } = pointOf(e, info);
    const target = resolveTarget(x, y, targetRef.current);
    if (target?.key !== targetRef.current?.key) {
      logDrag("bank:hover", { char, key: target?.key ?? null });
    }
    targetRef.current = target;
    setOverKeyboard(target !== null);
    setDragVisual((prev) => (prev ? { ...prev, x, y } : prev));
    dispatchDragOver({ key: target?.key ?? null, char, index: target?.index, x, y });
  };

  const handleDragEnd = (char: string, e: unknown, info: PanInfo) => {
    setDraggingChar(null);
    setOverKeyboard(false);
    setDragVisual(null);
    const { x, y } = pointOf(e, info);
    const target = resolveTarget(x, y, targetRef.current);
    targetRef.current = null;
    logDrop("bank:release", { char, key: target?.key ?? null, index: target?.index });
    dispatchDrop({ key: target?.key ?? null, char, index: target?.index, x, y });
  };

  const flagBubble = (flag: string) => (
    <button
      type="button"
      key={flag}
      className={"bank-flag" + (selectedFlags.has(flag) ? " bank-flag-active" : "")}
      onClick={() => toggleFlag(flag)}
    >
      {flag}
    </button>
  );

  // Bulle déclencheuse d'un groupe ("Alphabet"/"Languages") — mais aussi un
  // vrai flag à part entière (matchesFlag plus haut) : un clic ouvre/ferme
  // SON panneau et coche/décoche SON méta-flag en même temps, pas juste un
  // raccourci vers la liste des lettres/langues individuelles. Active
  // (surlignée) si le panneau est ouvert, si un de ses flags individuels
  // est déjà coché (reste visible une fois refermé), ou si son propre
  // méta-flag l'est (rester actif même après avoir basculé sur l'autre
  // groupe, qui ferme ce panneau-ci sans passer par CE clic).
  const groupTrigger = (label: "Alphabet" | "Languages", flags: string[]) => {
    const open = openGroup === label;
    const hasSelection = flags.some((f) => selectedFlags.has(f));
    const active = open || hasSelection || selectedFlags.has(label);
    return (
      <button
        type="button"
        key={label}
        className={"bank-flag bank-flag-group" + (active ? " bank-flag-active" : "")}
        aria-expanded={open}
        onClick={() => {
          setOpenGroup(open ? null : label);
          toggleFlag(label);
        }}
      >
        {label} ›
      </button>
    );
  };

  return (
    <div className="settings-row bank-row">
      <div className="section-resize-handle" {...resizeHandleProps} />
      <p className="settings-label">Character bank</p>
      <div className="bank-columns">
        <div className="bank-flags" style={{ maxHeight: bankHeight, width: flagsWidth }}>
          {hasDiacritique && flagBubble("Diacritic")}
          {categoryFlags.map(flagBubble)}
          {groupTrigger("Alphabet", letterFlags)}
          {groupTrigger("Languages", languageFlags)}
        </div>
        <AnimatePresence>
          {openGroup && (
            <motion.div
              key={openGroup}
              className="bank-flags-secondary"
              style={{ maxHeight: bankHeight, width: flagsWidth }}
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -8 }}
              transition={SPRING_DEPLOY}
            >
              {(openGroup === "Alphabet" ? letterFlags : languageFlags).map(flagBubble)}
            </motion.div>
          )}
        </AnimatePresence>
        <div className="bank-grid-col">
        <div className="section-col-resize-handle" {...colResizeHandleProps} />
        <div className="bank-grid" style={{ maxHeight: bankHeight }}>
        {visibleChars.map((c) => {
          const assignedKeys = assignments[c.char] ?? [];
          return (
          <motion.div
            key={c.char}
            className={
              "bank-chip" +
              (c.flags.includes("Diacritic") ? " kbd-tile-orange" : " kbd-tile-blue") +
              (draggingChar === c.char ? " bank-chip-dragging" : "") +
              (assignedKeys.length > 0 ? " bank-chip-assigned" : "")
            }
            transition={SPRING_REFLOW}
            drag
            dragSnapToOrigin
            dragElastic={0.15}
            whileDrag={{ scale: 1.2, zIndex: 50 }}
            whileTap={PRESS}
            style={{
              // Toutes les puces (pas juste celle tenue) pendant un drag :
              // la puce d'origine, invisible et pointer-events:none, ne
              // bloque plus le curseur — sans désactiver aussi les AUTRES,
              // celles que le curseur croise en chemin recevaient un vrai
              // survol CSS et s'allumaient en bleu/orange à sa place (même
              // classe de bug que le clavier, voir isDragging plus haut).
              pointerEvents: draggingChar ? "none" : undefined,
              // Invisible dès le grab, pas seulement au-dessus du clavier :
              // Framer déplace CET élément en `transform` à l'intérieur de
              // .bank-grid (bornée + overflow-y:auto), qui le coupait net
              // dès qu'il sortait du cadre — voir dragVisual, la puce
              // vraiment visible pendant le geste, portée hors de la grille.
              opacity: draggingChar === c.char ? 0 : undefined,
            }}
            onDragStart={(e, info) => {
              const { x, y } = pointOf(e, info);
              setDraggingChar(c.char);
              setDragVisual({ x, y, diacritic: c.flags.includes("Diacritic") });
              logDrag("bank:grab", { char: c.char });
            }}
            onDrag={(e, info) => handleDrag(c.char, e, info)}
            onDragEnd={(e, info) => handleDragEnd(c.char, e, info)}
            // Survol de découverte : rappelle où ce caractère est déjà
            // assigné en ouvrant la bulle de sa PREMIÈRE touche sur le
            // clavier (une redondance sur plusieurs touches est un usage
            // légitime — l'infobulle ci-dessous les liste toutes) — jamais
            // pendant un vrai drag (guard `!draggingChar`), ce serait un
            // tout autre geste en cours.
            onHoverStart={() => {
              if (!draggingChar && assignedKeys.length > 0) dispatchPeek(assignedKeys[0]);
            }}
            onHoverEnd={() => {
              if (assignedKeys.length > 0) dispatchPeek(null);
            }}
            title={
              assignedKeys.length > 0
                ? `${c.flags.join(", ")} · already on ${assignedKeys.map((k) => `"${k}"`).join(", ")}`
                : c.flags.join(", ")
            }
          >
            {c.char}
          </motion.div>
          );
        })}
        </div>
        </div>
      </div>
      {dragVisual &&
        !overKeyboard &&
        createPortal(
          <div
            className={"bank-chip bank-chip-proxy" + (dragVisual.diacritic ? " kbd-tile-orange" : " kbd-tile-blue")}
            style={{ position: "fixed", left: dragVisual.x - 17, top: dragVisual.y - 17 }}
          >
            {draggingChar}
          </div>,
          document.body,
        )}
    </div>
  );
}
