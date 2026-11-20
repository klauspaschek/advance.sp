//! The Bluetooth list's device icons: a controller has one, as headphones do.

use slot_ui::{sound_device_icon, Icon};

#[test]
fn a_controller_row_has_a_visible_icon_as_tall_as_the_headphones() {
    let ink = [255, 255, 255];
    let pad = sound_device_icon(Icon::Gamepad, ink);
    let phones = sound_device_icon(Icon::Headphones, ink);
    assert!(pad.w > 0 && pad.h > 0, "empty controller icon");
    assert!(pad.rgba.chunks_exact(4).any(|p| p[3] > 127), "no ink in the controller icon");
    assert_eq!(pad.h, phones.h);
}
