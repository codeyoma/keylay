//! The menu bar title: the layer name cut or padded to a fixed width, and the lowest battery
//! in 4 characters, so the title keeps its width as layers change and batteries drain. The
//! settings window previews the title through [`preview`], so there is one set of rules.

use crate::hub::{BatteryView, DeviceView, Transport};

/// Padding that survives as width at the end of the title (a plain space may not), and that
/// the space cleanup below leaves alone.
const PAD: char = '\u{00A0}';

pub fn title(format: &str, layer_width: usize, device: &crate::hub::DeviceView) -> String {
    let layer = device.layer_name.as_deref().map(|l| fit(l, layer_width)).unwrap_or_default();
    // Always 4 characters ("  5%", " 62%", "100%"), so the width holds as the battery drains.
    let battery = device
        .lowest_battery()
        .map(|b| format!("{b:>3}%").replace(' ', &PAD.to_string()))
        .unwrap_or_default();
    // A keyboard without layer reports shows its name, so the title is not just "87%".
    let format = if device.layer.is_none() && !format.contains("{keyboard}") {
        format!("{{keyboard}} {format}")
    } else {
        format.to_string()
    };
    format
        .replace("{keyboard}", device.display_name())
        .replace("{layer}", &layer)
        .replace("{battery}", &battery)
        .split(' ')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Cuts or pads a layer name to `width` characters: "Lower" becomes "Lower ", "Colemak"
/// becomes "Colem…". With the monospaced menu bar font that is a fixed width.
fn fit(name: &str, width: usize) -> String {
    if width == 0 {
        return name.to_string();
    }
    let len = name.chars().count();
    if len > width {
        name.chars().take(width - 1).chain(std::iter::once('…')).collect()
    } else {
        name.chars().chain(std::iter::repeat_n(PAD, width - len)).collect()
    }
}

/// The title for the settings preview: the shown keyboard's, or a sample one when no keyboard
/// is connected.
pub fn preview(format: &str, layer_width: usize, device: Option<&DeviceView>) -> String {
    let sample;
    let device = match device {
        Some(d) => d,
        None => {
            sample = DeviceView {
                id: String::new(),
                key: "Sweep".into(),
                name: "Sweep".into(),
                transport: Transport::Ble,
                keyboard: None,
                layer: Some(1),
                layer_name: Some("Lower".into()),
                layer_mask: 0b10,
                batteries: vec![BatteryView { label: "Battery".into(), level: Some(62) }],
                status: String::new(),
            };
            &sample
        }
    };
    title(format, layer_width, device)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::Transport;

    fn device(layer: Option<(&str, u8)>, batteries: &[Option<u8>]) -> DeviceView {
        DeviceView {
            id: "x".into(),
            key: "Sweep".into(),
            name: "Sweep Dongle".into(),
            transport: Transport::Ble,
            keyboard: Some("Sweep".into()),
            layer: layer.map(|l| l.1),
            layer_name: layer.map(|l| l.0.to_string()),
            layer_mask: 1,
            batteries: batteries.iter().map(|&level| BatteryView { label: "b".into(), level }).collect(),
            status: String::new(),
        }
    }

    #[test]
    fn layer_names_take_six_characters() {
        assert_eq!(fit("Lower", 6), "Lower\u{a0}");
        assert_eq!(fit("Adjust", 6), "Adjust");
        assert_eq!(fit("Colemak", 6), "Colem…");
        assert_eq!(fit("Colemak", 0), "Colemak");
    }

    #[test]
    fn title_has_the_same_length_for_every_layer() {
        let fmt = "{layer} {battery}";
        let a = title(fmt, 6, &device(Some(("yoma", 0)), &[Some(100), Some(62)]));
        let b = title(fmt, 6, &device(Some(("Scroll", 8)), &[Some(5)]));
        assert_eq!(a, "yoma\u{a0}\u{a0} \u{a0}62%");
        assert_eq!(b, "Scroll \u{a0}\u{a0}5%");
        assert_eq!(a.chars().count(), b.chars().count());
    }

    #[test]
    fn title_without_battery_or_layer() {
        assert_eq!(title("{layer} {battery}", 6, &device(Some(("Base", 0)), &[])), "Base\u{a0}\u{a0}");
        assert_eq!(title("{layer} {battery}", 6, &device(None, &[Some(87)])), "Sweep \u{a0}87%");
    }

    #[test]
    fn preview_uses_a_sample_keyboard_when_none_is_connected() {
        assert_eq!(preview("{keyboard} {layer} {battery}", 6, None), "Sweep Lower\u{a0} \u{a0}62%");
        assert_eq!(preview("{layer}", 6, Some(&device(Some(("Num", 2)), &[]))), "Num\u{a0}\u{a0}\u{a0}");
    }
}
