// Petit bus dédié au rappel "ce caractère est déjà en config" — séparé de
// tileDrag.ts pour ne pas mélanger deux préoccupations sans rapport (la
// géométrie de drop et ce simple aller-retour d'information). Deux volets :
//
// - ASSIGNMENTS_EVENT : KeyboardEditor republie la carte "caractère -> sa
//   touche" à chaque changement de données OU de section (min/maj) —
//   CharacterBank s'y abonne pour savoir quelles puces marquer en
//   pointillés. Un seul événement diffusé (pas un module-state lu à la
//   volée façon tileDrag.ts/board) : ici l'info doit déclencher un
//   RE-RENDU (la classe CSS de chaque puce en dépend), pas juste être
//   consultée ponctuellement pendant un geste.
//
// - TILE_PEEK_EVENT : émis par CharacterBank au survol (pas un drag) d'une
//   puce déjà assignée, pour ouvrir la bulle de sa touche sur le clavier —
//   volontairement distinct de TILE_DRAG_OVER_EVENT, qui pilote aussi
//   `bankDragging` (désactive le survol des vraies tuiles pendant un VRAI
//   geste) : un simple survol de découverte n'est pas un geste, mélanger
//   les deux aurait laissé `bankDragging` bloqué à true après un survol
//   sans drop correspondant pour le repasser à false.
export const ASSIGNMENTS_EVENT = "omnikey-assignments";
// caractère -> TOUTES ses touches (section déjà implicite : celle affichée).
// Un même caractère peut légitimement vivre sur plusieurs touches à la fois
// (redondance volontaire) — un seul domicile perdrait les autres, aussi bien
// pour le marquage visuel de la puce que pour dispatchPeek.
export type AssignmentMap = Record<string, string[]>;
export type AssignmentsDetail = { assignments: AssignmentMap };

export function publishAssignments(assignments: AssignmentMap) {
  window.dispatchEvent(new CustomEvent<AssignmentsDetail>(ASSIGNMENTS_EVENT, { detail: { assignments } }));
}

export const TILE_PEEK_EVENT = "omnikey-tile-peek";
export type TilePeekDetail = { key: string | null };

export function dispatchPeek(key: string | null) {
  window.dispatchEvent(new CustomEvent<TilePeekDetail>(TILE_PEEK_EVENT, { detail: { key } }));
}
