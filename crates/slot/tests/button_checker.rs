//! Device controls → Test buttons: every button lights up while held and none acts, until B is
//! held. Device controls → In-Game → Controller layout: greyed out without a controller.

mod common;

use slot::session::Session;
use slot_input::{Btn, RawEvent};
use slot_ui::{QuickRow, QuickValue};

fn tap(s: &mut Session, time: &mut u64, button: Btn) {
    *time += 20;
    s.feed([RawEvent::Down(button)], *time);
    *time += 20;
    s.feed([RawEvent::Up(button)], *time);
}

fn held(s: &Session) -> Vec<&'static str> {
    s.app().bluetooth.as_ref().unwrap().test_held().to_vec()
}

fn session() -> (tempfile::TempDir, Session, u64) {
    let d = common::tmp_root_with_carts(&[]);
    slot_store::write_slot_state(d.path(), &slot_store::SlotState { clock_set: true, ..Default::default() })
        .expect("write advance.state");
    let s = Session::boot(d.path().to_owned());
    (d, s, 1000)
}

/// From the main menu, moves through the tiles with `path`, opens one with A, then moves down
/// to `row`.
fn open_row(s: &mut Session, time: &mut u64, path: &[Btn], row: QuickRow) {
    s.app_mut().apply(slot_input::Action::QuickMenu);
    for &b in path {
        tap(s, time, b);
    }
    for _ in 0..12 {
        if s.app().quick_menu() == Some(row) {
            return;
        }
        tap(s, time, Btn::Down);
    }
    panic!("{row:?} not found, at {:?}", s.app().quick_menu());
}

/// Opens Device controls → Test buttons.
fn checker() -> (tempfile::TempDir, Session, u64) {
    let (d, mut s, mut time) = session();
    // Device controls: second row, second tile
    open_row(&mut s, &mut time, &[Btn::Down, Btn::Right, Btn::A], QuickRow::ButtonTest);
    tap(&mut s, &mut time, Btn::A);
    assert!(s.app().testing_buttons());
    (d, s, time)
}

#[test]
fn the_checker_opens_from_device_controls_with_nothing_held() {
    let (_d, s, _) = checker();
    assert!(held(&s).is_empty());
    let lines = s.app().bluetooth.as_ref().unwrap().lines();
    assert!(lines.iter().any(|l| l.contains("TESTVIEW 0 ")), "the handheld is drawn first: {lines:?}");
}

#[test]
fn a_held_button_lights_and_does_nothing_else() {
    let (_d, mut s, mut time) = checker();
    time += 20;
    s.feed([RawEvent::Down(Btn::Menu), RawEvent::Down(Btn::Start)], time);
    assert_eq!(held(&s), ["Menu", "Start"]);
    time += 2000;
    s.feed([], time);
    assert!(s.app().testing_buttons(), "a long MENU hold neither closed nor ejected anything");
    s.feed([RawEvent::Up(Btn::Menu), RawEvent::Up(Btn::Start)], time + 20);
    assert!(held(&s).is_empty());
    for b in [Btn::A, Btn::Select, Btn::L2, Btn::Up] {
        tap(&mut s, &mut time, b);
    }
    assert!(s.app().testing_buttons());
}

#[test]
fn a_tap_of_b_is_tested_and_holding_b_goes_back_to_device_controls() {
    let (_d, mut s, mut time) = checker();
    tap(&mut s, &mut time, Btn::B);
    assert!(s.app().testing_buttons(), "a tap of B is a button under test");
    time += 20;
    s.feed([RawEvent::Down(Btn::B)], time);
    assert_eq!(held(&s), ["B"]);
    s.feed([], time + slot::bluetooth::TEST_LEAVE_MS - 1);
    assert!(s.app().testing_buttons());
    s.feed([], time + slot::bluetooth::TEST_LEAVE_MS);
    assert!(!s.app().testing_buttons(), "held B left the checker");
    assert!(s.app().bluetooth.is_none(), "the checker closed");
    assert_eq!(s.app().quick_menu(), Some(QuickRow::ButtonTest), "back on its row in Device controls");
}

#[test]
fn a_controller_pressed_last_is_drawn_instead_of_the_handheld() {
    let (_d, mut s, _) = checker();
    s.app_mut().set_controllers(true, true);
    s.app_mut().test_extra("Share", true);
    s.app_mut().test_extra("R↑", true);
    assert_eq!(held(&s), ["Share", "RS up"]);
    let lines = s.app().bluetooth.as_ref().unwrap().lines();
    assert!(lines.iter().any(|l| l.contains("TESTVIEW 1 1 Share,RS up")), "{lines:?}");
    s.app_mut().set_controllers(false, true);
    let lines = s.app().bluetooth.as_ref().unwrap().lines();
    assert!(lines.iter().any(|l| l.contains("TESTVIEW 0 ")), "no controller, no controller drawing");
}

#[test]
fn the_controller_layout_is_empty_without_a_controller_and_toggles_with_one() {
    let (d, mut s, mut time) = session();
    // Device controls → In-Game, whose first row it is
    open_row(&mut s, &mut time, &[Btn::Down, Btn::Right, Btn::A, Btn::Down, Btn::Down, Btn::Down, Btn::A], QuickRow::PadLayout);
    assert_eq!(s.app().quick_value(QuickRow::PadLayout), None, "no value without a controller");
    tap(&mut s, &mut time, Btn::Right);
    assert!(s.app().pad_faces_by_position(), "nothing changes without a controller");
    s.app_mut().set_controllers(true, false);
    assert_eq!(s.app().quick_value(QuickRow::PadLayout), Some(QuickValue::Nintendo), "Nintendo by default");
    tap(&mut s, &mut time, Btn::Right);
    assert!(!s.app().pad_faces_by_position());
    assert_eq!(s.app().quick_value(QuickRow::PadLayout), Some(QuickValue::Xbox));
    assert!(!slot::bluetooth::pad_faces_by_position(d.path()), "saved");
}
