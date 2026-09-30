// Vocabulaire de mouvement partagé — deux springs nommés, réutilisés partout
// plutôt que redéfinis légèrement différemment à chaque fichier (c'est
// exactement ce qui avait dérivé : DEPLOY_SPRING/POPUP_SPRING d'un côté,
// SHAPE_SPRING/TILE_SPRING de l'autre, quatre variantes d'un même geste —
// et l'une d'elles, sur-amortie, bougeait avec un caractère différent du
// reste du HUD qu'elle était censée imiter).
//
// ζ = damping / (2·√(stiffness·mass)) — toujours légèrement sous-amorti
// (0.75-0.85, un tout petit dépassement avant de se stabiliser), jamais
// critique ni sur-amorti (ζ ≥ 1 : mou, mécanique — jamais "Apple").

// Matérialisation/dismiss : le panneau HUD qui apparaît, une bulle qui
// s'ouvre — tout ce qui "sort de nulle part".
export const SPRING_DEPLOY = { type: "spring", stiffness: 420, damping: 22, mass: 0.5 } as const; // ζ ≈ 0.76

// Reflow/layout/drag : tuiles qui se réarrangent, relâchement d'un drag —
// tout ce qui "se réorganise" plutôt que d'apparaître/disparaître. Sert
// aussi de spring de relâchement pour dragSnapToOrigin (même prop
// `transition`, non scindée par type ⇒ s'applique aux deux).
export const SPRING_REFLOW = { type: "spring", stiffness: 500, damping: 24, mass: 0.4 } as const; // ζ ≈ 0.85

// Un seul fondu dans les deux sens, réutilisé partout plutôt que les
// 0.15/0.16/0.18/0.25s éparpillés d'avant.
export const FADE = { duration: 0.18, ease: "easeOut" } as const;
export const FADE_IN = { duration: 0.18, ease: "easeIn" } as const;

// Retour de pression uniforme sur tout élément Framer tapable — HIG quasi
// universel (bouton/tuile/chip qui se contracte légèrement au clic), absent
// partout jusqu'ici.
export const PRESS = { scale: 0.96 } as const;
