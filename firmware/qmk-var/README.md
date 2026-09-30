# Module firmware « Var » - K3 Max (étape 0)

Prototype du module QMK décrit par D2 B1 et B2. Cible produit : ISO. Clavier de dev
d'Arthus : ANSI RGB (D1 A12). Le module est le même pour les deux, seule la keymap change.

## Contenu

| Fichier | Rôle |
|---|---|
| `src/var.c`, `src/var.h` | Touche Var, redirection pendant Var et pendant l'armement, signal de vie |
| `src/baudot_protocol.h` | Trame B1 et table des Key ID (positions) |
| `keymaps/ansi/`, `keymaps/iso/` | Keymaps VIA d'origine de Keychron, avec Var à la place d'Alt droite (couche Windows) et de Cmd droite (couche Mac) |
| `keychron_raw_hid.patch` | Ajoute un point d'accroche `kc_raw_hid_rx_user()` pour les commandes que Keychron et VIA ne reconnaissent pas (préfixe `0xBD`) |
| `build.sh` | Compilation dans le conteneur QMK officiel |
| `build/` | Binaires compilés (non versionnés) |

Le miroir Rust de `var.c` (`core/tests/common/firmware.rs`) est celui sur lequel tournent
les tests d'équivalence avec v0.1. Si tu modifies l'un, modifie l'autre.

## Compiler

Docker Desktop lancé, depuis la racine du dépôt, sous PowerShell :

```powershell
docker run --rm -v omnikey-qmk:/qmk_firmware `
  -v "${PWD}\firmware\qmk-var:/omnikey:ro" `
  -v "${PWD}\firmware\qmk-var\build:/out" `
  ghcr.io/qmk/qmk_cli sh /omnikey/build.sh ansi rgb
```

Remplacer `ansi rgb` par `iso rgb`, `ansi white` ou `iso white` selon le clavier. Le
binaire arrive dans `build/`, par exemple `build/keychron_k3_max_ansi_rgb_var.bin`.

## Flasher

Procédure Keychron : débrancher le câble, mettre l'interrupteur sur **Cable**, maintenir
**Échap** (ou le bouton reset sous la barre d'espace) en rebranchant le câble. Le clavier
passe en mode DFU ; flasher le `.bin` avec
[QMK Toolbox](https://github.com/qmk/qmk_toolbox/releases).

Si Var ne réagit pas après le flash, c'est que VIA a gardé l'ancienne keymap en mémoire.
Faire une réinitialisation d'usine du clavier (Fn + J + Z pendant 3 s).

> Garde un moyen de revenir au firmware d'origine avant de flasher : le firmware officiel du
> K3 Max sur le site de Keychron (même procédure de flash), ou Keychron Launcher.

## Tester les quatre points de l'étape 0

L'outil `baudot-probe` joue le rôle du software :

```powershell
cd input\hid
cargo build --release
.\target\release\baudot-probe.exe --list     # interfaces présentes
.\target\release\baudot-probe.exe            # lit les trames, répond au signal de vie
```

Ouvre un Bloc-notes à côté pour voir ce qui atteint l'OS.

**(a) HID brut lisible en USB.** Avec `baudot-probe` lancé, une ligne « signal de vie »
doit apparaître chaque seconde. Maintiens Var et tape `e` : `Var appuyée`,
`touche 16 appuyée`... Déjà vérifié avec le firmware d'origine : `baudot-probe --ping`
obtient une réponse VIA (voir le journal).

**(b) Pas de collision avec VIA.** Ouvre VIA (ou Keychron Launcher) pendant que
`baudot-probe` tourne. Vérifie que VIA lit et modifie toujours la keymap, et que
`baudot-probe` n'affiche que des trames Baudot ou « trame non Baudot, ignorée ». Signale
tout message d'erreur de VIA : il reçoit aussi le signal de vie, et c'est le point le plus
incertain.

**(c) Var n'émet aucun scancode.** Dans le Bloc-notes, maintiens Var seule puis relâche-la :
rien ne doit se passer (pas de menu, pas de focus sur la barre de menus). Pour une
vérification stricte, utilise un visualiseur d'événements clavier (par exemple la page
https://w3c.github.io/uievents/tools/key-event-viewer.html) : aucun événement à l'appui ni
au relâchement de Var.

**(d) Sans software, rien n'est avalé (M8).** Ferme `baudot-probe`, attends 3 s, puis
maintiens Var et tape `e` dans le Bloc-notes : le `e` doit s'écrire normalement. Même test
avec `baudot-probe --silent` (il lit mais ne répond jamais au signal de vie).

**Armement (AM1).** `baudot-probe --arm` : maintiens Var, tape `1`, relâche Var. L'outil
indique « clavier armé ». Tape `a` sans Var : il doit apparaître dans `baudot-probe`, pas
dans le Bloc-notes. Tape `b` : il doit s'écrire dans le Bloc-notes (armement levé).
Tape aussi Entrée ou une flèche pendant l'armement : elles doivent passer directement à
l'OS.

**Course A11.** Avec `--arm`, relâche Var le plus vite possible après la touche. Note si la
lettre suivante arrive quand même dans `baudot-probe`. Les horodatages de l'outil donnent
le délai entre la touche et le relâchement de Var.

Merci de me rapporter pour chaque point : OK, KO (avec ce que tu as observé), ou non testé.
