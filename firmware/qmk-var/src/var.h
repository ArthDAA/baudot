// Omnikey v0.2 "Var" - module firmware (D2 B2).
#pragma once

#include QMK_KEYBOARD_H

// Touche Var : remplace Alt droite. Ne produit jamais de scancode (M2).
enum var_keycodes {
    VAR = QK_USER_0,
};

// Délais du signal de vie. Sans réponse du software pendant
// VAR_HOST_TIMEOUT_MS, Var devient neutre et rien n'est plus redirigé (M8).
#ifndef VAR_HEARTBEAT_MS
#    define VAR_HEARTBEAT_MS 1000
#endif
#ifndef VAR_HOST_TIMEOUT_MS
#    define VAR_HOST_TIMEOUT_MS 3000
#endif

// À appeler en tête de process_record_user(). Retourne false si l'événement
// est absorbé (ne doit pas atteindre l'OS).
bool process_record_var(uint16_t keycode, keyrecord_t *record);

// À appeler depuis housekeeping_task_user().
void var_task(void);
