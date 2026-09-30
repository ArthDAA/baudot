# Baudot Companion App v0.2 « Var » (by Lockfree)

Application Tauri des claviers Baudot. Elle lit le clavier par HID brut, **sans aucun hook
clavier** : sans clavier Baudot branché, elle attend simplement.

## Architecture

```
Clavier Var (firmware ../firmware/qmk-var)
        │  trames HID brutes, seulement pendant Var ou l'armement
        ▼
src-tauri/         (backend Tauri, Rust)
  - VarSink : trame -> omnikey-core (Engine) -> ordres + messages HUD
  - omnikey-inject-windows : écrit le caractère, restitue les touches
  - parsing des .conf pour l'affichage, profil actif, rechargement à chaud
  - positionnement du HUD sur le moniteur de la fenêtre active
  - tray (Paramètres / Autostart / Quitter), registre HKCU\...\Run
        │  événements Tauri ("hud"), identiques à v0.1
        ▼
src/               (React + framer-motion, inchangé depuis v0.1)
  - fenêtre "hud" : transparente, click-through, always-on-top, sans focus
  - fenêtre "settings" : éditeur de clavier, banque de caractères, langues
```

## Développement

```powershell
npm install
npm run tauri dev
```

Le clavier Baudot est détecté à sa première trame (message « Clavier Baudot connecté » dans
le terminal). Fermer `baudot-probe` avant : lui aussi répond au clavier.

## Build de production

```powershell
npm run tauri build
```

Produit un installateur NSIS dans `src-tauri/target/release/bundle/nsis/`, avec les
`.conf` embarqués (`binaries/*.conf`).

## Notes

- Les profils `.conf` sont cherchés à côté de l'exe, dans `files/`, et dans les
  ressources embarquées.
- Le premier lancement manuel active le démarrage automatique avec Windows.
- Le choix min/maj de l'affichage relit Maj/Verr. Maj. au moment du SHOW ; l'écriture
  du caractère vient du Core.
