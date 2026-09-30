// Omnikey v0.2 "Var" - module firmware (D2 B2).
//
// Miroir C du modèle core/tests/common/firmware.rs, sur lequel tournent les
// tests d'équivalence avec v0.1 : toute modification ici doit y être
// répercutée (et inversement).
//
// Trois états :
//   repos : clavier normal, seul le signal de vie circule (M3).
//   tenu  : Var maintenue ; les touches de la table partent en trames au
//           lieu d'aller à l'OS (M4).
//   armé  : après le relâchement de Var, sur demande du software faite
//           PENDANT le maintien ; seules les positions du bloc
//           alphanumérique et Retour arrière sont redirigées. Pas de délai
//           d'expiration (AM2) : l'armement tombe sur SetArm(0), au prochain
//           appui de Var, ou si le software ne répond plus.

#include "var.h"
#include "raw_hid.h"
#include "baudot_protocol.h"

static bool     host_seen     = false; // au moins une réponse depuis le démarrage
static uint32_t last_ack      = 0;
static uint32_t last_heartbeat = 0;
static bool     held          = false;
static bool     armed         = false;
static bool     arm_requested = false;
static uint8_t  tx_seq        = 0;
// Positions dont l'appui a été redirigé : leur relâchement doit l'être
// aussi, quel que soit l'état, sinon l'OS recevrait un relâchement orphelin
// ou le software garderait une touche enfoncée.
static uint8_t redirected[(BAUDOT_KEY_MAX + 1 + 7) / 8];

static bool host_alive(void) {
    return host_seen && timer_elapsed32(last_ack) < VAR_HOST_TIMEOUT_MS;
}

static bool is_redirected(uint8_t id) {
    return redirected[id / 8] & (1 << (id % 8));
}

static void set_redirected(uint8_t id, bool on) {
    if (on) {
        redirected[id / 8] |= (1 << (id % 8));
    } else {
        redirected[id / 8] &= ~(1 << (id % 8));
    }
}

static uint8_t current_flags(void) {
    bool shift = (get_mods() | get_oneshot_mods()) & MOD_MASK_SHIFT;
    bool caps  = host_keyboard_led_state().caps_lock;
    return (shift != caps) ? BAUDOT_FLAG_UPPER : 0;
}

static void send_frame(uint8_t type, uint8_t key, uint8_t flags) {
    uint8_t frame[BAUDOT_FRAME_SIZE] = {0};
    frame[0] = BAUDOT_MAGIC;
    frame[1] = BAUDOT_VERSION;
    frame[2] = type;
    frame[3] = tx_seq++;
    frame[4] = key;
    frame[5] = flags;
    raw_hid_send(frame, sizeof(frame));
}

bool process_record_var(uint16_t keycode, keyrecord_t *record) {
    bool pressed = record->event.pressed;

    if (keycode == VAR) {
        if (pressed) {
            armed         = false;
            arm_requested = false;
            if (host_alive()) {
                held = true;
                send_frame(BAUDOT_VAR_DOWN, 0, 0);
            }
        } else if (held) {
            held = false;
            send_frame(BAUDOT_VAR_UP, 0, 0);
            armed         = arm_requested && host_alive();
            arm_requested = false;
        }
        return false; // Var n'émet jamais de scancode (M2)
    }

    uint8_t id = baudot_key_id(keycode);
    if (id == 0) {
        return true;
    }

    if (!pressed) {
        if (!is_redirected(id)) {
            return true;
        }
        set_redirected(id, false);
        send_frame(BAUDOT_KEY_UP, id, current_flags());
        return false;
    }

    if (held || (armed && baudot_key_armable(id))) {
        set_redirected(id, true);
        send_frame(BAUDOT_KEY_DOWN, id, current_flags());
        return false;
    }
    return true;
}

// Appelé par keychron_raw_hid.c (patch) pour toute commande que ni Keychron
// ni VIA ne reconnaissent. Retourne true si la trame est à nous : aucune
// réponse n'est alors renvoyée.
bool kc_raw_hid_rx_user(uint8_t *data, uint8_t length) {
    if (length < 6 || data[0] != BAUDOT_MAGIC) {
        return false;
    }
    if (data[1] != BAUDOT_VERSION) {
        return true; // trame Baudot d'une autre version : ignorée
    }
    switch (data[2]) {
        case BAUDOT_HEARTBEAT_ACK:
            host_seen = true;
            last_ack  = timer_read32();
            break;
        case BAUDOT_SET_ARM:
            if (data[4]) {
                // Une demande d'armement ne peut naître que pendant le
                // maintien de Var. Déjà armé : rien à faire.
                if (held && host_alive()) {
                    arm_requested = true;
                }
            } else {
                arm_requested = false;
                armed         = false;
            }
            break;
        default:
            break;
    }
    return true;
}

void var_task(void) {
    if (timer_elapsed32(last_heartbeat) >= VAR_HEARTBEAT_MS) {
        last_heartbeat = timer_read32();
        // Ni touche ni drapeaux : le signal de vie ne dit rien de la frappe.
        send_frame(BAUDOT_HEARTBEAT, 0, 0);
    }
    if (!host_alive()) {
        held          = false;
        armed         = false;
        arm_requested = false;
    }
}
