// Omnikey v0.2 "Var" - protocole Baudot (D2 B1).
//
// Doit rester identique à core/src/protocol.rs et core/src/key.rs.
//
// Trame de 32 octets, dans les deux sens :
//   [0] préfixe 0xBD  [1] version  [2] type  [3] séquence
//   [4] Key ID ou argument  [5] drapeaux (bit0 majuscule = Maj XOR Verr. Maj.)
//   [6..31] réservés, à zéro
//
// Le Key ID désigne une POSITION physique ISO, jamais un caractère : le
// clavier ne connaît ni langue ni disposition (D1 M5, AM5).

#pragma once

#include <stdbool.h>
#include <stdint.h>

#define BAUDOT_FRAME_SIZE 32
#define BAUDOT_MAGIC 0xBD
#define BAUDOT_VERSION 1

// Maj tenue XOR Verr. Maj. actif (D1 AM7) : seul ce résultat quitte le clavier.
#define BAUDOT_FLAG_UPPER (1 << 0)

// Clavier -> software
#define BAUDOT_VAR_DOWN 0x01
#define BAUDOT_VAR_UP 0x02
#define BAUDOT_KEY_DOWN 0x03
#define BAUDOT_KEY_UP 0x04
#define BAUDOT_HEARTBEAT 0x05
// Software -> clavier
#define BAUDOT_HEARTBEAT_ACK 0x81
#define BAUDOT_SET_ARM 0x82

#define BAUDOT_KEY_BACKSPACE 50
#define BAUDOT_KEY_MAX 78

// Position QMK (keycode de la couche de base) -> Baudot Key ID, 0 si la
// touche n'est pas dans la table. Maj et Verr. Maj. n'y sont jamais : ils
// passent toujours directement à l'OS, seul le bit majuscule voyage.
static inline uint8_t baudot_key_id(uint16_t keycode) {
    switch (keycode) {
        // Rangée des chiffres
        case KC_GRV:  return 1;
        case KC_1:    return 2;
        case KC_2:    return 3;
        case KC_3:    return 4;
        case KC_4:    return 5;
        case KC_5:    return 6;
        case KC_6:    return 7;
        case KC_7:    return 8;
        case KC_8:    return 9;
        case KC_9:    return 10;
        case KC_0:    return 11;
        case KC_MINS: return 12;
        case KC_EQL:  return 13;
        // Rangée du haut
        case KC_Q:    return 14;
        case KC_W:    return 15;
        case KC_E:    return 16;
        case KC_R:    return 17;
        case KC_T:    return 18;
        case KC_Y:    return 19;
        case KC_U:    return 20;
        case KC_I:    return 21;
        case KC_O:    return 22;
        case KC_P:    return 23;
        case KC_LBRC: return 24;
        case KC_RBRC: return 25;
        // Rangée du milieu
        case KC_A:    return 26;
        case KC_S:    return 27;
        case KC_D:    return 28;
        case KC_F:    return 29;
        case KC_G:    return 30;
        case KC_H:    return 31;
        case KC_J:    return 32;
        case KC_K:    return 33;
        case KC_L:    return 34;
        case KC_SCLN: return 35;
        case KC_QUOT: return 36;
        // ISO "#" et ANSI "\" : même position logique (scancode PC 0x2B).
        case KC_NUHS: return 37;
        case KC_BSLS: return 37;
        // Rangée du bas
        case KC_NUBS: return 38;
        case KC_Z:    return 39;
        case KC_X:    return 40;
        case KC_C:    return 41;
        case KC_V:    return 42;
        case KC_B:    return 43;
        case KC_N:    return 44;
        case KC_M:    return 45;
        case KC_COMM: return 46;
        case KC_DOT:  return 47;
        case KC_SLSH: return 48;
        // Espace, édition, navigation
        case KC_SPC:  return 49;
        case KC_BSPC: return 50;
        case KC_ENT:  return 51;
        case KC_TAB:  return 52;
        case KC_ESC:  return 53;
        case KC_DEL:  return 54;
        case KC_PGUP: return 55;
        case KC_PGDN: return 56;
        case KC_HOME: return 57;
        case KC_END:  return 58;
        case KC_UP:   return 59;
        case KC_DOWN: return 60;
        case KC_LEFT: return 61;
        case KC_RGHT: return 62;
        // Modificateurs
        case KC_LCTL: return 63;
        case KC_RCTL: return 64;
        case KC_LGUI: return 65;
        case KC_LALT: return 66;
        // F1-F12
        case KC_F1 ... KC_F12: return 67 + (keycode - KC_F1);
        default:      return 0;
    }
}

// Positions redirigées pendant l'armement : bloc alphanumérique (1-48) et
// Retour arrière. Identique à KeyId::is_armable() côté software.
static inline bool baudot_key_armable(uint8_t id) {
    return (id >= 1 && id <= 48) || id == BAUDOT_KEY_BACKSPACE;
}
