// Protocole de traçage pour diagnostiquer le drag/drop du clavier et de la
// banque, dans la console DevTools (clic droit > Inspecter dans la fenêtre
// Tauri, ou F12). Trois flux distincts :
//   [drag] — suivi du geste : poignée saisie, changement de touche survolée
//   [drop] — fin de geste : relâchement, ce qui a été figé
//   [map]  — toute mutation RÉELLE de la donnée persistée (perso.conf)
// Actif uniquement en dev : ces logs ne doivent jamais partir en production.
const ENABLED = import.meta.env.DEV;

function emit(tag: string, color: string, label: string, data?: unknown) {
  if (!ENABLED) return;
  if (data !== undefined) console.log(`%c${tag} ${label}`, `color:${color}`, data);
  else console.log(`%c${tag} ${label}`, `color:${color}`);
}

export const logDrag = (label: string, data?: unknown) => emit("[drag]", "#7dd3fc", label, data);
export const logDrop = (label: string, data?: unknown) => emit("[drop]", "#fca5a5", label, data);
export const logMap = (label: string, data?: unknown) => emit("[map]", "#86efac", label, data);
