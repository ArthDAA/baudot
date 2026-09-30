import { useCallback, useRef, useState } from "react";

const MIN_WIDTH = 80;
const MAX_WIDTH = 400;

// Même principe que useResizableHeight.ts, sur l'axe horizontal (colonnes
// gauche/droite d'une section plutôt que sa hauteur) — deux hooks distincts
// plutôt qu'un seul généralisé sur les deux axes : chacun reste trivial à
// suivre isolément, et la version verticale est déjà en production.
export function useResizableWidth(storageKey: string, defaultWidth: number) {
  const [width, setWidth] = useState<number>(() => {
    const raw = window.localStorage.getItem(storageKey);
    const n = raw !== null ? Number(raw) : NaN;
    return Number.isFinite(n) ? n : defaultWidth;
  });
  const drag = useRef<{ startX: number; startWidth: number } | null>(null);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      drag.current = { startX: e.clientX, startWidth: width };
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [width],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      if (!drag.current) return;
      const next = Math.min(
        MAX_WIDTH,
        Math.max(MIN_WIDTH, drag.current.startWidth + (e.clientX - drag.current.startX)),
      );
      setWidth(next);
      window.localStorage.setItem(storageKey, String(next));
    },
    [storageKey],
  );

  const onPointerUp = useCallback((e: React.PointerEvent<HTMLElement>) => {
    drag.current = null;
    e.currentTarget.releasePointerCapture(e.pointerId);
  }, []);

  return { width, resizeHandleProps: { onPointerDown, onPointerMove, onPointerUp } };
}
