export const TILE_DRAG_OVER_EVENT = "omnikey-tile-drag-over";
export const TILE_DROP_EVENT = "omnikey-tile-drop";

// x/y : position du pointeur — sert à KeyboardEditor pour faire vivre un
// proxy éphémère qui suit le geste banque en continu dès qu'il survole le
// clavier (voir bankProxy dans KeyboardEditor.tsx), exactement comme le
// proxy du clavier existe et est suivi pendant tout SON geste — condition
// nécessaire pour que Framer ait un historique de position à interpoler au
// moment du relais vers la vraie tuile, pas juste un point de départ
// instantané au tout dernier instant.
export type TileDragOverDetail = { key: string | null; char: string; index?: number; x?: number; y?: number };
export type TileDropDetail = { key: string | null; char: string; index?: number; x?: number; y?: number };

export function dispatchDragOver(detail: TileDragOverDetail) {
  window.dispatchEvent(new CustomEvent(TILE_DRAG_OVER_EVENT, { detail }));
}

export function dispatchDrop(detail: TileDropDetail) {
  window.dispatchEvent(new CustomEvent(TILE_DROP_EVENT, { detail }));
}

export type DropTarget = { key: string; index: number };

type Box = { left: number; top: number; right: number; bottom: number };
type KeyBox = Box & { key: string; centerX: number };

export type BoardGeometry = {
  keys: KeyBox[];
  byKey: Record<string, KeyBox>;
  rows: KeyBox[][];
  box: Box;
  scale: number;
};

const TILE = 44;
const GAP = 2;
const PAD = 8;
const BORDER = 1;
const POPUP_MAX = 260;
const POPUP_OFFSET = 10;
const PITCH = TILE + GAP;
const CONTENT_MAX = POPUP_MAX - 2 * PAD - 2 * BORDER;
const MAX_PER_LINE = Math.max(1, Math.floor((CONTENT_MAX + GAP) / PITCH));
const ROW_TOLERANCE = 8;
const CORRIDOR_SLACK = 8;
const HYSTERESIS = 0.26;

let board: BoardGeometry | null = null;
let counts: Record<string, number> = {};

export function publishBoard(g: BoardGeometry | null, c: Record<string, number>) {
  board = g;
  counts = c;
}

export function currentBoard(): BoardGeometry | null {
  return board;
}

export function tileCount(key: string): number {
  return counts[key] ?? 0;
}

export function measureBoard(scale: number): BoardGeometry | null {
  const keys: KeyBox[] = [];
  document.querySelectorAll<HTMLElement>("[data-kbd-key]").forEach((el) => {
    const key = el.dataset.kbdKey;
    if (!key) return;
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return;
    keys.push({
      key,
      left: r.left,
      top: r.top,
      right: r.right,
      bottom: r.bottom,
      centerX: r.left + r.width / 2,
    });
  });
  if (keys.length === 0) return null;
  const byKey: Record<string, KeyBox> = {};
  for (const k of keys) byKey[k.key] = k;
  const sorted = [...keys].sort((a, b) => a.top - b.top || a.left - b.left);
  const rows: KeyBox[][] = [];
  for (const k of sorted) {
    const row = rows.find((r) => Math.abs(r[0].top - k.top) < ROW_TOLERANCE * scale);
    if (row) row.push(k);
    else rows.push([k]);
  }
  const box: Box = {
    left: Math.min(...keys.map((k) => k.left)),
    top: Math.min(...keys.map((k) => k.top)),
    right: Math.max(...keys.map((k) => k.right)),
    bottom: Math.max(...keys.map((k) => k.bottom)),
  };
  return { keys, byKey, rows, box, scale };
}

type PopupLayout = {
  outer: Box;
  contentLeft: number;
  contentTop: number;
  perLine: number;
  lines: number;
  tile: number;
  pitch: number;
};

// perLine/lines restent décidés en unités locales (pré-transform) : c'est la
// largeur max de la bulle (CONTENT_MAX, en unités locales) qui fixe où le
// moteur de rendu revient à la ligne, AVANT que .kbd-rows ne soit mis à
// l'échelle — voir le diagnostic du brief. Seules les distances qui en
// résultent (positions, tailles) passent en coordonnées viewport via `s`.
export function popupLayout(g: BoardGeometry, key: string, n: number): PopupLayout | null {
  const kb = g.byKey[key];
  if (!kb) return null;
  const s = g.scale;
  const cells = Math.max(n, 1);
  const single = cells * TILE + (cells - 1) * GAP;
  const perLine = single <= CONTENT_MAX ? cells : MAX_PER_LINE;
  const lines = Math.ceil(cells / perLine);
  const contentWidth = Math.min(single, CONTENT_MAX) * s;
  const contentHeight = (lines * TILE + (lines - 1) * GAP) * s;
  const outerWidth = contentWidth + (2 * PAD + 2 * BORDER) * s;
  const outerHeight = contentHeight + (2 * PAD + 2 * BORDER) * s;
  const bottom = kb.top - POPUP_OFFSET * s;
  const top = bottom - outerHeight;
  const left = kb.centerX - outerWidth / 2;
  return {
    outer: { left, top, right: left + outerWidth, bottom },
    contentLeft: left + (PAD + BORDER) * s,
    contentTop: top + (PAD + BORDER) * s,
    perLine,
    lines,
    tile: TILE * s,
    pitch: PITCH * s,
  };
}

function inside(b: Box, x: number, y: number): boolean {
  return x >= b.left && x <= b.right && y >= b.top && y <= b.bottom;
}

function slotIndex(
  x: number,
  y: number,
  lay: PopupLayout,
  n: number,
  current: number | null,
): number {
  if (n <= 0) return 0;
  const line = Math.max(
    0,
    Math.min(lay.lines - 1, Math.floor((y - lay.contentTop) / lay.pitch)),
  );
  const start = line * lay.perLine;
  const count = Math.max(0, Math.min(lay.perLine, n - start));
  const rel = (x - lay.contentLeft - lay.tile / 2) / lay.pitch;
  let k = Math.floor(rel) + 1;
  if (current !== null && Math.floor(current / lay.perLine) === line) {
    const kc = current - start;
    if (rel > kc - 1 - HYSTERESIS && rel < kc + HYSTERESIS) k = kc;
  }
  k = Math.max(0, Math.min(count, k));
  return Math.max(0, Math.min(n, start + k));
}

function nearestKey(g: BoardGeometry, x: number, y: number, reach: number): string | null {
  let bestRow: KeyBox[] | null = null;
  let bestDy = Infinity;
  for (const row of g.rows) {
    const top = Math.min(...row.map((k) => k.top));
    const bottom = Math.max(...row.map((k) => k.bottom));
    const dy = y < top ? top - y : y > bottom ? y - bottom : 0;
    if (dy < bestDy) {
      bestDy = dy;
      bestRow = row;
    }
  }
  if (!bestRow) return null;
  let best: string | null = null;
  let bestDx = Infinity;
  for (const k of bestRow) {
    const dx = x < k.left ? k.left - x : x > k.right ? x - k.right : 0;
    if (dx < bestDx) {
      bestDx = dx;
      best = k.key;
    }
  }
  return Math.hypot(bestDx, bestDy) <= reach ? best : null;
}

export type ResolveOptions = {
  reach?: number;
  slackTop?: number;
  slackSide?: number;
  slackBottom?: number;
};

export function resolveTarget(
  x: number,
  y: number,
  current: DropTarget | null,
  opts: ResolveOptions = {},
): DropTarget | null {
  const g = board;
  if (!g) return null;
  // reach/slackTop/slackSide/slackBottom/CORRIDOR_SLACK sont exprimés en
  // unités naturelles (celles du clavier à kbdScale === 1) — mis à l'échelle
  // ici, à l'usage, pour rester cohérents avec des rects mesurés post-
  // transform (measureBoard). Infinity * g.scale reste Infinity : la ligne
  // d'amortissement plus bas compare `reach` (valeur d'origine, non mise à
  // l'échelle), pas la version mise à l'échelle.
  const reach = opts.reach ?? Infinity;
  const slackTop = (opts.slackTop ?? 160) * g.scale;
  const slackSide = (opts.slackSide ?? 48) * g.scale;
  const slackBottom = (opts.slackBottom ?? 32) * g.scale;
  const outer: Box = {
    left: g.box.left - slackSide,
    top: g.box.top - slackTop,
    right: g.box.right + slackSide,
    bottom: g.box.bottom + slackBottom,
  };
  if (!inside(outer, x, y)) return null;

  let key: string | null = null;
  if (current) {
    const kb = g.byKey[current.key];
    const lay = popupLayout(g, current.key, tileCount(current.key));
    if (kb && inside(kb, x, y)) key = current.key;
    else if (lay && inside(lay.outer, x, y)) key = current.key;
    else if (
      kb &&
      lay &&
      x >= kb.left - CORRIDOR_SLACK * g.scale &&
      x <= kb.right + CORRIDOR_SLACK * g.scale &&
      y >= lay.outer.bottom &&
      y <= kb.top
    )
      key = current.key;
  }
  if (!key) key = nearestKey(g, x, y, reach * g.scale);
  if (!key && current && reach === Infinity) key = current.key;
  if (!key) return null;

  const n = tileCount(key);
  const lay = popupLayout(g, key, n);
  const index = lay
    ? slotIndex(x, y, lay, n, current && current.key === key ? current.index : null)
    : 0;
  return { key, index };
}
