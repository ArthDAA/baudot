# D2 - Conception - Omnikey v0.2 "Var"

> DRAFT D2 (Articulation). Contrat logique, zéro code. Gate : BIOPGE complet.
> Réf. : `1-Conditions.v0-2.md` (amendé 30 sept. 2026). Le visuel v0.1 est hors de ce document : il est conservé tel quel et consomme les mêmes événements (voir B6).
> **Amendement 1 (30 sept. 2026)** : suite à D1 AM1-AM7, B1 (trame bidirectionnelle, SetArm, Key ID = position), B2 (armement, touches d'édition pendant Var), B4 (restitution des touches, état Armed conservé, résolveur de disposition), B5 (touche logique au lieu de Key ID) et B7 (restitution de touche) sont amendés. À revalider par Arthus.

## Solution globale

Trois étages sans hook :

1. **Firmware** (module QMK "Var") : Var est une touche personnalisée. Tant qu'elle est tenue, les touches adressables ne vont pas à l'OS, elles partent en trames Baudot sur un canal HID privé. Aucune langue, aucune variante dans le firmware.
2. **Software** : un `HidSource` lit ces trames et les donne au `Core` (machine à états, sans dépendance OS). Le `Core` consulte la configuration, décide des variantes, émet des événements vers le HUD (inchangé) et des ordres d'écriture vers l'`Injector`.
3. **Injection** : l'`Injector` écrit le caractère choisi dans l'application active. C'est la seule action du software sur l'OS. Il ne lit jamais la saisie.

Flux : touche -> firmware -> trame -> HidSource -> Core -> {HUD, Injector}.

---

## B1 - Trame Baudot (contrat firmware vers software)

> Covers : M2, M3, M4, M5, O1, O4, F4, F5

| Champ | Contenu |
|---|---|
| **Boundary** | Format binaire d'un rapport HID brut de 32 octets, dans les deux sens (Amendement 1). Possède : structure, versionnage, table des Baudot Key ID (positions ISO, F5). Ne possède pas : transport (B3), sémantique linguistique, disposition clavier |
| **Inputs** | Clavier -> software : Var appuyée/relâchée, touche de la table appuyée/relâchée (pendant Var ou armement), état majuscule, signal de vie. Software -> clavier : réponse au signal de vie, demande ou levée d'armement |
| **Outputs** | Rapport de 32 octets : [0] préfixe magique Baudot `0xBD` · [1] version de protocole (`1`) · [2] type · [3] numéro de séquence (0-255, cyclique, propre à chaque sens) · [4] Baudot Key ID ou argument (0 si non applicable) · [5] drapeaux : bit0 **majuscule** = Maj tenue XOR Verr. Maj. actif, calculé par le clavier à l'instant de la trame (D1 AM7) ; autres bits à zéro · [6-31] réservés, à zéro. Types clavier -> software : `0x01` VarDown, `0x02` VarUp, `0x03` KeyDown, `0x04` KeyUp, `0x05` Heartbeat. Types software -> clavier : `0x81` HeartbeatAck, `0x82` SetArm ([4] = 1 armer, 0 lever) |
| **Process** | 1. renseigner type et Key ID -> 2. calculer le bit majuscule (Maj XOR Verr. Maj.) -> 3. incrémenter la séquence -> 4. compléter à zéro -> 5. émettre |
| **Guaranty** | Aucun octet ne contient de texte, de caractère ni de scancode. Le Key ID est un code propre au projet (table F5), il désigne une position et non un caractère. Pendant le maintien : VarDown, KeyDown/KeyUp, VarUp. Pendant l'armement : KeyDown/KeyUp des positions du bloc alphanumérique et de Retour arrière. Sinon : seul le Heartbeat, et le KeyUp d'une touche dont le KeyDown a déjà été transmis. `0xBD` n'est utilisé ni par VIA (`0x01`-`0x15`, `0xFF`) ni par Keychron (`0xA0`-`0xAB`) |
| **Errors** | Version inconnue côté software -> trame ignorée, compteur d'erreurs incrémenté. Préfixe absent (trame VIA) -> ignorée sans erreur. Trou de séquence -> le `Core` force un retour à Idle. Côté clavier : SetArm reçu hors maintien et hors armement -> ignoré |

## B2 - Module firmware "Var"

> Covers : M1, M2, M3, M4, M8, O5

| Champ | Contenu |
|---|---|
| **Boundary** | Module QMK ajouté à la keymap. Possède : keycode Var, redirection des touches pendant Var et pendant l'armement, émission des trames, watchdog du software, application des limites de l'armement. Ne possède pas : langues, variantes, configuration, disposition |
| **Inputs** | Événements de touche QMK · état Maj · état des LED hôte (Verr. Maj.) · trames software (HeartbeatAck, SetArm) |
| **Outputs** | Trames B1 · suppression des événements correspondants vers l'OS |
| **Process** | 1. Var appuyée: si software vivant, VarDown émis, état "tenu", armement et demande d'armement effacés, aucun événement vers l'OS -> 2. toute touche de la table F5 pendant "tenu": KeyDown émis, événement OS supprimé, position mémorisée -> 3. Maj, Verr. Maj. et touches hors table (couche Fn, média, Bluetooth...) : passent normalement -> 4. SetArm(1) pendant "tenu": demande mémorisée ; SetArm(0): demande et armement effacés -> 5. Var relâchée: VarUp émis, état "armé" si une demande est mémorisée, sinon "repos" -> 6. pendant "armé": positions du bloc alphanumérique et Retour arrière redirigées comme en 2, tout le reste passe normalement ; l'armement reste actif jusqu'à SetArm(0) ou au prochain appui de Var, sans délai -> 7. relâchement d'une position mémorisée: KeyUp émis, événement OS supprimé, quel que soit l'état -> 8. en permanence: Heartbeat toutes les secondes |
| **Guaranty** | Hors "tenu" et "armé", le firmware ne fait rien de plus qu'un clavier normal (M3). Si le software ne répond pas au Heartbeat depuis 3 s [ASSUMED], Var ne redirige plus rien et l'armement tombe : aucune frappe n'est avalée (M8). Au démarrage, le software est réputé absent jusqu'à sa première réponse. Var n'envoie jamais de scancode (M2). Aucune demande d'armement ne peut naître hors du maintien de Var |
| **Errors** | Perte de la liaison USB -> état "repos" immédiat. Canal HID plein -> événements de touche conservés, Heartbeat abandonné. Var tenue lors d'un retour de vie du software -> prise en compte au prochain appui, pas en cours de geste |

## B3 - HidSource (lecture côté software)

> Covers : M6, F4, O4, O5, A2

| Champ | Contenu |
|---|---|
| **Boundary** | Découvre le clavier Baudot, lit ses trames, répond au Heartbeat, gère la reconnexion. Ne possède pas : interprétation des trames (B4) |
| **Inputs** | Périphériques HID présents · trames B1 |
| **Outputs** | `BaudotEvent` typé (VarDown, VarUp, KeyDown{id, maj, verr}, KeyUp{id}) vers le `Core` · état `DeviceHealth` {absent, connecté, version incompatible} |
| **Process** | 1. énumérer les interfaces vendor-defined -> 2. ouvrir celle qui répond au préfixe Baudot -> 3. boucle de lecture -> 4. valider préfixe/version/séquence -> 5. convertir -> 6. répondre au Heartbeat -> 7. sur retrait, revenir à l'énumération |
| **Guaranty** | Aucun hook système, aucune lecture d'interface clavier standard. Retrait à chaud du clavier -> `Core` renvoyé à Idle sous 100 ms [ASSUMED]. Plusieurs claviers : le premier qui répond est retenu, choix affiché dans Settings |
| **Errors** | `DeviceNotFound` -> `DeviceHealth=absent` et, si O3 actif, bascule sur la source secondaire. `ProtocolVersionMismatch` -> HUD "clavier à mettre à jour". `ReadError` -> reconnexion bornée |

## B4 - Core (machine à états)

> Covers : M1, M5, M12, O1, O2, O7

| Champ | Contenu |
|---|---|
| **Boundary** | Décide, pour chaque `BaudotEvent`, variantes à montrer, index de cycle, écriture, restitution des touches, état d'armement voulu. Possède : états (Idle, ModifierHeld, CyclingVariants, Armed, CyclingLanguage), marques empilées, touches enfoncées. Ne possède pas : lecture matériel, injection, affichage, disposition OS |
| **Inputs** | `BaudotEvent` · un résolveur position -> touche logique v0.1 (fourni par la plateforme ; Amendement 1) · `Layout` (B5) |
| **Outputs** | `Decision` {ordres (écrire du texte, restituer l'appui/relâchement d'une position), liste d'`HudEvent`, trames vers le clavier (SetArm)} |
| **Process** | Transposition 1:1 de `StateMachine::process_key` de v0.1, la touche logique remplaçant le code VK : 1. VarDown -> ModifierHeld (ou retour de Armed), événements HUD identiques -> 2. KeyDown avec variantes: afficher, avancer le cycle à chaque appui -> 3. touche que v0.1 laissait passer (PassThrough) : ordre de restituer l'appui, puis le relâchement correspondant -> 4. marque combinante: empiler, sinon composer -> 5. Espace pendant Var: cycle de langue -> 6. VarUp -> valider l'élément courant ; Idle, ou Armed si des marques restent empilées -> 7. après chaque événement: si l'état d'armement voulu change, trame SetArm (voulu = marques en attente au relâchement, ou état Armed) |
| **Guaranty** | Aucune I/O. Séquence identique en entrée -> sortie identique. Sur les séquences de référence, sortie identique à v0.1 (M12, AM3), vérifié en rejouant côte à côte la machine v0.1 d'origine et le `Core` alimenté par un modèle du firmware. Var ne peut pas "manger" une lettre : une touche sans variante est restituée à l'OS, qui la traduit avec sa propre disposition, comme en v0.1 |
| **Errors** | `LayoutMissing` -> touches restituées, événement HUD d'erreur. `CycleOutOfRange` -> retour à l'index 0. Trou de séquence ou retrait du clavier -> Idle sans écriture, relâchement restitué pour toute touche encore enfoncée |

## B5 - ConfStore (spec `.conf` v2)

> Covers : M9, F7

| Champ | Contenu |
|---|---|
| **Boundary** | Seul lecteur/écrivain de profils, remplace les deux parseurs de v0.1. Possède : parsing, validation, fusion langue + `perso`, migration, rechargement à chaud. Ne possède pas : choix du profil actif |
| **Inputs** | Chemin `.conf` · demande de fusion (langue, perso) |
| **Outputs** | `Layout` {min, maj: touche logique v0.1 vers liste de variantes avec origine `from_lang`} · avertissements |
| **Process** | 1. lire UTF-8 -> 2. détecter version (sans en-tête = v1) -> 3. parser `min:` et `maj:` -> 4. traduire les noms de touche du `.conf` en touche logique (mêmes règles que `key_char_to_vk` de v0.1 ; la correspondance avec une position se fait à l'exécution via le résolveur, Amendement 1) -> 5. valider -> 6. fusionner en conservant les variantes utilisateur |
| **Guaranty** | Un `.conf` v1 valide produit le même `Layout` qu'en v0.1. Fusion idempotente. Variantes utilisateur jamais supprimées par un changement de langue. Écriture atomique |
| **Errors** | `ParseError(ligne)` -> profil rejeté, l'actif inchangé. `UnknownKey` -> ligne ignorée + avertissement. `IoError` propagé |

## B6 - Bridge vers le HUD (visuel inchangé)

> Covers : M7, M10, M11

| Champ | Contenu |
|---|---|
| **Boundary** | Transport des `HudEvent` du software vers l'UI Tauri. Ne change ni les composants React ni leur comportement |
| **Inputs** | `HudEvent` : Held, Show, Cycle, Hold, Pop, Hide, LangShow, LangCycle, LangCommit, LangCancel, AltState (sémantique conservée, renommable en "VarState" côté interne uniquement), Error |
| **Outputs** | Événements Tauri consommés tels quels par `HudApp`, `AccentHUD`, `KeyboardEditor`, `CharacterBank`, `LanguageBank`, `SettingsApp` |
| **Process** | 1. convertir en événement Tauri -> 2. émettre à la fenêtre HUD -> 3. l'UI réagit comme en v0.1 |
| **Guaranty** | L'UI v0.1 fonctionne sans modification de rendu. Les événements Tauri émis vers le frontend gardent exactement la forme (noms et charges utiles) que consomme `HudApp.tsx` aujourd'hui : l'agent lit `handle_ipc` et `HudEvent` dans `src-tauri/src/main.rs` avant de toucher quoi que ce soit, et traduit tout identifiant propre à Windows (code de touche virtuelle) vers la même valeur que reçoit le frontend actuellement, à partir du Baudot Key ID. Aucune animation, aucune géométrie (`tileDrag.ts`), aucun style n'est modifié |
| **Errors** | Événement inconnu -> ignoré. Version d'événement plus récente -> UI en mode dégradé, sans crash |

## B7 - Injector

> Covers : O6, A4, B1

| Champ | Contenu |
|---|---|
| **Boundary** | Écrit du texte Unicode dans l'application active. Possède : `SendInput` (Windows), événements Unicode (macOS), suppression de re-capture inutile (plus de hook, donc plus de garde anti-auto-injection à gérer dans le chemin principal). Ne possède pas : décision du caractère |
| **Inputs** | Ordres du `Core` : unités UTF-16 ou séquence base + marques ; restitution de l'appui ou du relâchement d'une position (injectée comme touche physique, pour que l'OS applique sa disposition et Maj, comme le PassThrough de v0.1 ; Amendement 1) |
| **Outputs** | Caractères apparus dans l'application active |
| **Process** | 1. recevoir l'ordre -> 2. injecter dans l'ordre donné -> 3. confirmer au `Core` |
| **Guaranty** | N'injecte que sur ordre du `Core`. Ne lit rien. Remplaçable en v0.3 par un TIP TSF sans changer B4 |
| **Errors** | `PermissionDenied` (macOS Accessibilité) -> HUD guide l'utilisateur. `InjectFailed` -> `Core` forcé à Idle. Application élevée refusant l'injection -> comportement documenté, résolu par TSF en v0.3 |

## B8 - Source secondaire (hook v0.1) - ABANDONNÉ

> **Abandonné (D1 AM10, 30 sept. 2026) : Var uniquement.** Aucun hook n'est embarqué ni lancé. Bloc conservé pour mémoire.
>
> Covers : O3, A1

| Champ | Contenu |
|---|---|
| **Boundary** | Le moteur actuel réduit à sa capture, qui convertit AltGr et les touches en `BaudotEvent`, pour les claviers non flashés. Hors chemin principal (M6) |
| **Inputs / Outputs** | Frappes système vers `BaudotEvent` |
| **Guaranty** | Désactivée par défaut dès qu'un clavier Baudot est présent. Aucun événement ne passe deux fois. Même sortie que B3 pour le même geste |
| **Errors** | `HookInstallFailed` -> erreur exposée. Doublon HID+hook détecté -> le hook se coupe |

## B9 - Settings et persistance

> Covers : M7, B5

| Champ | Contenu |
|---|---|
| **Boundary** | Préférences existantes (thème, démarrage auto, rappel d'accents, étiquette de langue, dernier profil, cycle) plus l'état `DeviceHealth`. Le rendu de SettingsApp est conservé ; une ligne d'état "clavier Baudot" s'ajoute |
| **Guaranty** | Un utilisateur v0.1 garde ses préférences. Stockage neutre entre OS, avec import du registre Windows au premier lancement |
| **Errors** | `ConfigCorrupt` -> sauvegarde `.bak` puis défauts |

## B10 - Fiabilité et distribution

> Covers : B6, B7

| Champ | Contenu |
|---|---|
| **Boundary** | Tests de rejeu du `Core` par séquences de `BaudotEvent`, contrôle de non-régression avec v0.1, empaquetage, firmware flashable |
| **Process** | 1. enregistrer les séquences v0.1 (français, pinyin, vietnamien, Dead Keys empilées, cycle de langue) et les réexprimer en `BaudotEvent` -> 2. rejouer en CI -> 3. construire -> 4. publier logiciel et firmware ensemble avec leur numéro de protocole |
| **Guaranty** | Aucune release si une séquence de référence échoue. Firmware et software n'annoncent que des versions de protocole compatibles |
| **Errors** | Séquence divergente -> build bloqué |

---

## Traçabilité D1 vers D2

| Exigence | Blocs |
|---|---|
| M1 | B2, B4 |
| M2, M3, M4 | B1, B2 |
| M5 | B1, B4 |
| M6 | B3 (B8 abandonné, AM10) |
| M7, M10, M11 | B6, B9 |
| M8 | B2, B3 |
| M9 | B5, B9 |
| M12 | B4, B10 |
| B1 (TSF), B2 (conf en firmware) | Hors v0.2. B7 et B5 sont prévus pour |

## Ordre de fabrication recommandé (D3)

1. **Prototype firmware seul** sur ton Keychron : Var + trames + lecture brute par un petit script. Il valide A2 (HID brut, cohabitation avec VIA) avant tout le reste.
2. **B10 tests + B4** : extraire le `Core`, réexprimer les séquences v0.1 en `BaudotEvent`.
3. **B3 + B7** : lecture HID et injection sous Windows, bout en bout sur ton poste. Le HUD s'allume sans avoir été modifié.
4. **B5, B6, B9** : ConfStore unique, pont d'événements propre, persistance.
5. ~~**B8** : le hook devient source secondaire.~~ Abandonné (AM10).
6. **macOS** : lecture HID puis injection, dès qu'une machine Mac existe et que A1 est tranché.

## Retour vers D1 déclenché si

- Le prototype firmware montre que ton modèle n'expose pas l'HID brut ou le bloque avec VIA (A2) -> amender F3/F4.
- A1 sans solution matérielle pour les testeurs Mac -> amender O3 (le hook devient source principale de la bêta Mac).

---

# Consignes D3 pour l'agent de fabrication

> Tu reçois ce DRAFT sans autre contexte. Il fait foi. Le code que tu écris doit respecter `2-Conception.v0-2.md` ; les BIOPGE ne sont jamais recopiés dans le code (docstrings normales seulement).

## Objectif en une phrase

Réécrire Omnikey autour d'un clavier flashable : la touche **Var** (remplace Alt_R) n'envoie aucun scancode à l'OS ; maintenue, le firmware envoie au software des codes Baudot par HID brut ; le software décide des variantes, affiche le HUD et écrit le caractère. Plus de hook clavier global dans le chemin principal.

## Cartographie du dépôt (état v0.1)

| Élément | Décision |
|---|---|
| `omnikey-hud/src/*.tsx`, `*.ts`, `styles.css` (HudApp, AccentHUD, KeyboardEditor, CharacterBank, LanguageBank, SettingsApp, tileDrag, keyboardLayout, assignments, motion, characterBankData...) | **INTOUCHABLES**, sauf une ligne d'état "clavier Baudot" dans SettingsApp (B9) |
| `omnikey-hud/src-tauri/src/main.rs` (Tauri : `EngineState`, `parse_conf`, `load_language`, `watch_conf_file`, `handle_ipc`, registre, tray) | À modifier : remplacer la source stdout du moteur par les nouveaux composants, unifier le parseur (B5), garder commandes Tauri et événements sortants identiques |
| `main.rs` racine + `lockfree_core/Cargo.toml` (moteur, hook, machine à états) | À éclater : la machine à états va dans `core/` (B4), le hook est abandonné (AM10) et ne sert plus que d'oracle v0.1 aux tests, l'injection devient B7 |
| `omnikey-hud/src-tauri/binaries/*.conf` (38 langues + `perso.conf`) | Inchangés, doivent se charger sans perte (M9) |
| `omnikey-hud/scripts/`, `tauri.conf.json` | Adapter le script de build du moteur et les ressources embarquées si les binaires changent |
| `firmware/qmk-var/` | À créer (B1, B2) |

## Ordre de fabrication et verrous

**Étape 0 - Prototype firmware (VERROU).** Sur un fork de `Keychron/qmk_firmware`, branche `wireless_playground`, dossier `keyboards/keychron/k3_max` : ajouter le keycode Var, l'émission de trames B1, le Heartbeat. Écrire un petit lecteur HID brut côté PC. Vérifier, sur du matériel réel ou en le disant explicitement si impossible : (a) l'HID brut est lisible en USB, (b) les trames Baudot ne collisionnent pas avec les commandes VIA, (c) Var n'émet aucun scancode, (d) M8 tient (sans réponse au Heartbeat, aucune frappe avalée). **Si (a), (b) ou (c) échoue : STOP, ne pas improviser. Rapporter et amender D1 (F3/F4/A2) puis D2 (B1-B3).** Tu n'as probablement pas le matériel : dans ce cas, livre le firmware et le lecteur, décris exactement comment Arthus les teste, et attends son retour avant l'étape 3.

**Étape 1 - Tests d'abord.** Extraire le `Core` (B4) dans une crate sans dépendance OS. Enregistrer des séquences de référence issues du comportement v0.1 (français, pinyin, vietnamien, Dead Keys empilées, cycle de langue) et les réexprimer en `BaudotEvent`. Ces tests sont la définition de "identique à v0.1" (M12).

**Étape 2 - ConfStore (B5)** : un seul parseur, `.conf` v1 lisibles, fusion idempotente.

**Étape 3 - HidSource (B3) + Injector Windows (B7)**, branchés au `Core` puis au pont HUD (B6). Résultat attendu : le HUD existant s'allume avec le nouveau clavier sans avoir été modifié.

**Étape 4 - Persistance et Settings (B9).** (Source secondaire hook B8 abandonnée, AM10.)

**Étape 5 - macOS (HidSource + Injector)** : uniquement si Arthus confirme A1 (les bêta-testeurs sont sous macOS et ont besoin de matériel). Sinon, prototype seulement.

## Règles DRAFT applicables

- Jamais de code avant le bloc BIOPGE correspondant.
- Erreur **formelle** (faute d'expression) : corriger sur place. Erreur **logique** (le code viole le contrat D2) : STOP, amender `2-Conception`, revalider, reprendre. **Incohérence systémique** (plusieurs blocs à réécrire) : remonter à D1.
- Ne jamais requalifier une erreur logique en formelle pour éviter la friction.
- Ne jamais résoudre une ambiguïté en silence : tout ce que le DRAFT marque [ASSUMED] et que tu constates faux est signalé et amendé dans D1.
- Tenir `3-DevJournal.v0-2.md` (dans ce dossier) : ce qui est fait, ce qui diverge, ce qui est vérifié ou non.

## Contraintes non négociables (rappel)

1. Var n'émet jamais de scancode (M2). Hors maintien de Var, aucun octet Baudot n'est émis à part le Heartbeat (M3).
2. Aucun hook clavier global dans le chemin principal (M6).
3. Le firmware ne connaît aucune langue ni variante (M5).
4. Tout le visuel v0.1 est conservé (M7).
5. Le HUD reste affichage seul (M10).
6. Aucun terme d'architecture interne (firmware, hook, HID, événements) dans l'UI ou les textes visibles par l'utilisateur (M11).
7. ~~Écrire en français avec les accents dans tout texte destiné à l'utilisateur~~ **Tout texte destiné à l'utilisateur est en anglais (D1 AM12)**. Traits d'union simples (-) uniquement, jamais de tirets longs.

## Ce que tu ne peux pas vérifier seul

Le comportement réel sur le K3 Max, le Bluetooth et le dongle 2.4 GHz, les permissions macOS, et A1 (matériel des bêta-testeurs). Pour chacun, dis ce que tu as et n'as pas testé. Ne présente jamais du code non exécuté comme validé.
