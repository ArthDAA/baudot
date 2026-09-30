import { useCallback, useRef, useState } from "react";

const MIN_HEIGHT = 60;
const MAX_HEIGHT = 400;

// Redimensionnement vertical d'une section (Banque de caractères, Langues)
// en tirant sur sa bordure supérieure (.settings-row) — la ligne existante
// reste identique, juste rendue tirable (voir .section-resize-handle dans
// styles.css) : aucun élément visuel ajouté, seulement la possibilité de
// changer la taille. Persisté (localStorage) : sinon le réglage se
// réinitialiserait à chaque réouverture de l'app.
export function useResizableHeight(storageKey: string, defaultHeight: number) {
  const [height, setHeight] = useState<number>(() => {
    const raw = window.localStorage.getItem(storageKey);
    const n = raw !== null ? Number(raw) : NaN;
    return Number.isFinite(n) ? n : defaultHeight;
  });
  const drag = useRef<{ startY: number; startHeight: number } | null>(null);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      drag.current = { startY: e.clientY, startHeight: height };
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [height],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      if (!drag.current) return;
      const next = Math.min(
        MAX_HEIGHT,
        Math.max(MIN_HEIGHT, drag.current.startHeight + (e.clientY - drag.current.startY)),
      );
      setHeight(next);
      window.localStorage.setItem(storageKey, String(next));
    },
    [storageKey],
  );

  const onPointerUp = useCallback((e: React.PointerEvent<HTMLElement>) => {
    drag.current = null;
    e.currentTarget.releasePointerCapture(e.pointerId);
  }, []);

  return { height, resizeHandleProps: { onPointerDown, onPointerMove, onPointerUp } };
}
