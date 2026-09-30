import { useMemo, useState, useEffect, useLayoutEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { motion, AnimatePresence } from "framer-motion";
import { displayGlyph, isCombiningMark } from "./AccentHUD";
import { KEYBOARD_ROWS, UNIT_PX, ROW_GAP_PX, keyWidth, shiftedLabel, type KeyDef } from "./keyboardLayout";
import {
  TILE_DRAG_OVER_EVENT,
  TILE_DROP_EVENT,
  measureBoard,
  publishBoard,
  resolveTarget,
  type TileDragOverDetail,
  type TileDropDetail,
} from "./tileDrag";
import {
  TILE_PEEK_EVENT,
  publishAssignments,
  type AssignmentMap,
  type TilePeekDetail,
} from "./assignments";
import { SPRING_DEPLOY, SPRING_REFLOW, PRESS } from "./motion";
import { logDrag, logDrop, logMap } from "./dragLog";

// Provenance posée UNIQUEMENT par load_language côté Rust (voir main.rs) :
// `from_lang = true` = insérée par un chargement de langue, jetable au
// prochain chargement. Aucune action de l'éditeur ne doit jamais la poser —
// déplacer/réordonner une tuile la CONSERVE telle quelle (voir
// applyTileMove), une tuile venant de la banque arrive toujours non marquée.
type Variant = { char: string; from_lang: boolean };
type PersoEntry = { key: string; variants: Variant[] };
type PersoConfData = { lang: string; min: PersoEntry[]; maj: PersoEntry[] };
type Section = "min" | "maj";
type Target = { key: string; index: number };
type Move = { key: string; variant: Variant; index?: number; source?: Target };
type ProxyState = { char: string; width: number; height: number };

// lang vide : simple état "pas encore chargé" avant que read_perso_conf ne
// réponde, jamais affiché tel quel — le backend ne renvoie plus jamais de
// nom de repli générique, toujours une vraie langue (voir main.rs, Rust).
const EMPTY_DATA: PersoConfData = { lang: "", min: [], maj: [] };
const OPEN_INTENT_MS = 90;
const CLOSE_GRACE_MS = 250;
const RETURN_MS = 270;
const HOVER_REACH = 22;
// Un seul geste de drag actif à la fois dans toute l'app : cet identifiant
// fixe suffit à faire le lien entre le proxy (qui le porte) et LA tuile
// réelle qui vient d'atterrir (qui le porte temporairement, le temps de la
// transition) — Framer interpole automatiquement leurs rects respectifs
// quand l'un disparaît pendant que l'autre apparaît dans le même rendu,
// sans qu'on ait à mesurer/animer quoi que ce soit à la main.
const DRAG_LAYOUT_ID = "kbd-drag-tile";

function rowWidth(row: { offset: number; keys: KeyDef[] }): number {
  const content = row.keys.reduce((sum, k) => sum + keyWidth(k.units), 0);
  const gaps = Math.max(0, row.keys.length - 1) * ROW_GAP_PX;
  return row.offset * UNIT_PX + content + gaps;
}

// Largeur/hauteur naturelles (échelle 1) du clavier — calculées depuis les
// données plutôt que mesurées en live sur .kbd-rows : une fois une échelle
// appliquée, une mesure DOM refléterait déjà la taille réduite, rendant
// toute mesure ultérieure circulaire (on rétrécirait indéfiniment).
const NATURAL_KBD_WIDTH = Math.max(...KEYBOARD_ROWS.map(rowWidth));
// Doit rester synchronisé avec .kbd-key{height} dans styles.css.
const KEY_HEIGHT_PX = 34;
const NATURAL_KBD_HEIGHT =
  KEYBOARD_ROWS.length * KEY_HEIGHT_PX + (KEYBOARD_ROWS.length - 1) * ROW_GAP_PX;

function findEntry(list: PersoEntry[], key: string): number {
  for (let i = list.length - 1; i >= 0; i -= 1) {
    if (list[i].key === key) return i;
  }
  return -1;
}

function applyTileMove(list: PersoEntry[], move: Move): PersoEntry[] {
  const { variant, key: targetKey, source } = move;
  const working = [...list];

  if (source) {
    const si = findEntry(working, source.key);
    if (si !== -1) {
      const entry = working[si];
      working[si] = {
        ...entry,
        variants: entry.variants.filter((_, i) => i !== source.index),
      };
    }
  }

  const ti = findEntry(working, targetKey);
  if (ti === -1) {
    return [...working, { key: targetKey, variants: [variant] }];
  }
  const current = working[ti].variants;
  if (current.some((v) => v.char === variant.char)) return working;
  const at =
    move.index === undefined
      ? current.length
      : Math.max(0, Math.min(move.index, current.length));
  working[ti] = {
    ...working[ti],
    variants: [...current.slice(0, at), variant, ...current.slice(at)],
  };
  return working;
}

export function KeyboardEditor() {
  const [data, setData] = useState<PersoConfData>(EMPTY_DATA);
  const [section, setSection] = useState<Section>("min");
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [dragSource, setDragSource] = useState<Target | null>(null);
  const [hoverTarget, setHoverTarget] = useState<Target | null>(null);
  // Vrai pendant tout geste venant de la banque (posé au premier survol,
  // levé au drop) — sert uniquement à désactiver le survol CSS brut des
  // vraies tuiles pendant qu'on tient quelque chose (voir renderKey) : sans
  // ça, une tuile innocente que le curseur croise en chemin s'allumait en
  // bleu/orange par pur :hover, alors que ce n'est pas elle qu'on tient.
  const [bankDragging, setBankDragging] = useState(false);
  const [proxy, setProxy] = useState<ProxyState | null>(null);
  // La tuile qui vient d'être déposée avec succès — porte DRAG_LAYOUT_ID le
  // temps de la transition partagée avec le proxy qui vient de disparaître
  // (voir handleUp), puis se libère une fois l'animation terminée.
  const [justLanded, setJustLanded] = useState<{ key: string; char: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Largeur dispo pour le clavier (fenêtre Paramètres redimensionnable, voir
  // tauri.conf.json) — null tant que le ResizeObserver n'a pas encore fait sa
  // première mesure, kbdScale reste alors à 1 (pas de flash agrandi/réduit).
  const [kbdAvailWidth, setKbdAvailWidth] = useState<number | null>(null);
  // Toujours = largeur dispo / largeur naturelle, sans plafond à 1 : le
  // clavier remplit toujours exactement la largeur de l'app, qu'il doive
  // rétrécir (fenêtre étroite) ou grandir (fenêtre large) pour y arriver.
  // Remonté ici (avant `publish`) : measureBoard() en a besoin pour que la
  // géométrie de la bulle (tileDrag.ts) reste juste à toute échelle, pas
  // seulement à 100 %.
  const kbdScale = kbdAvailWidth === null ? 1 : kbdAvailWidth / NATURAL_KBD_WIDTH;
  const kbdViewportRef = useRef<HTMLDivElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const proxyRef = useRef<HTMLDivElement | null>(null);
  const proxyPos = useRef({ x: 0, y: 0, returning: false });

  useLayoutEffect(() => {
    const el = proxyRef.current;
    if (!proxy || !el) return;
    const p = proxyPos.current;
    el.style.transform = `translate(${p.x}px, ${p.y}px) scale(${p.returning ? 1 : 1.08})`;
  }, [proxy]);

  // Partagé entre startTileDrag (clavier) et le survol banque : replace le
  // proxy déjà monté sans passer par un rendu React (écriture DOM directe,
  // comme le reste du suivi en direct).
  const placeProxy = (x: number, y: number, returning: boolean) => {
    proxyPos.current = { x, y, returning };
    const el = proxyRef.current;
    if (!el) return;
    el.style.transform = `translate(${x}px, ${y}px) scale(${returning ? 1 : 1.08})`;
  };

  const dataRef = useRef(data);
  const sectionRef = useRef(section);
  const dragRef = useRef<Target | null>(null);
  useEffect(() => {
    dataRef.current = data;
  }, [data]);
  useEffect(() => {
    sectionRef.current = section;
  }, [section]);
  useEffect(() => {
    dragRef.current = dragSource;
  }, [dragSource]);

  useEffect(() => {
    invoke<PersoConfData>("read_perso_conf")
      .then(setData)
      .catch((e) => setError(String(e)));
  }, []);

  // Rafraîchit sur "perso-changed" (backend → save_perso_conf/load_language/
  // watcher externe, voir main.rs) — SEUL mécanisme de rafraîchissement,
  // qu'il vienne de cet éditeur, d'AltGr+Espace ou d'une édition externe au
  // bloc-notes. Volontairement une mise à jour d'état, PAS un remontage
  // (contrairement à l'ancien `key={reloadToken}` côté SettingsApp) : le
  // clavier reste monté, Framer interpole entre l'ancien et le nouvel état
  // au lieu de tout refaire clignoter à chaque changement de langue.
  useEffect(() => {
    const unlisten = listen("perso-changed", () => {
      invoke<PersoConfData>("read_perso_conf")
        .then(setData)
        .catch((e) => setError(String(e)));
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const entries = section === "min" ? data.min : data.maj;
  const byKey = useMemo(() => {
    const m = new Map<string, PersoEntry>();
    for (const e of entries) m.set(e.key, e);
    return m;
  }, [entries]);

  // Rappel "déjà en config" pour la banque — republié à chaque changement
  // de données OU de section, puisque la touche affichée pour un même
  // caractère peut différer entre min et maj.
  useEffect(() => {
    const map: AssignmentMap = {};
    for (const e of entries) {
      for (const v of e.variants) {
        (map[v.char] ??= []).push(e.key);
      }
    }
    publishAssignments(map);
  }, [entries]);

  const publish = () => {
    const counts: Record<string, number> = {};
    const src = dragRef.current;
    for (const row of KEYBOARD_ROWS) {
      for (const k of row.keys) {
        const n = byKey.get(k.key)?.variants.length ?? 0;
        counts[k.key] = src && src.key === k.key ? Math.max(0, n - 1) : n;
      }
    }
    publishBoard(measureBoard(kbdScale), counts);
  };
  const publishRef = useRef(publish);
  publishRef.current = publish;

  useLayoutEffect(() => {
    publishRef.current();
  }, [byKey, dragSource, section, kbdAvailWidth]);

  useEffect(() => {
    const onGeom = () => publishRef.current();
    window.addEventListener("resize", onGeom);
    window.addEventListener("scroll", onGeom, true);
    return () => {
      window.removeEventListener("resize", onGeom);
      window.removeEventListener("scroll", onGeom, true);
    };
  }, []);

  // Fenêtre Paramètres redimensionnable (tauri.conf.json) : le clavier doit
  // TOUJOURS occuper toute la largeur de l'app, jamais rester à sa largeur
  // naturelle (NATURAL_KBD_WIDTH) débordante ou perdue dans une fenêtre plus
  // large. kbdAvailWidth suit la largeur réelle disponible ; kbdScale
  // (déclaré plus haut) l'étire ou le rétrécit pour toujours la remplir
  // exactement, measureBoard()/resolveTarget (tileDrag.ts) restant corrects
  // puisqu'ils reçoivent désormais explicitement ce facteur d'échelle.
  useEffect(() => {
    const el = kbdViewportRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width;
      if (width !== undefined) setKbdAvailWidth(width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const stableCount = (key: string) => {
    const list = sectionRef.current === "min" ? dataRef.current.min : dataRef.current.maj;
    const ei = findEntry(list, key);
    const n = ei === -1 ? 0 : list[ei].variants.length;
    const src = dragRef.current;
    return src && src.key === key ? Math.max(0, n - 1) : n;
  };

  const commit = (nextList: PersoEntry[]) => {
    const next =
      sectionRef.current === "min"
        ? { ...dataRef.current, min: nextList }
        : { ...dataRef.current, maj: nextList };
    dataRef.current = next;
    setData(next);
    logMap("persist", { section: sectionRef.current, entries: nextList });
    invoke("save_perso_conf", { data: next }).catch((e) => setError(String(e)));
  };

  const applyMove = (move: Move) => {
    const list = sectionRef.current === "min" ? dataRef.current.min : dataRef.current.maj;
    commit(applyTileMove(list, move));
  };

  const removeVariant = (key: string, index: number) => {
    const list = sectionRef.current === "min" ? dataRef.current.min : dataRef.current.maj;
    const ei = findEntry(list, key);
    if (ei === -1) return;
    logMap("remove", { key, index, char: list[ei].variants[index].char });
    commit(
      list.map((e, i) =>
        i === ei ? { ...e, variants: e.variants.filter((_, vi) => vi !== index) } : e,
      ),
    );
  };

  const closeTimerRef = useRef<number | null>(null);
  const openRef = useRef<{ key: string; timer: number } | null>(null);

  const cancelClose = () => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  };
  const scheduleClose = () => {
    cancelClose();
    closeTimerRef.current = window.setTimeout(() => {
      closeTimerRef.current = null;
      setSelectedKey(null);
    }, CLOSE_GRACE_MS);
  };
  const cancelOpen = () => {
    if (openRef.current !== null) {
      window.clearTimeout(openRef.current.timer);
      openRef.current = null;
    }
  };
  useEffect(
    () => () => {
      cancelClose();
      cancelOpen();
    },
    [],
  );

  const handleHoverMove = (e: React.MouseEvent<HTMLDivElement>) => {
    const current = selectedKey ? { key: selectedKey, index: 0 } : null;
    const t = resolveTarget(e.clientX, e.clientY, current, {
      reach: HOVER_REACH,
      slackTop: 160,
      slackSide: 24,
      slackBottom: 16,
    });
    if (!t) {
      cancelOpen();
      scheduleClose();
      return;
    }
    cancelClose();
    if (t.key === selectedKey) {
      cancelOpen();
      return;
    }
    if (selectedKey !== null) {
      cancelOpen();
      setSelectedKey(t.key);
      return;
    }
    if (openRef.current?.key === t.key) return;
    cancelOpen();
    const timer = window.setTimeout(() => {
      openRef.current = null;
      setSelectedKey(t.key);
    }, OPEN_INTENT_MS);
    openRef.current = { key: t.key, timer };
  };

  // Vrai dès que le proxy éphémère de la banque existe — permet de savoir,
  // dans le handler ci-dessous, s'il faut le faire naître ou juste le
  // replacer, sans dépendre de `proxy` (périmé dans une closure d'effet à
  // dépendances vides).
  const bankProxyActiveRef = useRef(false);

  useEffect(() => {
    const handler = (e: Event) => {
      const { key, char, index, x, y } = (e as CustomEvent<TileDragOverDetail>).detail;
      cancelClose();
      cancelOpen();
      setBankDragging(true);
      if (!key) {
        setSelectedKey(null);
        setHoverTarget(null);
        // On quitte le clavier : plus de cible, le proxy éphémère n'a plus
        // lieu d'être — la puce redevient visible dans la banque (voir son
        // style, conditionné à `overKeyboard`).
        if (bankProxyActiveRef.current) {
          bankProxyActiveRef.current = false;
          setProxy(null);
        }
        return;
      }
      const idx = index ?? stableCount(key);
      setSelectedKey(key);
      setHoverTarget((t) => (t && t.key === key && t.index === idx ? t : { key, index: idx }));

      // Le proxy éphémère doit vivre et être suivi en continu pendant TOUT
      // le survol du clavier — exactement comme celui du clavier existe et
      // se déplace pendant tout SON geste — pas juste apparaître au dernier
      // instant : c'est cet historique qui donne à Framer quelque chose à
      // interpoler au moment du relais vers la vraie tuile (voir
      // TILE_DROP_EVENT plus bas). Une fois né, on le repositionne par
      // simple écriture DOM (placeProxy), sans repasser par React.
      if (x !== undefined && y !== undefined) {
        const px = x - 17;
        const py = y - 17;
        if (bankProxyActiveRef.current) {
          placeProxy(px, py, false);
        } else {
          bankProxyActiveRef.current = true;
          proxyPos.current = { x: px, y: py, returning: false };
          // Ce proxy est rendu hors de .kbd-rows (portail vers document.body,
          // voir plus bas) : sa taille ne suit donc pas kbdScale toute seule,
          // contrairement aux tuiles de la bulle vers lesquelles il se fond —
          // sans ce facteur, l'atterrissage (layoutId) ferait un saut de taille.
          setProxy({ char, width: 34 * kbdScale, height: 34 * kbdScale });
        }
      }
    };
    window.addEventListener(TILE_DRAG_OVER_EVENT, handler);
    return () => window.removeEventListener(TILE_DRAG_OVER_EVENT, handler);
  }, [kbdScale]);

  useEffect(() => {
    const handler = (e: Event) => {
      const { key, char, index } = (e as CustomEvent<TileDropDetail>).detail;
      setBankDragging(false);
      bankProxyActiveRef.current = false;
      if (!key) {
        setHoverTarget(null);
        setSelectedKey(null);
        setProxy(null);
        logDrop("bank:drop (hors clavier, annulé)", { char });
        return;
      }
      const targetIndex = index ?? stableCount(key);
      logDrop("bank:drop", { char, key, index: targetIndex });
      // Même geste que le clavier (handleUp) : commit des données, retrait
      // du proxy et pose du layoutId TOUS ensemble, synchrones — le proxy
      // ayant déjà vécu (et été suivi) pendant tout le survol qui précède,
      // Framer a cette fois un vrai historique de position à interpoler.
      // Toujours non marquée : une tuile venant de la banque n'a jamais de
      // provenance langue, seul load_language en pose (voir Variant).
      applyMove({ key, variant: { char, from_lang: false }, index: targetIndex });
      setSelectedKey(key);
      setHoverTarget(null);
      setProxy(null);
      setJustLanded({ key, char });
    };
    window.addEventListener(TILE_DROP_EVENT, handler);
    return () => window.removeEventListener(TILE_DROP_EVENT, handler);
  }, []);

  // Survol de découverte (pas un drag) d'une puce banque déjà assignée —
  // ouvre juste la bulle de sa touche, comme un survol clavier normal.
  // Volontairement séparé du bus de drag (voir assignments.ts) : ne touche
  // ni `bankDragging` ni `hoverTarget`, rien à faire au-delà d'ouvrir/
  // programmer la fermeture de la bulle.
  useEffect(() => {
    const handler = (e: Event) => {
      const { key } = (e as CustomEvent<TilePeekDetail>).detail;
      if (!key) {
        scheduleClose();
        return;
      }
      cancelClose();
      cancelOpen();
      setSelectedKey(key);
    };
    window.addEventListener(TILE_PEEK_EVENT, handler);
    return () => window.removeEventListener(TILE_PEEK_EVENT, handler);
  }, []);

  const startTileDrag = (
    sourceKey: string,
    index: number,
    variant: Variant,
    e: React.PointerEvent,
  ) => {
    if (e.button !== 0 || dragRef.current !== null) return;
    e.preventDefault();
    const char = variant.char;
    const rect = e.currentTarget.getBoundingClientRect();
    const offsetX = e.clientX - rect.left;
    const offsetY = e.clientY - rect.top;
    const origin = { x: rect.left, y: rect.top };
    const source: Target = { key: sourceKey, index };
    const pointerId = e.pointerId;
    let target: Target | null = source;
    let done = false;
    let moved = false;
    let raf = 0;
    let pending: { x: number; y: number } | null = null;

    logDrag("kbd:grab", { char, source });
    cancelClose();
    cancelOpen();
    dragRef.current = source;
    proxyPos.current = { x: origin.x, y: origin.y, returning: false };
    setDragSource(source);
    setHoverTarget(source);
    setSelectedKey(sourceKey);
    setProxy({ char, width: rect.width, height: rect.height });
    publishRef.current();

    const place = placeProxy;

    const resolveAt = (px: number, py: number) => {
      const next = resolveTarget(px, py, target);
      if (!next) {
        if (target !== null) logDrag("kbd:hover", { char, key: null });
        target = null;
        setHoverTarget(null);
        setSelectedKey(sourceKey);
        return;
      }
      if (target?.key !== next.key) logDrag("kbd:hover", { char, key: next.key, index: next.index });
      target = next;
      setSelectedKey(next.key);
      setHoverTarget((t) =>
        t && t.key === next.key && t.index === next.index ? t : next,
      );
    };

    const flush = () => {
      raf = 0;
      const p = pending;
      pending = null;
      if (!p || done) return;
      place(p.x - offsetX, p.y - offsetY, false);
      resolveAt(p.x, p.y);
    };

    const finish = () => {
      dragRef.current = null;
      setProxy(null);
      setDragSource(null);
      setHoverTarget(null);
    };

    const settleReturn = () => {
      setHoverTarget(null);
      const el = proxyRef.current;
      if (el) {
        el.classList.add("kbd-tile-proxy-return");
        void el.offsetWidth;
      }
      place(origin.x, origin.y, true);
      window.setTimeout(() => {
        dragRef.current = null;
        setProxy(null);
        setDragSource(null);
        setSelectedKey(sourceKey);
      }, RETURN_MS);
    };

    const cleanup = () => {
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", handleUp);
      window.removeEventListener("pointercancel", handleCancel);
      window.removeEventListener("blur", handleCancel);
      window.removeEventListener("keydown", handleKey);
      try {
        rootRef.current?.releasePointerCapture(pointerId);
      } catch {
        void 0;
      }
    };

    const handleMove = (ev: PointerEvent) => {
      if (ev.pointerId !== pointerId || done) return;
      if (!moved && Math.hypot(ev.clientX - e.clientX, ev.clientY - e.clientY) > 3) moved = true;
      pending = { x: ev.clientX, y: ev.clientY };
      if (!raf) raf = requestAnimationFrame(flush);
    };

    const handleUp = (ev: PointerEvent) => {
      if (ev.pointerId !== pointerId || done) return;
      done = true;
      cleanup();
      resolveAt(ev.clientX, ev.clientY);
      const isNoop = !target || (target.key === source.key && target.index === source.index);
      logDrop("kbd:drop", { char, source, target, applied: !isNoop });
      if (!isNoop && target) {
        applyMove({ key: target.key, variant, index: target.index, source });
        // Même geste que finish(), mais on passe le témoin à la vraie
        // tuile plutôt que de couper le proxy dans le vide : les deux
        // écritures d'état (proxy -> null, justLanded -> cette tuile) sont
        // groupées dans le même rendu React, donc Framer les voit comme une
        // seule transition à interpoler entre les deux.
        dragRef.current = null;
        setProxy(null);
        setDragSource(null);
        setHoverTarget(null);
        setJustLanded({ key: target.key, char });
      } else if (moved) {
        settleReturn();
      } else {
        finish();
        setSelectedKey(sourceKey);
      }
    };

    const handleCancel = () => {
      if (done) return;
      done = true;
      cleanup();
      logDrop("kbd:cancel", { char, source });
      if (moved) settleReturn();
      else {
        finish();
        setSelectedKey(sourceKey);
      }
    };

    const handleKey = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") handleCancel();
    };

    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", handleUp);
    window.addEventListener("pointercancel", handleCancel);
    window.addEventListener("blur", handleCancel);
    window.addEventListener("keydown", handleKey);
    try {
      rootRef.current?.setPointerCapture(pointerId);
    } catch {
      void 0;
    }
  };

  // Tant qu'on tient quelque chose (clavier ou banque), les vraies tuiles
  // ne doivent plus répondre au survol brut : seule la tuile tenue (le
  // proxy, ou la puce banque) doit paraître "active", jamais une tuile
  // innocente que le curseur croise en chemin vers sa cible.
  const isDragging = dragSource !== null || bankDragging;

  // Touches décoratives (Tab, Verr. Maj., Espace...) : jamais adressables
  // par le moteur, donc jamais de data-kbd-key (invisible à measureBoard/
  // resolveTarget dans tileDrag.ts — aucune bulle, aucune cible de drop ne
  // peut jamais s'y accrocher), pas de bouton cliquable non plus.
  const renderGhostKey = (k: KeyDef) => (
    <div key={k.key} className="kbd-key-wrap" style={{ width: keyWidth(k.units) }}>
      <button
        type="button"
        disabled
        className={"kbd-key kbd-key-ghost" + (k.ghostIcon ? " kbd-key-ghost-icon" : "")}
      >
        {k.label}
      </button>
    </div>
  );

  // Bascule min/maj greffée sur Verr. Maj. elle-même plutôt qu'un bouton
  // "min/maj" hors-clavier : c'est la vraie touche qui a un état persistant
  // sur un clavier réel (Maj., elle, ne fait que s'appuyer — reste
  // décorative). Vraie touche cliquable (pas disabled, pas atténuée comme
  // les touches décoratives), qui se lit comme enfoncée (↓ + LED allumée,
  // Maj active) ou relâchée (↑ + LED éteinte, min) — jamais de data-kbd-key
  // non plus, ce n'est toujours pas une touche adressable par le moteur.
  const renderToggleKey = (k: KeyDef) => {
    const isMaj = section === "maj";
    return (
      <div key={k.key} className="kbd-key-wrap" style={{ width: keyWidth(k.units) }}>
        <button
          type="button"
          className={"kbd-key kbd-key-toggle" + (isMaj ? " kbd-key-toggle-active" : "")}
          onClick={() => setSection(isMaj ? "min" : "maj")}
          title={isMaj ? "Shift layer active - click to go back to lowercase" : "Click to edit the Shift layer"}
        >
          <span className="kbd-key-toggle-led" />
          {isMaj ? "↓" : "↑"}
        </button>
      </div>
    );
  };

  const renderKey = (k: KeyDef) => {
    if (k.ghost) return renderGhostKey(k);
    if (k.toggle) return renderToggleKey(k);
    const variants = byKey.get(k.key)?.variants ?? [];
    const has = variants.length > 0;
    const isPreviewTarget = hoverTarget?.key === k.key;
    const isDraggingFromHere = dragSource?.key === k.key;

    // `slot` = position séquentielle parmi les tuiles réellement rendues
    // (0..n-1), calculée AVANT l'insertion du marqueur ci-dessous — c'est ce
    // que tileDrag.ts lit via data-slot-index pour mesurer la géométrie de
    // la bulle. À ne pas confondre avec `i`, l'index dans le tableau complet
    // des variantes (qui a des trous une fois la tuile en cours de drag
    // filtrée).
    const tiles: Array<
      { marker: true } | { marker: false; variant: Variant; i: number; slot: number }
    > = variants
      .map((variant, i) => ({ marker: false as const, variant, i }))
      .filter((t) => !(isDraggingFromHere && t.i === dragSource!.index))
      .map((t, slot) => ({ ...t, slot }));
    if (isPreviewTarget && hoverTarget) {
      const at = Math.max(0, Math.min(hoverTarget.index, tiles.length));
      tiles.splice(at, 0, { marker: true });
    }

    const isOpen = (selectedKey === k.key || isDraggingFromHere) && tiles.length > 0;

    return (
      <div
        key={k.key}
        data-kbd-key={k.key}
        className="kbd-key-wrap"
        style={{ width: keyWidth(k.units) }}
      >
        <button
          className={
            "kbd-key" + (has ? " kbd-key-active" : "") + (isOpen ? " kbd-key-selected" : "")
          }
        >
          {k.label ?? (section === "maj" ? shiftedLabel(k.key) : k.key)}
        </button>

        <AnimatePresence>
          {isOpen && (
            <motion.div
              className="kbd-popup"
              data-popup-key={k.key}
              // Sur une touche VIDE, la bulle n'existe que parce que la
              // banque la survole (marqueur) — au drop, elle recevrait donc
              // à la fois SA PROPRE entrée (fondu/échelle) ET la transition
              // partagée de la tuile qui atterrit dedans, deux animations
              // simultanées sur des rects imbriqués : Framer mesure la
              // position finale de la tuile pendant que son parent est
              // encore en train de grandir, d'où un vol qui rate sa cible.
              // `initial={false}` ici la traite comme déjà stable dès que
              // c'est elle qui reçoit l'atterrissage.
              initial={justLanded?.key === k.key ? false : { opacity: 0, scale: 0.85, x: "-50%" }}
              animate={{ opacity: 1, scale: 1, x: "-50%" }}
              exit={{ opacity: 0, scale: 0.85, x: "-50%" }}
              transition={SPRING_DEPLOY}
              style={{ transformOrigin: "bottom center" }}
            >
              <div className="kbd-tiles">
                {tiles.map((t) => {
                  if (t.marker) return <div key="marker" className="kbd-tile-marker" />;
                  const isLanding = justLanded?.key === k.key && justLanded?.char === t.variant.char;
                  return (
                    <motion.div
                      key={t.variant.char}
                      layout
                      layoutId={isLanding ? DRAG_LAYOUT_ID : undefined}
                      transition={SPRING_REFLOW}
                      data-tile-index={t.i}
                      data-slot-index={t.slot}
                      data-char={t.variant.char}
                      className={
                        "kbd-tile" +
                        (isCombiningMark(t.variant.char) ? " kbd-tile-orange" : " kbd-tile-blue") +
                        (isLanding ? " kbd-tile-landing" : "")
                      }
                      style={{ pointerEvents: isDragging ? "none" : undefined }}
                      whileTap={PRESS}
                      onPointerDown={(e) => startTileDrag(k.key, t.i, t.variant, e)}
                      onLayoutAnimationComplete={
                        isLanding
                          ? () => {
                              setJustLanded(null);
                              setSelectedKey(null);
                            }
                          : undefined
                      }
                    >
                      <span className="kbd-tile-char">{displayGlyph(t.variant.char)}</span>
                      <button
                        className="kbd-tile-remove"
                        onPointerDown={(e) => e.stopPropagation()}
                        onClick={() => removeVariant(k.key, t.i)}
                      >
                        ×
                      </button>
                    </motion.div>
                  );
                })}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    );
  };

  return (
    <div className="kbd-editor" ref={rootRef}>
      {error && <div className="settings-error">{error}</div>}

      <div
        className="kbd-body"
        onMouseMove={dragSource ? undefined : handleHoverMove}
        onMouseLeave={dragSource ? undefined : scheduleClose}
      >
        <div
          className="kbd-rows-viewport"
          ref={kbdViewportRef}
          style={{ height: NATURAL_KBD_HEIGHT * kbdScale }}
        >
          <div
            className="kbd-rows"
            style={{
              width: NATURAL_KBD_WIDTH,
              transform: `scale(${kbdScale})`,
              transformOrigin: "top center",
            }}
          >
            {KEYBOARD_ROWS.map((row, i) => (
              <div className="kbd-row" key={i} style={{ marginLeft: row.offset * UNIT_PX }}>
                {row.keys.map(renderKey)}
              </div>
            ))}
          </div>
        </div>
      </div>

      {proxy &&
        createPortal(
          <motion.div
            ref={proxyRef}
            layoutId={DRAG_LAYOUT_ID}
            className={
              "kbd-tile kbd-tile-proxy " +
              (isCombiningMark(proxy.char) ? "kbd-tile-orange" : "kbd-tile-blue")
            }
            style={{
              position: "fixed",
              left: 0,
              top: 0,
              width: proxy.width,
              height: proxy.height,
            }}
          >
            <span className="kbd-tile-char">{displayGlyph(proxy.char)}</span>
          </motion.div>,
          document.body,
        )}
    </div>
  );
}
