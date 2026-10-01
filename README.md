# Baudot Companion App v0.2 « Var » (by Lockfree)

Application compagnon des **claviers Baudot** (nom de code du projet : Omnikey). Elle permet de taper des caractères accentués et des symboles sous Windows, sans changer
de disposition clavier.

**Principe :** sur un clavier Baudot (aujourd'hui un Keychron K3 Max flashé), la touche **Var** remplace Alt droite.
On maintient Var, on appuie sur une lettre (ex. `e`), et un HUD façon « Dynamic Island »
affiche les variantes (`é è ê ë`). Chaque nouvel appui sur la même lettre fait défiler les
variantes. Au **relâchement de Var**, le caractère sélectionné est écrit dans l'application
active.

**Pas de keylogger :** l'application n'installe aucun hook clavier. Elle ne lit que le canal
privé du clavier, sur lequel le **firmware** n'envoie rien hors du maintien de Var (et de
l'armement des diacritiques, voir plus bas). Sans clavier Baudot branché, l'application attend.

| Geste | Effet |
|---|---|
| `Var` + lettre, puis relâcher Var | Écrit la 1re variante |
| `Var` + lettre ×N | Fait défiler les variantes |
| `Var` + `Maj` + lettre (ou Verr. Maj.) | Variantes majuscules (section `maj:`) |
| `Var` + lettre A, puis lettre B | Valide la variante de A et ouvre celle de B |
| `Var` + lettre sans variante, Entrée, Tab... | La touche passe normalement, après validation de la variante en cours |
| Variante = diacritique combinante (ex. `◌̄`) | **Mode Dead Keys** : la marque est empilée, la prochaine lettre la reçoit, même tapée après le relâchement de Var |
| `Retour arrière` en mode Dead Keys | Dépile la dernière diacritique |
| `Var` + `Espace` (×N) | Sélecteur de langue ; relâcher Var pour valider |
| Clic gauche sur l'icône de la barre des tâches | Ouvre la fenêtre de configuration |
| Clic droit sur l'icône | Menu : démarrage auto (« Launch at Windows startup ») / « Quit » |

---

## Architecture

```
┌──────────────────────────────┐
│ Clavier Var (firmware QMK)   │  firmware/qmk-var
│  - Var n'envoie aucun code à l'OS
│  - pendant Var : positions des touches -> trames privées (HID brut)
│  - armement : la lettre suivante, sur demande faite pendant Var
│  - aucune langue, aucune disposition
└──────────────┬───────────────┘
               │ trames de 32 octets, dans les deux sens (protocole « Baudot »)
               ▼
┌──────────────────────────────┐
│ Baudot Companion App (Tauri) │  omnikey-hud/src-tauri
│  - input/hid : lit le clavier, répond au signal de vie
│  - core : machine à états v0.1, profils .conf
│  - inject/windows : écrit le caractère (SendInput)
│  - HUD, tray, autostart, préférences, rechargement à chaud
└──────────────┬───────────────┘
               │ événements Tauri ("hud", "perso-changed", "theme", ...)
               ▼
┌──────────────────────────────┐
│ Frontend React + framer-motion (omnikey-hud/src, inchangé depuis v0.1)
│  - fenêtre "hud"      : transparente, click-through, toujours au premier plan
│  - fenêtre "settings" : éditeur de clavier, banque de caractères, langues
└──────────────────────────────┘
```

**Positions, pas lettres :** le clavier envoie la position de la touche. L'application la
traduit selon la disposition active de Windows, exactement comme Windows le faisait pour
v0.1. Un `.conf` garde donc le même sens qu'en v0.1, en QWERTY comme en AZERTY.

La conception complète (conditions, contrats par bloc, journal) est dans `data/`.

---

## Carte du projet

```
Omnikey-main/
├── core/                        Crate sans dépendance OS (omnikey-core)
│   ├── src/key.rs               Positions (Key ID) et touches logiques v0.1
│   ├── src/protocol.rs          Trames Baudot
│   ├── src/machine.rs           Machine à états, transposée ligne à ligne de v0.1
│   ├── src/engine.rs            Trame -> disposition -> machine -> ordres + armement
│   ├── src/conf.rs              Lecture, fusion et écriture des .conf (parseur unique)
│   └── tests/                   Équivalence avec v0.1 (oracle = main.rs d'origine)
├── input/hid/                   Lecture du clavier par HID brut + outil baudot-probe
├── inject/windows/              Écriture du caractère et résolution de disposition
├── firmware/qmk-var/            Module QMK Var, keymaps ANSI/ISO, patch Keychron, build Docker
├── main.rs, lockfree_core/      Moteur v0.1 (hook). Plus embarqué : ne sert que d'oracle
│                                aux tests d'équivalence
├── data/                        DRAFT : conditions (D1), conception (D2), journal (D3)
├── branding/                    Logo Lockfree (SVG, PNG), source des icônes de l'app
└── omnikey-hud/                 Application Tauri
    ├── scripts/generate-character-bank.mjs   Régénère src/characterBankData.ts
    ├── src/                     Frontend React (voir omnikey-hud/README.md)
    └── src-tauri/
        ├── src/main.rs          Backend : VarSink, profils, HUD, tray, préférences
        ├── tauri.conf.json      Fenêtre HUD, CSP, bundle NSIS, ressources embarquées
        └── binaries/*.conf      38 profils de langue + perso.conf (modèle par défaut)
```

---

## Profils `.conf`

Format texte UTF-8 :

```
lang: Français
min:
e = é, è, ê, ë
a = à, â, æ
maj:
e = É, È, Ê, Ë
4 = €, ¥, ₿
```

- **Clé** : `a-z`, `0-9`, ponctuation OEM (`` ; = , - . / ` [ \ ] ' ``) ou flèches (`↑ ↓ ← →`).
- **Variantes** : séparées par des virgules. Seul le 1er caractère de chaque variante compte.
- `◌` en tête d'une variante sert à écrire une diacritique combinante seule (`◌̄`). Il est
  ignoré, et la marque déclenche le mode Dead Keys.
- **Diacritiques natives sur les chiffres** : chaque langue propose aussi ses propres accents en
  mode Dead Keys (Var + chiffre, relâcher, puis la lettre), un par touche à partir de `1`, du plus
  fréquent au moins fréquent dans la langue (français : `1` aigu, `2` grave, `3` circonflexe,
  `4` cédille, `5` tréma). Pinyin et vietnamien ont leur propre schéma.
- `*` en fin de variante (dans `perso.conf`) marque une variante venant de la langue chargée.
  C'est ce qui permet de la remplacer quand on change de langue, sans toucher aux ajouts
  personnels.

**Comment ça tourne :** le profil actif est **toujours** `perso.conf`. Choisir une langue
fusionne ses variantes dans `perso.conf`.

### Où sont les fichiers à l'exécution

| Quoi | Emplacement |
|---|---|
| `perso.conf` actif (modifiable) | `%APPDATA%\com.lockfree.omnikey\perso.conf` |
| Langues du sélecteur | `%APPDATA%\com.lockfree.omnikey\selected_languages.json` |
| Préférences (thème, dernier profil…) | Registre `HKCU\Software\Lockfree\Omnikey` |
| Démarrage automatique | Registre `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` → `Omnikey` |

> ⚠️ Au premier lancement manuel, l'application **active d'elle-même le démarrage automatique
> avec Windows**, y compris en `tauri dev`, où c'est alors l'exe de debug qui est enregistré.
> Pour l'enlever : clic droit sur l'icône → décocher « Launch at Windows startup ».

---

## Développement

### Prérequis (Windows)

- **Node.js** (testé avec v24) et npm
- **Rust** stable via [rustup](https://rustup.rs)
- **Visual Studio Build Tools**, charge de travail « Développement Desktop en C++ »
- WebView2 (déjà présent sur Windows 10/11 récents)
- **Docker Desktop**, seulement pour compiler le firmware

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install Rustlang.Rustup
# puis rouvrir le terminal pour que cargo soit dans le PATH
```

### Lancer en dev

```powershell
cd omnikey-hud
npm install
npm run tauri dev
```

- Les modifications de `src/` sont rechargées à chaud. Celles de `src-tauri/`, `core/`,
  `input/` ou `inject/` recompilent et relancent l'app.
- DevTools : clic droit → Inspecter dans la fenêtre de configuration.
- Après ajout de langues ou de caractères dans les `.conf` :
  `node scripts/generate-character-bank.mjs`.

### Tests

```powershell
cd core
cargo test --release
```

Rejoue côte à côte la machine v0.1 d'origine et le Core v0.2 alimenté par un modèle du
firmware (séquences nommées + 20 000 séquences aléatoires). À lancer avant chaque release.

### Build de production

```powershell
cd omnikey-hud
npm run tauri build
```

Produit un installateur **NSIS** (`Baudot Companion App_<version>_x64-setup.exe`) dans
`src-tauri/target/release/bundle/nsis/`, avec tous les
`.conf` embarqués. Pour le firmware, voir `firmware/qmk-var/README.md`.

### Problèmes courants

| Symptôme | Cause / solution |
|---|---|
| `Missing script: "tauri"` / pas de `package.json` | Lancer les commandes depuis `omnikey-hud/`, pas la racine |
| `cargo` introuvable | Rust pas installé, ou terminal pas rouvert après installation |
| `link.exe not found` | Build Tools C++ manquants |
| Var ne fait rien | Clavier pas flashé, ou pas de « Clavier Baudot connecté » dans le terminal ; fermer `baudot-probe` |
| `Port 1420 is already in use` | Une autre instance tourne encore (`netstat -ano \| findstr :1420`, puis `taskkill /PID <pid> /F`) |
| `EBUSY ... src-tauri\target\...` | Vite surveillait `src-tauri` ; déjà exclu dans `vite.config.ts` |
