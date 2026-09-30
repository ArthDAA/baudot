import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence, type PanInfo } from "framer-motion";
import { SPRING_REFLOW, PRESS } from "./motion";
import { useResizableHeight } from "./useResizableHeight";
import { useResizableWidth } from "./useResizableWidth";

type Profile = { path: string; name: string; label: string };

// Même tolérance que pointOf dans CharacterBank.tsx (clientX/Y d'abord, sinon
// info.point ajusté du scroll) — dupliqué plutôt que partagé : deux drags
// complètement indépendants (celui-ci n'a rien à voir avec tileDrag.ts, la
// cible est un simple rectangle, pas une géométrie de touches).
function pointOf(e: unknown, info: PanInfo): { x: number; y: number } {
  const ev = e as { clientX?: number; clientY?: number } | null;
  if (ev && typeof ev.clientX === "number" && typeof ev.clientY === "number") {
    return { x: ev.clientX, y: ev.clientY };
  }
  return { x: info.point.x - window.scrollX, y: info.point.y - window.scrollY };
}

function within(rect: DOMRect | undefined, x: number, y: number): boolean {
  if (!rect) return false;
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

// Distance sous laquelle un geste drag+relâcher est en fait un simple clic
// (le pointeur a à peine bougé) — sert à distinguer "charger tout de suite"
// de "faire glisser vers la liste curée" sur le même chip, sans dépendre du
// onClick natif de Framer (peu fiable une fois `drag` posé sur l'élément).
const CLICK_THRESHOLD_PX = 4;

// Le rechargement de l'éditeur après un chargement de langue passe par
// l'évènement backend "perso-changed" (écouté dans SettingsApp), pas par un
// callback ici — load_language l'émet lui-même après avoir écrit
// perso.conf, donc un second mécanisme ad hoc ferait doublon.
export function LanguageBank() {
  const [all, setAll] = useState<Profile[]>([]);
  const [selected, setSelected] = useState<Profile[]>([]);
  const [draggingPath, setDraggingPath] = useState<string | null>(null);
  const [overDrop, setOverDrop] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dropRef = useRef<HTMLDivElement | null>(null);
  // Hauteur des 2 colonnes — tirable depuis la bordure du haut de la section
  // (voir .section-resize-handle, CharacterBank.tsx en a la définition CSS).
  const { height: bankHeight, resizeHandleProps } = useResizableHeight("lang-bank-height", 220);
  // Largeur de la colonne "Toutes les langues" — tirable depuis la bordure
  // avec "Dans le cycle" (curseur ↔), même principe que .bank-flags dans
  // CharacterBank.tsx.
  const { width: allColWidth, resizeHandleProps: colResizeHandleProps } = useResizableWidth(
    "lang-bank-col-width",
    220,
  );

  useEffect(() => {
    invoke<Profile[]>("list_profiles").then(setAll).catch((e) => setError(String(e)));
    invoke<Profile[]>("list_selected_languages").then(setSelected).catch((e) => setError(String(e)));
  }, []);

  const persist = (next: Profile[]) => {
    setSelected(next);
    invoke("save_selected_languages", { names: next.map((p) => p.name) }).catch((e) =>
      setError(String(e)),
    );
  };

  const addToRotation = (p: Profile) => {
    if (selected.some((s) => s.path === p.path)) return;
    persist([...selected, p]);
  };

  const removeFromRotation = (p: Profile) => {
    persist(selected.filter((s) => s.path !== p.path));
  };

  const loadNow = async (p: Profile) => {
    try {
      await invoke("load_language", { path: p.path });
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDrag = (e: unknown, info: PanInfo) => {
    const { x, y } = pointOf(e, info);
    setOverDrop(within(dropRef.current?.getBoundingClientRect(), x, y));
  };

  const handleDragEnd = (p: Profile, e: unknown, info: PanInfo) => {
    setDraggingPath(null);
    setOverDrop(false);
    const distance = Math.hypot(info.offset.x, info.offset.y);
    if (distance < CLICK_THRESHOLD_PX) {
      loadNow(p);
      return;
    }
    const { x, y } = pointOf(e, info);
    if (within(dropRef.current?.getBoundingClientRect(), x, y)) {
      addToRotation(p);
    }
  };

  const isSelected = (p: Profile) => selected.some((s) => s.path === p.path);

  return (
    <div className="settings-row lang-bank-row">
      <div className="section-resize-handle" {...resizeHandleProps} />
      <p className="settings-label">Click: load now · drag right: add to the Var + Space cycle</p>
      {error && <div className="settings-error">{error}</div>}
      <div className="lang-bank-columns">
        <div className="lang-bank-col" style={{ flex: `0 0 ${allColWidth}px` }}>
          <p className="lang-bank-col-label">All languages</p>
          <div className="lang-bank-all" style={{ maxHeight: bankHeight }}>
            {all.map((p) => (
              <motion.div
                key={p.path}
                className={"lang-list-item" + (isSelected(p) ? " lang-list-item-picked" : "")}
                transition={SPRING_REFLOW}
                drag
                dragSnapToOrigin
                dragElastic={0.15}
                whileDrag={{ scale: 1.03, zIndex: 50 }}
                whileTap={PRESS}
                style={{ opacity: draggingPath === p.path ? 0.4 : 1 }}
                onDragStart={() => setDraggingPath(p.path)}
                onDrag={handleDrag}
                onDragEnd={(e, info) => handleDragEnd(p, e, info)}
                title={isSelected(p) ? `${p.label} · already in the cycle` : p.label}
              >
                {p.label}
              </motion.div>
            ))}
          </div>
        </div>
        <div className="lang-bank-col lang-bank-col-cycle">
          <div className="section-col-resize-handle" {...colResizeHandleProps} />
          <p className="lang-bank-col-label">In the cycle (Var + Space)</p>
          <div
            ref={dropRef}
            className={"lang-bank-selected" + (overDrop ? " lang-bank-selected-over" : "")}
            style={{ maxHeight: bankHeight }}
          >
            {selected.length === 0 && (
              <div className="lang-bank-empty">Drag a language from the left to add it.</div>
            )}
            <AnimatePresence initial={false}>
              {selected.map((p, i) => (
                <motion.button
                  type="button"
                  key={p.path}
                  layout
                  initial={{ opacity: 0, scale: 0.85 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.85 }}
                  transition={SPRING_REFLOW}
                  whileTap={PRESS}
                  className="lang-list-item lang-list-item-active"
                  onClick={() => removeFromRotation(p)}
                  title="Click to remove from the cycle"
                >
                  <span className="lang-chip-index">{i + 1}</span>
                  {p.label}
                </motion.button>
              ))}
            </AnimatePresence>
          </div>
        </div>
      </div>
    </div>
  );
}
