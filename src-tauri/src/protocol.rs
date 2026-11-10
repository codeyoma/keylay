//! Wire format shared with the firmware hooks. See `firmware/PROTOCOL.md`.
//!
//! A state message is the same bytes over USB raw HID (padded to 32 bytes) and over the
//! Bluetooth GATT characteristic:
//!
//! | offset | size | field                                                        |
//! |--------|------|--------------------------------------------------------------|
//! | 0      | 1    | magic `0x4B`                                                 |
//! | 1      | 1    | protocol version (1)                                         |
//! | 2      | 1    | message type, `0x01` = state                                 |
//! | 3      | 1    | highest active layer index                                   |
//! | 4      | 4    | active layer bitmask, little endian                          |
//! | 8      | 1    | battery count N                                              |
//! | 9      | N    | battery levels 0-100, `0xFF` = unknown; 0 = the keyboard (or |
//! |        |      | split central), 1.. = split peripherals                      |

use uuid::Uuid;

pub const MAGIC: u8 = 0x4B;
pub const VERSION: u8 = 0x01;
pub const MSG_STATE: u8 = 0x01;
pub const MSG_GET_STATE: u8 = 0x02;

const HEADER_LEN: usize = 9;
const MAX_BATTERIES: usize = 8;

/// QMK's default raw HID usage page and usage.
pub const RAW_HID_USAGE_PAGE: u16 = 0xFF60;
pub const RAW_HID_USAGE: u16 = 0x61;
/// QMK's raw HID report size, without the report ID.
pub const RAW_HID_REPORT_LEN: usize = 32;

/// GATT service and characteristic of the ZMK module (`firmware/zmk`).
pub const BLE_SERVICE: Uuid = Uuid::from_u128(0x9f3c0001_5b2e_4a6d_8f1c_3e7a2b9d4c60);
pub const BLE_STATE_CHARACTERISTIC: Uuid = Uuid::from_u128(0x9f3c0002_5b2e_4a6d_8f1c_3e7a2b9d4c60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub layer: u8,
    pub layer_mask: u32,
    pub batteries: Vec<Option<u8>>,
}

/// Parses a state message. Fields may be appended in later protocol versions, so a newer
/// version with the same header still parses.
pub fn parse(buf: &[u8]) -> Option<Report> {
    if buf.len() < HEADER_LEN || buf[0] != MAGIC || buf[1] == 0 || buf[2] != MSG_STATE {
        return None;
    }
    let count = (buf[8] as usize)
        .min(MAX_BATTERIES)
        .min(buf.len() - HEADER_LEN);
    let batteries = buf[HEADER_LEN..HEADER_LEN + count]
        .iter()
        .map(|&level| (level <= 100).then_some(level))
        .collect();
    Some(Report {
        layer: buf[3],
        layer_mask: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
        batteries,
    })
}

/// A raw HID output report that asks the keyboard for its state. The first byte is the
/// report ID (0), which hidapi strips before sending.
pub fn get_state_request() -> [u8; RAW_HID_REPORT_LEN + 1] {
    let mut report = [0u8; RAW_HID_REPORT_LEN + 1];
    report[1] = MAGIC;
    report[2] = VERSION;
    report[3] = MSG_GET_STATE;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_split_keyboard_state() {
        let msg = [0x4B, 1, 1, 3, 0b1001, 0, 0, 0, 3, 87, 0xFF, 100];
        assert_eq!(
            parse(&msg),
            Some(Report { layer: 3, layer_mask: 0b1001, batteries: vec![Some(87), None, Some(100)] })
        );
    }

    #[test]
    fn parses_a_padded_qmk_report_without_batteries() {
        let mut msg = [0u8; 32];
        msg[..9].copy_from_slice(&[0x4B, 1, 1, 2, 0b101, 0, 0, 0, 0]);
        assert_eq!(parse(&msg), Some(Report { layer: 2, layer_mask: 0b101, batteries: vec![] }));
    }

    #[test]
    fn caps_the_battery_count_at_the_message_length() {
        let msg = [0x4B, 1, 1, 0, 1, 0, 0, 0, 5, 50];
        assert_eq!(parse(&msg).unwrap().batteries, vec![Some(50)]);
    }

    #[test]
    fn rejects_other_reports() {
        assert_eq!(parse(&[0xFF, 1, 1, 0, 0, 0, 0, 0, 0]), None); // VIA "unhandled" echo
        assert_eq!(parse(&[0x4B, 1, 2, 0, 0, 0, 0, 0, 0]), None); // a request, not a state
        assert_eq!(parse(&[0x4B, 1, 1]), None);
    }

    #[test]
    fn request_has_report_id_and_header() {
        let req = get_state_request();
        assert_eq!(req.len(), 33);
        assert_eq!(&req[..4], &[0, MAGIC, VERSION, MSG_GET_STATE]);
    }
}
