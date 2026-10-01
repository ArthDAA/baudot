# D1 - Conditions - Omnikey v0.2 "Var"

> DRAFT D1 (Registration), amendé après décision d'Arthus (30 sept. 2026).
> Décision structurante : la touche Alt_R est remplacée par **Var**. Maintenue, elle n'envoie aucun scancode à l'OS : le clavier (Keychron flashable, QMK) transmet au software des codes Baudot via un canal HID privé. Le software en déduit variantes, affichage et écriture. Objectif : s'éloigner le plus possible de la logique d'un keylogger.
> Règle de périmètre : **tout le visuel est conservé** (HUD, éditeur de clavier, banque de caractères, banque de langues, thèmes, animations).
> Marqueurs : [CODE] constaté dans le code v0.1 · [STATED] dit par Arthus · [ASSUMED] hypothèse à valider.

## État de départ (v0.1.0, constaté)

- Moteur `lockfree_core.exe` : hook `WH_KEYBOARD_LL`, donc le moteur voit TOUTES les frappes de l'OS, en permanence. C'est exactement la propriété que v0.2 veut supprimer. [CODE]
- Machine à états 5 états (Idle, ModifierHeld, CyclingVariants, Armed, CyclingLanguage), injection Unicode via `SendInput`. [CODE]
- Liaison moteur vers HUD : texte sur stdout, non versionné (`HELD`, `SHOW:vk`, `CYCLE:idx`, `HOLD:vk:idx`, `POP`, `HIDE`, `LANGSHOW`, `LANGCYCLE:idx`, `LANGCOMMIT:idx`, `LANGCANCEL`, `ALT:0/1`). [CODE]
- Données : 38 `.conf` + `perso.conf`, parsés deux fois (moteur et backend Tauri). [CODE]
- Aucun test automatisé. NSIS seul, version 0.1.0. Settings en français uniquement. [CODE]
- Le frontend (HudApp, AccentHUD, KeyboardEditor, CharacterBank, LanguageBank, SettingsApp) ne dépend de rien d'OS : il consomme des événements Tauri. [CODE]

## F - Formats

| # | Élément | Valeur |
|---|---|---|
| F1 | Langages | Rust (software), TypeScript/React 19 (UI), C (firmware QMK) |
| F2 | Cadre applicatif | Tauri v2, Vite 7, Framer Motion (inchangés) |
| F3 | Matériel cible v0.2 | Keychron K3 Max **ISO uniquement** (Amendement 1), firmware QMK/VIA flashable (fork Keychron), **filaire USB** [ASSUMED, voir A2] |
| F4 | Canal clavier vers software | HID brut vendor-defined (rapports de 32 octets), **bidirectionnel** (Amendement 1), lecture par le software sans hook |
| F5 | Identifiants de touche | Table "Baudot Key ID" propre au projet : **une position physique de la disposition ISO** (bloc alphanumérique, Espace, flèches, touches d'édition et modificateurs hors Maj/Verr. Maj.). Ni scancode HID, ni position matricielle. Le firmware ne connaît aucune disposition : la traduction position -> touche logique v0.1 (`a`, `4`, `;`...) est faite par le software selon la disposition active de l'OS, exactement comme Windows le faisait pour v0.1 (Amendement 1) |
| F6 | Livrables v0.2 | Module firmware QMK "Var", installeur Windows, paquet macOS, spec de trame, `.conf` spec v2, `docs/DRAFT/` |
| F7 | Profils | `.conf` v1 lisibles sans perte (rétrocompatibilité) |
| F8 | Arborescence cible | `core/` (crate sans OS), `input/{hid,hook}`, `inject/{windows,macos}`, `firmware/qmk-var/`, `hud/`, `docs/DRAFT/` |

## M - Mandatory

| # | Exigence | Source |
|---|---|---|
| M1 | Le geste reste : maintenir Var, choisir, relâcher | [STATED] |
| M2 | **Var n'émet aucun scancode vers l'OS**, ni à l'appui, ni au maintien, ni au relâchement | [STATED] |
| M3 | **Hors maintien de Var et hors armement, le clavier n'émet aucun octet vers le canal Baudot**, à part le signal de vie (M8) et le relâchement d'une touche dont l'appui a déjà été transmis. L'armement n'est accepté par le clavier que s'il est demandé pendant le maintien de Var, il ne concerne que les touches du bloc alphanumérique et Retour arrière, et il cesse dès que le software le lève ou que Var est de nouveau appuyée. Pas de délai d'expiration (Amendement 1). Le software ne voit jamais la saisie normale | [STATED] |
| M4 | Pendant le maintien de Var, les touches de la table F5 ne partent pas vers l'OS : elles partent en codes Baudot vers le software, qui les restitue à l'OS quand v0.1 les laissait passer. Maj et Verr. Maj. passent toujours directement (leur état voyage dans chaque trame) | [STATED] |
| M5 | Le software décide des variantes à partir de la configuration, pas le firmware (le firmware ne connaît aucune langue) | [STATED] |
| M6 | **Aucun hook clavier global dans le chemin principal** du software v0.2 | [STATED] (esprit : loin du keylogger) |
| M7 | Tout le visuel v0.1 est conservé et fonctionne à l'identique | [STATED] |
| M8 | Sans software actif, le clavier reste utilisable : Var retombe en comportement inoffensif et rien n'est avalé | [ASSUMED] (sinon un clavier "cassé" sans l'app) |
| M9 | `.conf` v0.1 et `perso.conf` chargés sans perte | [CODE] |
| M10 | Le HUD reste affichage seul | [STATED] |
| M11 | Aucun terme d'architecture interne dans l'UI ni le texte public | [STATED] |
| M12 | Comportement Dead Keys (marques empilées, composition) identique à v0.1 | [CODE] |

## B - Bonus

| # | Élément | Note |
|---|---|---|
| B1 | Injection par TSF TIP (au lieu de `SendInput`) | Cohérent avec la cible entreprise : entrée par HID, sortie par TIP, aucun hook nulle part. Prévu v0.3 |
| B2 | Profils `.conf` stockés dans le firmware (SKU unique) | v0.3. En v0.2 la configuration reste côté software (M5) |
| B3 | Mode "sans Var" pour claviers non flashables (hook actuel conservé en secours) | Voir O3 et A1 |
| B4 | Support sans fil (Bluetooth) | Limite QMK, voir A2 |
| B5 | Indicateur "clavier Baudot connecté" dans Settings | Faible coût, visuel minimal |
| B6 | Signature de code, updater | Bêta |
| B7 | Suite de tests par rejeu de séquences | Fortement recommandé |

## O - Open Points (décision + raison)

| # | Point | Décision proposée | Raison |
|---|---|---|---|
| O1 | Ce que transporte la trame | Événements Baudot : Var haut/bas, touche haut/bas, état Maj et Verr. Maj. | Le software a besoin de la casse pour choisir min/maj |
| O2 | Cycle des variantes | Chaque appui de la touche pendant Var avance le cycle, calculé par le software | Identique à l'usage v0.1 |
| O3 | Sort du hook v0.1 | **Conservé comme source d'entrée secondaire**, désactivé par défaut quand un clavier Baudot est détecté | Permet de continuer à développer et à tester sans clavier flashé |
| O4 | Canal HID partagé avec VIA | Préfixe magique + numéro de version dans chaque trame, filtrés côté software | VIA utilise déjà l'interface HID brute de QMK |
| O5 | Anti-blocage (M8) | Signal de vie du software vers le clavier ; sans réponse, Var retombe en passthrough | Rend M8 vérifiable |
| O6 | Injection en v0.2 | `SendInput` (Windows) et événements Unicode (macOS) | TSF viendra en v0.3 (B1) |
| O7 | Machine à états | Extraite dans une crate `core` sans dépendance OS, alimentée par des événements Baudot | La même logique reçoit HID ou hook |
| O8 | Numérotation | v0.2 = "Var" (bêta), v1.0 après retours | Voir A3 |

## A - Ambiguïtés

| # | Ambiguïté | Statut |
|---|---|---|
| A1 | **Bêta : tous les testeurs sont sous macOS.** Ont-ils un Keychron flashé ? Sinon ils n'ont pas de Var et il n'y a pas de bêta | Non résolu. Seul point qui peut invalider le calendrier. Soit tu leur prêtes des claviers flashés, soit le mode hook (O3) est porté sur Mac pour la bêta |
| A2 | Modèle : **Keychron K3 Max** (QMK/VIA, fork Keychron, branche `wireless_playground`, dossier `keyboards/keychron/k3_max`). Son firmware expose-t-il l'HID brut en USB sans conflit avec VIA ? Le Bluetooth de QMK ne transporte pas l'HID brut, le dongle 2.4 GHz est non vérifié | [ASSUMED] : USB filaire uniquement. **Non vérifié sur le matériel : c'est le verrou n°1 de D3 (étape 0)** |
| A3 | "Grosse version" : v0.2 ou directement 1.0 ? | [ASSUMED] : v0.2, la 1.0 arrive après la bêta |
| A4 | Il faut encore **écrire** dans l'OS : l'injection est nécessaire même sans hook. Sur macOS elle demande la permission Accessibilité | Constat. M6 est tenu (aucune lecture de frappes), mais un parcours de permission subsiste sur Mac |
| A5 | Lecture HID brut sous macOS : permission "Surveillance de l'entrée" requise ou non pour une interface vendor-defined ? | [ASSUMED] non requise, à tester sur un vrai Mac |
| A6 | Les codes Baudot contiennent quand même l'identité de la touche pressée (position "E"). La différence avec un keylogger tient à : canal privé, uniquement pendant Var, pas de flux global. Est-ce la formulation que tu veux défendre publiquement ? | À valider (M11 : jamais de description de l'architecture dans le public, donc vaut surtout pour l'argument devant un fabricant ou un juriste) |
| A7 | Arrêt du software pendant que Var est tenu | Couvert par O5 |
| A8 | Statut juridique de Lockfree (non immatriculée) | Hors code |

## Amendement 1 (30 sept. 2026, décisions d'Arthus)

| # | Décision | Raison |
|---|---|---|
| AM1 | **Armement demandé par le software** : quand des diacritiques restent empilées au relâchement de Var (état Armed de v0.1), le software demande au clavier de lui transmettre la prochaine touche en code Baudot. Le clavier n'accepte la demande que pendant le maintien de Var ; l'armement se lève sur ordre du software ou au prochain appui de Var | M12 et M3 étaient contradictoires : v0.1 compose avec une lettre tapée APRÈS le relâchement d'AltGr, que le clavier ne pouvait plus transmettre. Proposition d'Arthus |
| AM2 | **Aucun délai d'expiration** sur l'armement | [STATED] "essentiel pour l'utilisation" : v0.1 reste armé indéfiniment |
| AM3 | **Le comportement de v0.1 (`main.rs`) et ses interactions avec le HUD font référence** : tout écart doit être justifié par une contrainte M, signalé et testé | [STATED] |
| AM4 | **ISO uniquement** | [STATED] |
| AM5 | Les Key ID sont des **positions** ; la traduction en touche logique passe par la disposition active de l'OS | Constaté : le poste d'Arthus tourne en disposition Windows US (seule disposition installée). v0.1 dépendait de la disposition OS (codes VK), un tableau AZERTY figé dans le firmware aurait changé le comportement. Garde aussi M5 (firmware sans aucune connaissance linguistique) |
| AM6 | Pendant Var, les touches d'édition et modificateurs (Entrée, Tab, Échap, Retour arrière, Ctrl, Alt gauche, Win, flèches, F1-F12...) passent aussi par le software, qui les restitue | En v0.1, ces touches validaient le cycle en cours (ou annulaient le sélecteur de langue) AVANT d'atteindre l'application. Les laisser filer directement vers l'OS inversait l'ordre (Entrée avant le "é"). Toujours limité au maintien de Var |

| AM7 | **Un seul bit « majuscule »** dans les trames, calculé par le clavier (Maj tenue XOR Verr. Maj. actif, relevé à l'appui de la touche), au lieu de deux drapeaux séparés | [STATED]. Le software n'a besoin que de ce résultat (même règle que v0.1) et en apprend moins sur la frappe |
| AM8 | **Compatibilité VIA volontairement ignorée** : le clavier Var n'a pas à rester utilisable avec VIA ou Keychron Launcher | [STATED]. Clôt le point (b) de l'étape 0 et la partie VIA de A2 / O4 |
| AM9 | Stockage des profils dans le firmware (B2 du tableau Bonus) : **bien plus tard** | [STATED] |
| AM14 | **Diacritiques natives sur les chiffres, dans toutes les langues** : en plus des lettres accentuées (é, è, ê, ë…), chaque profil propose ses propres diacritiques combinantes en mode Dead Keys, comme le pinyin, pour offrir deux façons de taper. Une diacritique par touche chiffre à partir de 1, sans Maj (section `min:` seulement), **dans l'ordre de leur fréquence dans les textes de la langue**, sans limite à 5 (français : 1 aigu, 2 grave, 3 circonflexe, 4 tréma, 5 cédille, la cédille sur 5 par choix d'Arthus). Chaque langue ne reçoit que les accents qu'elle utilise. Pinyin et vietnamien gardent leur propre schéma ; l'anglais n'a pas d'accent. Un premier essai par familles d'accents (même touche pour la même famille dans toutes les langues) a été écarté par Arthus | [STATED] |
| AM13 | **Orientation produit (plus tard)** : la cible finale est une **seule petite puce, flashée et documentée, à intégrer dans les claviers** par les fabricants. Le K3 Max (QMK) et la devboard CH559 ne servent qu'au prototypage. À traiter dans une future version de D1, pas en v0.2 | [STATED] |
| AM12 | **L'application est en anglais** : tout texte visible (interface, menus, dialogues, messages d'erreur) est en anglais. Remplace la consigne D3 « écrire en français avec les accents dans tout texte destiné à l'utilisateur » ; la règle des traits d'union simples (jamais de tirets longs) est conservée. Les noms de langues restent des endonymes (« Français », « Deutsch »), comme dans tout sélecteur de langue. Documentation interne (DRAFT, README, commentaires) inchangée, en français | [STATED] |
| AM11 | **Noms définitifs** : les claviers s'appellent **claviers Baudot**, l'application **Baudot Companion App** (Omnikey reste le nom de code). Pied de page de l'app : « by » suivi du logo Lockfree ; icônes de l'app tirées du même logo. Identifiants internes inchangés (dossier AppData `com.lockfree.omnikey`, registre `HKCU\Software\Lockfree\Omnikey`) pour ne perdre ni profils ni préférences ; l'entrée de démarrage automatique est migrée vers le nouveau nom | [STATED] |
| AM10 | **Var uniquement** : l'app ne contient ni ne lance plus aucun hook clavier. Sans clavier Var, elle attend. Remplace O3 (hook conservé en source secondaire) ; Bonus B3 abandonné | [STATED], pour que l'app soit partageable : un hook global embarqué reste un keylogger technique, même inactif chez les possesseurs d'un clavier Var |

### Ambiguïtés ajoutées

| # | Ambiguïté | Statut |
|---|---|---|
| A9 | Côté macOS, la traduction position -> touche logique passera par la disposition active du Mac (UCKeyTranslate). La couche Mac du K3 Max place Var à la place de Cmd droite | [ASSUMED], à valider avec A1 |
| A10 | Touches tenues pendant Var puis restituées par le software : l'auto-répétition de l'OS ne s'applique pas aux touches injectées. Tenir une lettre sans variante pendant Var ne la répète donc plus | Écart mineur accepté provisoirement, à confirmer |
| A12 | Le K3 Max branché sur le poste se présente comme ANSI RGB (PID USB `0x0A30`), pas ISO (`0x0A31`) | **Résolu (30 sept. 2026)** : c'est un ANSI, clavier de dev d'Arthus. Cible produit inchangée (ISO, AM4). Le firmware fournit les deux keymaps ; la touche ANSI `\` partage le Key ID de la touche ISO `#` (même scancode PC `0x2B`) |
| A11 | Course : si Var est relâchée moins de quelques ms après la touche qui empile une diacritique, l'ordre d'armement peut arriver après le relâchement. La lettre suivante part alors directement à l'OS sans composition | [ASSUMED] négligeable (délai USB + traitement < 5 ms), à mesurer à l'étape 0 |

## Critères de sortie D1

- [x] Cinq catégories remplies
- [x] Ambiguïtés marquées avec statut
- [x] Points ouverts avec décision proposée
- [ ] **A1 à trancher avant D3**, A2 à vérifier sur ton Keychron avant de figer la trame
