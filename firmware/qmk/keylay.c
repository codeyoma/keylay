// KeyLay: reports the active layer to the KeyLay menu bar app over raw HID.
// See firmware/PROTOCOL.md. Paste this into keymap.c and add `RAW_ENABLE = yes` to the
// keymap's rules.mk. If VIA is enabled, rename raw_hid_receive to raw_hid_receive_kb.
#ifdef RAW_ENABLE
#    include "raw_hid.h"

#    define KEYLAY_MAGIC 0x4B
#    define KEYLAY_VERSION 0x01
#    define KEYLAY_MSG_STATE 0x01
#    define KEYLAY_MSG_GET_STATE 0x02
#    define KEYLAY_REPORT_LEN 32 // RAW_EPSIZE

static void keylay_send_state(layer_state_t state) {
    uint8_t  report[KEYLAY_REPORT_LEN] = {0};
    uint32_t mask               = (uint32_t)state;
    report[0]                   = KEYLAY_MAGIC;
    report[1]                   = KEYLAY_VERSION;
    report[2]                   = KEYLAY_MSG_STATE;
    report[3]                   = get_highest_layer(state);
    report[4]                   = mask & 0xFF;
    report[5]                   = (mask >> 8) & 0xFF;
    report[6]                   = (mask >> 16) & 0xFF;
    report[7]                   = (mask >> 24) & 0xFF;
    report[8]                   = 0; // no batteries on a wired keyboard
    raw_hid_send(report, sizeof(report));
}

// The app asks for the state when it connects.
void raw_hid_receive(uint8_t *data, uint8_t length) {
    if (length >= 3 && data[0] == KEYLAY_MAGIC && data[2] == KEYLAY_MSG_GET_STATE) {
        keylay_send_state(layer_state | default_layer_state);
    }
}

// Sends the state whenever it changes, after tri-layer and every other update settled.
void housekeeping_task_user(void) {
    static layer_state_t last_state = 0;
    static bool          sent       = false;
    if (!is_keyboard_master()) {
        return;
    }
    layer_state_t state = layer_state | default_layer_state;
    if (!sent || state != last_state) {
        last_state = state;
        sent       = true;
        keylay_send_state(state);
    }
}
#endif
