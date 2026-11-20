//! External Bluetooth gamepads: told apart from the handheld's own buttons, read on Linux's
//! standard gamepad layout, and released when they disconnect.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use slot::input::evdev::{
    is_gamepad, layout_of, pad_code_to_btn, to_raw_for, trigger_axes, Ev, FaceButtons, Hat,
    Layout, PadAxes, Range, ABS_BRAKE, ABS_GAS, ABS_RZ, ABS_X, ABS_Z, EVENT_BYTES, EV_ABS, EV_KEY,
};
use slot_input::{Btn, InputSource, RawEvent};

/// BTN_SOUTH (0x130) is bit 48 of the fifth word from the end.
const PAD_KEYS: &str = "7fff000000000000 0 0 0 0\n";
/// X, Y, Z, RZ, GAS, BRAKE and the hat: an Xbox controller over Bluetooth LE.
const XBOX_ABS: &str = "30627\n";

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn packet(kind: u16, code: u16, value: i32) -> [u8; EVENT_BYTES] {
    let mut b = [0u8; EVENT_BYTES];
    b[16..18].copy_from_slice(&kind.to_le_bytes());
    b[18..20].copy_from_slice(&code.to_le_bytes());
    b[20..24].copy_from_slice(&value.to_le_bytes());
    b
}

fn key(code: u16, value: i32) -> Ev {
    Ev {
        kind: EV_KEY,
        code,
        value,
    }
}

fn abs(code: u16, value: i32) -> Ev {
    Ev {
        kind: EV_ABS,
        code,
        value,
    }
}

/// event0 is the handheld's gpio keys, event3 an Xbox pad on the Bluetooth bus.
fn tree() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let sys = d.path().join("sys/class/input");
    write(&d.path().join("dev/input/event0"), "");
    write(&sys.join("event0/device/name"), "gpio-keys\n");
    write(&sys.join("event0/device/id/bustype"), "0019\n");
    write(&sys.join("event0/device/capabilities/key"), PAD_KEYS);
    write(&d.path().join("dev/input/event3"), "");
    write(&sys.join("event3/device/name"), "Xbox Wireless Controller\n");
    write(&sys.join("event3/device/id/bustype"), "0005\n");
    write(&sys.join("event3/device/capabilities/key"), PAD_KEYS);
    write(&sys.join("event3/device/capabilities/abs"), XBOX_ABS);
    d
}

#[test]
fn a_bluetooth_node_is_a_pad_and_the_handhelds_own_is_not() {
    let d = tree();
    let sys = d.path().join("sys/class/input");
    assert_eq!(layout_of(&sys, Path::new("/dev/input/event3")), Layout::Pad);
    assert_eq!(
        layout_of(&sys, Path::new("/dev/input/event0")),
        Layout::Handheld
    );
    assert!(is_gamepad(&sys, Path::new("/dev/input/event3")));
}

#[test]
fn headphone_media_keys_are_not_a_gamepad() {
    let d = tempfile::tempdir().unwrap();
    let sys = d.path().join("sys/class/input");
    // only the volume keys, 114 and 115: bit 50 and 51 of the second word from the end
    write(
        &sys.join("event4/device/capabilities/key"),
        "c000000000000 0\n",
    );
    assert!(!is_gamepad(&sys, Path::new("/dev/input/event4")));
}

#[test]
fn face_buttons_follow_their_labels_by_default() {
    let labels = FaceButtons::default();
    assert_eq!(pad_code_to_btn(0x130, labels), Some(Btn::A));
    assert_eq!(pad_code_to_btn(0x131, labels), Some(Btn::B));
    assert_eq!(pad_code_to_btn(0x133, labels), Some(Btn::X));
    assert_eq!(pad_code_to_btn(0x134, labels), Some(Btn::Y));
}

#[test]
fn by_position_the_east_button_is_a() {
    let at = FaceButtons::Positions;
    assert_eq!(pad_code_to_btn(0x131, at), Some(Btn::A));
    assert_eq!(pad_code_to_btn(0x130, at), Some(Btn::B));
}

#[test]
fn shoulders_view_menu_and_guide_map_to_the_handhelds_buttons() {
    let f = FaceButtons::Labels;
    assert_eq!(pad_code_to_btn(0x136, f), Some(Btn::L1));
    assert_eq!(pad_code_to_btn(0x137, f), Some(Btn::R1));
    assert_eq!(pad_code_to_btn(0x13a, f), Some(Btn::Select));
    assert_eq!(pad_code_to_btn(158, f), Some(Btn::Select), "View as KEY_BACK");
    assert_eq!(pad_code_to_btn(0x13b, f), Some(Btn::Start));
    assert_eq!(pad_code_to_btn(0x13c, f), Some(Btn::Menu), "the Xbox button");
}

#[test]
fn the_handhelds_own_codes_keep_their_own_table() {
    // 0x134 is L1 on the handheld, Y on a pad
    assert_eq!(
        to_raw_for(Layout::Handheld, FaceButtons::Labels, key(0x134, 1)),
        Some(RawEvent::Down(Btn::L1))
    );
    assert_eq!(
        to_raw_for(Layout::Pad, FaceButtons::Labels, key(0x134, 1)),
        Some(RawEvent::Down(Btn::Y))
    );
    assert_eq!(
        to_raw_for(Layout::Pad, FaceButtons::Labels, key(0x130, 2)),
        None,
        "autorepeat is not a press"
    );
}

#[test]
fn an_le_pad_has_its_triggers_on_brake_and_gas() {
    let d = tree();
    let sys = d.path().join("sys/class/input");
    assert_eq!(
        trigger_axes(&sys, Path::new("/dev/input/event3")),
        [ABS_BRAKE, ABS_GAS]
    );
    write(&sys.join("event3/device/capabilities/abs"), "3003f\n");
    assert_eq!(
        trigger_axes(&sys, Path::new("/dev/input/event3")),
        [ABS_Z, ABS_RZ]
    );
}

fn xbox_axes() -> PadAxes {
    let stick = Range {
        min: 0,
        max: 65535,
    };
    let trigger = Range { min: 0, max: 1023 };
    PadAxes::new([stick, stick], [ABS_BRAKE, ABS_GAS], [trigger, trigger])
}

#[test]
fn the_left_stick_steers_the_d_pad_from_its_own_range() {
    let mut axes = xbox_axes();
    let mut hat = Hat::default();
    hat.steer_with_stick(true);
    assert!(axes.feed(&mut hat, abs(ABS_X, 32768)).is_empty(), "centred");
    assert!(axes.feed(&mut hat, abs(ABS_X, 40000)).is_empty(), "a nudge");
    assert_eq!(
        axes.feed(&mut hat, abs(ABS_X, 65535)),
        vec![RawEvent::Down(Btn::Right)]
    );
    assert_eq!(
        axes.feed(&mut hat, abs(ABS_X, 32768)),
        vec![RawEvent::Up(Btn::Right)]
    );
}

#[test]
fn the_right_stick_presses_nothing() {
    let mut axes = xbox_axes();
    let mut hat = Hat::default();
    hat.steer_with_stick(true);
    assert!(axes.feed(&mut hat, abs(ABS_Z, 65535)).is_empty());
    assert!(axes.feed(&mut hat, abs(ABS_RZ, 0)).is_empty());
}

#[test]
fn a_trigger_past_half_its_travel_presses_l2_or_r2() {
    let mut axes = xbox_axes();
    let mut hat = Hat::default();
    assert!(axes.feed(&mut hat, abs(ABS_BRAKE, 300)).is_empty());
    assert_eq!(
        axes.feed(&mut hat, abs(ABS_BRAKE, 800)),
        vec![RawEvent::Down(Btn::L2)]
    );
    assert!(axes.feed(&mut hat, abs(ABS_BRAKE, 1023)).is_empty());
    assert_eq!(
        axes.feed(&mut hat, abs(ABS_BRAKE, 0)),
        vec![RawEvent::Up(Btn::L2)]
    );
    assert_eq!(
        axes.feed(&mut hat, abs(ABS_GAS, 1023)),
        vec![RawEvent::Down(Btn::R2)]
    );
}

#[test]
fn the_hat_is_the_pads_d_pad() {
    let mut axes = xbox_axes();
    let mut hat = Hat::default();
    assert_eq!(
        axes.feed(&mut hat, abs(0x11, -1)),
        vec![RawEvent::Down(Btn::Up)]
    );
}

#[test]
fn a_pad_that_disconnects_mid_press_lets_its_buttons_go() {
    let d = tree();
    // the pad's node holds A pressed, then ends as a disconnected device does
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&packet(EV_KEY, 0x130, 1));
    bytes.extend_from_slice(&packet(0, 0, 0));
    fs::write(d.path().join("dev/input/event3"), bytes).unwrap();
    let card = tempfile::tempdir().unwrap();
    let mut input = slot::input::DeviceInput::open_in(
        &d.path().join("dev/input"),
        &d.path().join("sys/class/input"),
        card.path(),
        false,
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut seen = Vec::new();
    while seen.len() < 2 && Instant::now() < deadline {
        seen.extend(input.poll(0));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        &seen[..2],
        &[RawEvent::Down(Btn::A), RawEvent::Up(Btn::A)],
        "{seen:?}"
    );
}
