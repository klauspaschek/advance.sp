//! The Bluetooth list's device icons: a controller has one, as headphones do.

use slot_ui::{sound_device_icon, Icon};

#[test]
fn a_controller_row_has_a_visible_icon_no_wider_than_the_headphones() {
    let ink = [255, 255, 255];
    let pad = sound_device_icon(Icon::Gamepad, ink);
    let phones = sound_device_icon(Icon::Headphones, ink);
    assert!(pad.w > 0 && pad.h > 0, "empty controller icon");
    assert!(pad.rgba.chunks_exact(4).any(|p| p[3] > 127), "no ink in the controller icon");
    assert!(pad.w <= phones.w + 1, "the controller icon is wider than the headphones: {} > {}", pad.w, phones.w);
    assert!(pad.h < phones.h, "a wide glyph is shorter than the headphones");
}

#[test]
fn the_other_device_icons_keep_the_headphones_height() {
    let ink = [255, 255, 255];
    let phones = sound_device_icon(Icon::Headphones, ink);
    for icon in [Icon::Bluetooth, Icon::Volume] {
        assert_eq!(sound_device_icon(icon, ink).h, phones.h, "{icon:?}");
    }
}
