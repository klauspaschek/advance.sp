//! Bluetooth → Test buttons: every button lights up while held and none acts, until B is held.

mod common;

use slot::session::Session;
use slot_input::{Btn, RawEvent};
use slot_ui::QuickRow;

fn tap(s: &mut Session, time: &mut u64, button: Btn) {
    *time += 20;
    s.feed([RawEvent::Down(button)], *time);
    *time += 20;
    s.feed([RawEvent::Up(button)], *time);
}

fn rows(s: &Session) -> Vec<String> {
    s.app().bluetooth.as_ref().unwrap().lines()
}

/// Opens the Bluetooth tile from the carousel and goes to Test buttons.
fn checker() -> (tempfile::TempDir, Session, u64) {
    let d = common::tmp_root_with_carts(&[]);
    slot_store::write_slot_state(d.path(), &slot_store::SlotState { clock_set: true, ..Default::default() })
        .expect("write slot.state");
    let mut s = Session::boot(d.path().to_owned());
    let mut time = 1000;
    s.app_mut().apply(slot_input::Action::QuickMenu);
    for b in [Btn::Right, Btn::Right, Btn::Down] {
        tap(&mut s, &mut time, b);
    }
    assert_eq!(s.app().quick_menu(), Some(QuickRow::Radio), "phase {:?}", s.app().phase());
    tap(&mut s, &mut time, Btn::A);
    let menu = s.app().bluetooth.as_ref().expect("the Bluetooth tile opened");
    let at = menu.rows().iter().position(|r| r == "Test buttons").expect("a Test buttons row");
    for _ in 0..at {
        tap(&mut s, &mut time, Btn::Down);
    }
    tap(&mut s, &mut time, Btn::A);
    assert!(s.app().testing_buttons());
    (d, s, time)
}

#[test]
fn the_checker_opens_from_the_bluetooth_tile_with_every_group_idle() {
    let (_d, s, _) = checker();
    let lines = rows(&s);
    assert_eq!(lines[0], "Test buttons");
    assert_eq!(lines[1], "Hold B to go back");
    for group in ["D-pad", "Face buttons", "Shoulders", "System", "Controller"] {
        assert!(lines.contains(&format!("ROW TEST {group}|–")), "{group}: {lines:?}");
    }
}

#[test]
fn a_held_button_lights_its_group_and_does_nothing_else() {
    let (_d, mut s, mut time) = checker();
    time += 20;
    s.feed([RawEvent::Down(Btn::Menu), RawEvent::Down(Btn::Start)], time);
    let lines = rows(&s);
    assert!(lines.contains(&"SELECTED TEST System|Start Menu".to_owned()), "{lines:?}");
    time += 2000;
    s.feed([], time);
    assert!(s.app().testing_buttons(), "a long MENU hold neither closed nor ejected anything");
    s.feed([RawEvent::Up(Btn::Menu), RawEvent::Up(Btn::Start)], time + 20);
    assert!(rows(&s).contains(&"ROW TEST System|–".to_owned()));
    assert_eq!(s.app().quick_menu(), Some(QuickRow::Radio), "MENU did not move the settings grid under the checker");
    for b in [Btn::A, Btn::Select, Btn::L2, Btn::Up] {
        tap(&mut s, &mut time, b);
    }
    assert!(s.app().testing_buttons());
}

#[test]
fn a_tap_of_b_is_tested_and_holding_b_goes_back() {
    let (_d, mut s, mut time) = checker();
    tap(&mut s, &mut time, Btn::B);
    assert!(s.app().testing_buttons(), "a tap of B is a button under test");
    time += 20;
    s.feed([RawEvent::Down(Btn::B)], time);
    assert!(rows(&s).contains(&"SELECTED TEST Face buttons|B".to_owned()));
    s.feed([], time + slot::bluetooth::TEST_LEAVE_MS - 1);
    assert!(s.app().testing_buttons());
    s.feed([], time + slot::bluetooth::TEST_LEAVE_MS);
    assert!(!s.app().testing_buttons(), "held B left the checker");
    let menu = s.app().bluetooth.as_ref().unwrap();
    assert_eq!(menu.rows()[menu.selected], "Test buttons", "back on its row");
}

#[test]
fn a_controllers_extra_buttons_show_in_their_own_row() {
    let (_d, mut s, _) = checker();
    s.app_mut().test_extra("Share", true);
    s.app_mut().test_extra("R↑", true);
    assert!(rows(&s).contains(&"SELECTED TEST Controller|Share RS up".to_owned()), "{:?}", rows(&s));
    s.app_mut().test_extra("Share", false);
    s.app_mut().test_extra("R↑", false);
    assert!(rows(&s).contains(&"ROW TEST Controller|–".to_owned()));
}
