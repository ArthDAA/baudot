# 3 - DevJournal - Omnikey v0.2 "Var"

> Journal de fabrication tenu par l'agent de D3. Une entrée par étape : fait, écarts avec D2, vérifié ou non vérifié (et comment).

## Avant l'étape 0 - Amendement 1 (30 sept. 2026)

- **Contradiction relevée entre M12 et M3** : en v0.1, la lettre qui reçoit une diacritique est tapée APRÈS le relâchement d'AltGr, ce que M3 interdisait au clavier de transmettre. Tranché par Arthus : armement demandé par le software, accepté par le clavier seulement pendant Var, sans délai d'expiration (D1 AM1, AM2).
- **Disposition** : ISO uniquement (AM4). Constaté que la seule disposition Windows installée sur le poste est **US** : un tableau AZERTY figé dans le firmware aurait changé le comportement de v0.1, qui dépendait de la disposition de l'OS. D'où AM5 : Key ID = position, traduite par le software via la disposition active.
- **Touches d'édition pendant Var** (AM6) : en v0.1, Entrée ou Ctrl validaient le cycle AVANT d'atteindre l'application. Pour garder cet ordre, elles passent aussi par le software pendant Var.
- D2 amendé en conséquence : B1, B2, B4, B5, B7. **À revalider par Arthus.**

## Étape 0 - Prototype firmware

- Statut : **compilé, non flashé**. Flasher le clavier reste une action d'Arthus.
- Compilation (30 sept. 2026) : conteneur QMK officiel `ghcr.io/qmk/qmk_cli` via Docker Desktop, script `firmware/qmk-var/build.sh`. `keychron/k3_max/ansi/rgb:var` et `keychron/k3_max/iso/rgb:var` compilent sans avertissement sur nos fichiers (patch appliqué, `var.c` compilé, édition de liens OK). Binaires dans `firmware/qmk-var/build/`. Le conteneur demande `pip install -r requirements.txt` du fork à chaque lancement (ajouté au script).
- Clavier de dev ANSI (D1 A12 résolu) : keymap ANSI ajoutée ; la touche ANSI `\` (`KC_BSLS`) reçoit le Key ID 37, comme la touche ISO `#` (même scancode PC `0x2B`). Dossier réorganisé : module partagé dans `src/`, keymaps dans `keymaps/ansi` et `keymaps/iso`.
- Livré :
  - `firmware/qmk-var/` : module `var.c` / `var.h`, table `baudot_protocol.h`, keymap ISO RGB (VIA d'origine, Var à la place d'Alt droite et de Cmd droite), patch `keychron_raw_hid.patch`, procédure de test `README.md`.
  - `input/hid/` : `baudot-probe`, lecteur HID brut qui joue le rôle du software (réponse au signal de vie, armement, mode silencieux, mode `--ping`).
- Constaté dans le fork Keychron (`wireless_playground`) :
  - Keychron définit déjà `via_command_kb()`, qui renvoie vers `kc_raw_hid_rx()`. Sans modification, une commande inconnue repart chez VIA, qui répond « non géré » (`0xFF`). D'où le patch : un point d'accroche faible `kc_raw_hid_rx_user()` dans le `default:` de `kc_raw_hid_rx()`. Le patch a été vérifié à blanc (`patch --dry-run`) sur le fichier actuel de la branche.
  - Identifiants réservés : VIA `0x01`-`0x15` et `0xFF`, Keychron `0xA0`-`0xAB`. Le préfixe `0xBD` n'est utilisé par aucun des deux.
  - `housekeeping_task_kb()` est redéfini par Keychron, mais le cœur QMK appelle `housekeeping_task_user()` séparément, donc `var_task()` s'exécute bien.
  - Le signal de vie ne porte ni touche ni drapeaux Maj/Verr. Maj. (sinon il révélerait l'état de Maj hors de Var, à 1 Hz).
- **Constaté sur le matériel branché (lecture seule, firmware d'origine) :**
  - Le clavier branché se présente comme **K3 Max ANSI RGB** (PID `0x0A30`). Confirmé par Arthus : clavier de dev ANSI, cible produit ISO.
  - Un dongle **Keychron Link** (PID `0xD030`) est aussi présent. `baudot-probe` l'écarte : il ne retient que les PID K3 Max.
  - `baudot-probe --ping` : l'interface HID brute (page `0xFF60`) s'ouvre sans droits particuliers, VIA répond `[01, 00, 0C, 00]` (protocole VIA 12). **(a) partiellement vérifié : le canal est lisible et inscriptible en USB depuis Windows, sans hook**, avec le firmware d'origine.
  - La commande Keychron `0xA1` (version du firmware) reçoit `0xFF` (non gérée) : **le firmware livré n'est pas celui de la branche `wireless_playground` actuelle**. Pas bloquant, mais le flash remplacera un firmware plus ancien ou différent.
- Vérifications :
  - (a) HID brut USB : partiel (voir ci-dessus). Les trames Baudot elles-mêmes restent à voir après le flash.
  - (b) Collision VIA : non testée. Point le plus incertain : VIA reçoit aussi les trames spontanées (signal de vie) sur la même interface.
  - (c) Aucun scancode : non testé sur matériel. Garanti par construction dans `var.c` (`return false` sur Var dans tous les cas) et vérifié sur le modèle Rust.
  - (d) M8 : non testé sur matériel. Vérifié sur le modèle Rust (`without_software_var_is_neutral_and_nothing_is_swallowed`, `losing_the_software_mid_gesture_releases_var`).
  - Course A11 : non mesurée.
- **Retour d'Arthus après flash (30 sept. 2026)** :
  - Firmware ISO flashé par erreur sur le clavier ANSI, puis reflash avec le firmware ANSI. Aucun dégât (même microcontrôleur).
  - Trames reçues par `baudot-probe`, redirection pendant Var confirmée (y compris Tab et Suppr, AM6), armement et lettre suivante reçue hors Var : « ça a l'air de marcher nickel ». Vérifié par Arthus, pas par l'agent.
  - (b) collision VIA : **hors périmètre**, VIA volontairement ignoré (D1 AM8).
  - (c) Var n'émet rien seule : **confirmé par Arthus**.
  - (d) sans software, rien n'est avalé : **confirmé par Arthus**.
  - Course A11 : non mesurée.
- **Un seul bit « majuscule »** (D1 AM7, décision d'Arthus) : le clavier calcule Maj XOR Verr. Maj. et n'envoie que ce bit. Modifiés : B1 (D2), `baudot_protocol.h`, `var.c`, `core/src/protocol.rs`, `engine.rs`, modèle firmware, `baudot-probe`. Test ajouté (`key_frames_carry_only_the_uppercase_bit`). 33 tests verts. Firmwares ANSI et ISO recompilés (30 sept. 2026, 08:05). **Nouveau firmware pas encore flashé.**

## Étape 1 - Core et tests de référence

- Statut : **fait, vérifié par tests** (`cd core && cargo test --release` : 32 tests, tous verts).
- `core/` (crate `omnikey-core`, aucune dépendance) :
  - `key.rs` : table des Key ID (positions ISO), touches logiques v0.1 (valeurs VK, simples nombres), `conf_key_to_logical` (transposition de `key_char_to_vk`).
  - `protocol.rs` : trames B1 dans les deux sens, suivi de séquence.
  - `machine.rs` : `StateMachine` transposée ligne à ligne de v0.1 ; messages HUD sous forme d'énum, avec les lignes IPC exactes de v0.1.
  - `engine.rs` : trame -> résolution de disposition -> machine -> ordres (`Write`, `Forward`) + synchronisation de l'armement (`SetArm`).
- **Définition de « identique à v0.1 »** (`core/tests/v01_equivalence.rs`) : le même clavier physique est piloté à travers deux chaînes, v0.1 (hook -> machine d'origine) et v0.2 (modèle du firmware -> trames -> Engine). Comparés : messages HUD à l'identique, texte et touches reçus par l'application.
  - L'oracle v0.1 est une **copie verbatim** de `main.rs`. Un test échoue si elle dérive.
  - Les 39 `.conf` embarqués donnent le même profil qu'avec le parseur v0.1.
  - 10 séquences nommées : cycle français, enchaînement sous un seul Var, Entrée pendant un cycle, pinyin avec lettre après relâchement (armement), marques vietnamiennes empilées + Retour arrière, réouverture de Var en état armé, sélecteur de langue (validation et annulation), disposition Windows Français, Verr. Maj.
  - 20 000 séquences aléatoires (français US et FR, pinyin, vietnamien FR, perso).
  - Garanties du modèle firmware : aucun KeyDown hors Var et hors armement (2 000 séquences), Var neutre sans software, perte du software en plein geste.
- **Écart assumé avec v0.1, trouvé par les tests aléatoires** : en état Armed, v0.1 avalait le relâchement de Retour arrière même quand l'appui était déjà parti vers l'OS, qui croyait alors la touche toujours enfoncée. v0.2 restitue toujours le relâchement d'une touche dont l'appui a été restitué (`Engine`, seule dérogation, commentée). Le banc de test le déclare explicitement, rien d'autre n'est masqué.
- Non couvert par les tests (écarts connus, D1 A10) : auto-répétition d'une touche tenue pendant Var ; touches hors table (média, Impr. écran, pavé numérique) que v0.1 voyait passer.
- Non vérifié : le code C lui-même (pas compilé). Seul son miroir Rust est testé.

## Étape 2 - ConfStore

- Statut : **fait, vérifié par tests**, **pas encore branché** dans l'application Tauri (étape 4, B5/B9).
- `core/src/conf.rs` : un seul découpage de lignes (`scan`), deux projections (`parse_layout` pour la machine, `parse_display` pour le HUD), édition de `perso.conf` (provenance `*`, `merge_language`, migration de l'ancien format à zones), écriture atomique.
- Vérifié : même profil que v0.1 sur les 39 `.conf` ; fusion idempotente ; variantes de l'utilisateur conservées au changement de langue ; aller-retour lecture/écriture ; migration.
- Écart minime et volontaire : une ligne `lang: ...` contenant un `=` à l'intérieur d'une section était lue comme une touche `l` par le moteur v0.1 et comme un nom de langue par le backend v0.1. Le parseur unique suit le backend. Aucun `.conf` existant n'est concerné.

## Étape 3 - HidSource + Injector Windows

- Statut : **branché, compilé, en test chez Arthus.**
- `input/hid` (B3) : boucle `run` / `serve` (reconnexion toutes les secondes, réponse au signal de vie, suivi de séquence, trames SetArm renvoyées). Le clavier n'est considéré comme Var qu'à sa **première trame Baudot** : un K3 Max au firmware d'origine a le même identifiant USB et ne doit pas couper le moteur v0.1.
- `inject/windows` (B7) : texte par `SendInput` Unicode en un seul bloc (identique à v0.1) ; restitution des touches par scancode (`KEYEVENTF_SCANCODE`), pour que Windows applique sa disposition et Maj comme pour une frappe physique. Résolveur de disposition : `MapVirtualKeyExW` avec la disposition de la fenêtre au premier plan, qui donne les mêmes codes VK que le hook v0.1.
- Backend Tauri (B6) : `VarSink` passe chaque événement au Core, exécute les ordres, puis donne les messages HUD à `handle_ipc` sous forme de lignes IPC v0.1 exactes. **Frontend inchangé.** Le Core suit le profil actif (`start_engine` met à jour son profil à chaque chargement, changement de langue ou rechargement à chaud).
- Source secondaire provisoire (avant B8) : tant qu'aucun clavier Var ne parle, le moteur v0.1 (hook) tourne comme avant. À la première trame Var, il est arrêté (M6) ; au retrait du clavier, il est relancé. Au démarrage de l'app, le moteur v0.1 tourne moins d'une seconde, le temps du premier signal de vie.
- Vérifié : compilation sans avertissement ; 33 tests du Core verts ; avec le `tauri dev` d'Arthus relancé automatiquement sur le nouveau code, **plus aucun `lockfree_core.exe` ne tourne**, donc le clavier Var a bien été reconnu.
- Non vérifié par l'agent : le comportement à la frappe (HUD, écriture, Dead Keys, sélecteur de langue). À tester par Arthus avec le nouveau firmware (bit majuscule unique ; l'ancien firmware ignorerait Verr. Maj.).
- **Var uniquement (D1 AM10, 30 sept. 2026)** : le moteur v0.1 n'est plus ni embarqué ni lancé. Retirés du backend : lancement et supervision de `lockfree_core.exe`, Job Object, lecture de son stdout, recherche de l'exe (`OMNIKEY_CORE`). `start_engine` devient `activate_profile` (profil du Core + affichage + surveillance). À la fermeture, `shutdown_core` relâche côté Windows toute touche restituée encore enfoncée. Retirés du paquet : `binaries/lockfree_core.exe` (ressource NSIS), script `build:engine`. `main.rs` et `lockfree_core/` restent dans le dépôt comme oracle des tests d'équivalence uniquement. D2 B8 marqué abandonné. README mis à jour.
- **Retour d'Arthus (30 sept. 2026)** : « ça a l'air de marcher à la perfection ». Test fait par Arthus à la frappe réelle ; le détail scénario par scénario (Dead Keys, sélecteur de langue, débranchement) n'a pas été rapporté point par point.

## Hors étapes - Noms et identité visuelle (30 sept. 2026, D1 AM11)

- Noms définitifs : **claviers Baudot**, application **Baudot Companion App**. Renommés : nom du produit (installeur `Baudot Companion App_0.1.0_x64-setup.exe`), titres de fenêtres et de dialogues, info-bulle de l'icône, titre et bouton « Quitter » de la configuration, messages du terminal, README.
- Conservés volontairement : identifiant `com.lockfree.omnikey` (dossier AppData où vit `perso.conf`), clé de registre `HKCU\Software\Lockfree\Omnikey` (préférences), noms internes des crates et des événements. Les changer ferait perdre profils et préférences.
- Démarrage automatique : la valeur de registre `Omnikey` est transférée sous `Baudot Companion App` au lancement (`migrate_legacy_autostart`), puis retirée.
- Pied de page de la configuration : « by » suivi du logo Lockfree (`LockfreeLogo.tsx`, SVG en couleur du texte pour suivre le thème clair ou sombre).
- Icônes de l'app générées depuis le logo PNG (centré sur un carré transparent, `tauri icon`). Sources rangées dans `branding/`.
- Vérifié : compilation sans avertissement, vérification TypeScript OK, installeur reconstruit. Non vérifié : rendu visuel du pied de page et des icônes (à regarder par Arthus).
- Icône de la barre des tâches adaptée au thème (30 sept. 2026) : `icons/tray-dark.png` (logo blanc, barre sombre) et `icons/tray-light.png` (logo noir, barre claire), 64 × 64, générées depuis `branding/lockfree-logo.png`. Choix selon `SystemUsesLightTheme` (thème de la barre des tâches, distinct de celui des applications), appliqué à l'icône de notification et au bouton de la fenêtre de configuration. Mise à jour en direct : un fil attend `RegNotifyChangeKeyValue` sur la clé Personalize (aucune scrutation). Compilé sans avertissement. Non vérifié à l'écran ; bascule de thème en direct non testée (l'agent n'a pas modifié le thème Windows d'Arthus). L'icône de l'exécutable et de l'installeur reste le logo noir (un .ico ne peut pas suivre le thème).

## Hors étapes - Application en anglais (30 sept. 2026, D1 AM12)

- Traduits : fenêtre de configuration (thèmes, options, bouton Quit, pied de page), banque de langues (consigne, colonnes, infobulles, « Var + Space » au lieu de « AltGr + Espace »), éditeur de clavier (infobulle de la couche Maj), banque de caractères (titre, infobulles, catégories Subscript, Superscript, Arrow, Currency, Math, Logic, Diacritic, groupe Languages), menu de l'icône (Launch at Windows startup, Quit), titres de fenêtres et de dialogues (Settings, Error), messages d'erreur du backend et du ConfStore, `lang="en"` de la page, nom par défaut « Profile ».
- Catégories corrigées à la source (`scripts/generate-character-bank.mjs`) puis `characterBankData.ts` régénéré (323 caractères, 38 profils).
- Modèle `binaries/perso.conf` : `lang: Perso` devient `lang: Custom` (seul le libellé change). Un `perso.conf` déjà créé chez l'utilisateur garde son nom actuel jusqu'au prochain chargement de langue.
- Conservés : noms de langues en endonymes ; messages du terminal de développement ; documentation interne en français.
- Vérifié : 33 tests du Core verts, backend compilé sans avertissement, vérification TypeScript OK. Non vérifié : relecture visuelle de l'interface.
- HUD collé en haut (30 sept. 2026) : la fenêtre HUD était placée 32 px sous le haut de l'écran, en plus des 32 px de marge de `.hud-anchor` (place de l'ombre). Fenêtre désormais au bord supérieur du moniteur actif (`position_hud`) ; marge CSS inchangée, l'île est donc 32 px plus haut. Compilé ; rendu à vérifier par Arthus.
