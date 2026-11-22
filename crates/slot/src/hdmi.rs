//! HDMI out. The display engine's output is switched by `System/hdmi.sh` before slot starts, so
//! a cable going into the HDMI port, or out of it, means slot starts again: [`Hotplug::poll`] says
//! when, and slot saves the game and runs the launcher again in its own place ([`relaunch`]),
//! which switches the output and starts a fresh slot that resumes the game.
//!
//! Ported from Cartridge (github.com/NukeBomB3000/cartridge), GPL-3.0-or-later.

use std::path::PathBuf;

use slot_gfx::hdmi_active;

use slot_input::Millis;

/// Where the TV's sound goes: the HDMI transmitter's ALSA card, as `hdmi.sh` routes it.
pub const HDMI_ROUTE: &str = "plughw:CARD=ahubhdmi,DEV=0";

/// Whether the HDMI transmitter has the output now.
pub fn active() -> bool {
    std::fs::read_to_string(OUTPUT).is_ok_and(|s| hdmi_active(&s))
}

/// Whether an audio route is the TV's.
pub fn is_hdmi_route(route: &str) -> bool {
    route.trim() == HDMI_ROUTE
}

/// Saves nothing itself: once the game is saved, replaces this process with
/// `System/launch_frontend.sh`, which runs `hdmi.sh sync` and starts slot on the other output.
/// The process keeps its place under BaseOS, whatever BaseOS does when a frontend ends. Every
/// descriptor but stdio is closed on the way, so the GPU, framebuffer and sound devices are free
/// for the new slot. Returns only if the launcher could not be run.
pub fn relaunch(root: &std::path::Path) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    let launcher = root.join("System/launch_frontend.sh");
    slot_power::close_on_exec();
    std::process::Command::new("/bin/sh").arg(launcher).exec()
}

const EXTCON: &str = "/sys/class/extcon/hdmi/state";
const OUTPUT: &str = "/sys/class/disp/disp/attr/sys";

/// How often the cable is looked at.
pub const HOTPLUG_POLL_MS: Millis = 1000;

/// How many looks in a row must see the cable changed: a connector that is being wiggled in
/// does not restart anything until it holds.
const HOLDS: u8 = 2;

/// Watches the cable against the state it had when slot started, which is the state
/// `hdmi.sh sync` put the output in. Comparing with the start rather than with the output means
/// an output the script could not switch (a kernel without the debugfs switch) restarts slot
/// once, not over and over: the next slot starts with the cable as it is.
pub struct Hotplug {
    extcon: PathBuf,
    /// The cable at start; `None` until first looked at, or on a handheld without the port.
    start: Option<bool>,
    at: Millis,
    changed_for: u8,
}

/// Whether the extcon state says a cable is in: `HDMI=1`.
pub fn cable_in(state: &str) -> bool {
    state
        .trim()
        .strip_prefix("HDMI=")
        .is_some_and(|v| v.trim() == "1")
}

impl Default for Hotplug {
    fn default() -> Self {
        Hotplug::at(EXTCON.into())
    }
}

impl Hotplug {
    pub fn at(extcon: PathBuf) -> Self {
        let start = std::fs::read_to_string(&extcon).ok().map(|s| cable_in(&s));
        Hotplug {
            extcon,
            start,
            at: 0,
            changed_for: 0,
        }
    }

    /// Looks at the cable if it is time, and says whether it has gone in or out since slot
    /// started. It keeps saying so until slot leaves, so a restart put off (a link session) comes
    /// once it can. A handheld with no HDMI port has no extcon file and never restarts.
    pub fn poll(&mut self, now: Millis) -> bool {
        if now < self.at {
            return false;
        }
        self.at = now + HOTPLUG_POLL_MS;
        let Some(start) = self.start else {
            return false;
        };
        let Ok(state) = std::fs::read_to_string(&self.extcon) else {
            return false;
        };
        if cable_in(&state) == start {
            self.changed_for = 0;
            return false;
        }
        self.changed_for = self.changed_for.saturating_add(1);
        self.changed_for >= HOLDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig(cable: &str) -> (tempfile::TempDir, Hotplug) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("extcon"), cable).unwrap();
        let watch = Hotplug::at(dir.path().join("extcon"));
        (dir, watch)
    }

    fn plug(dir: &tempfile::TempDir, cable: &str) {
        std::fs::write(dir.path().join("extcon"), cable).unwrap();
    }

    #[test]
    fn the_extcon_state_says_whether_a_cable_is_in() {
        assert!(cable_in("HDMI=1\n"));
        assert!(!cable_in("HDMI=0\n"));
        assert!(!cable_in(""));
    }

    #[test]
    fn a_cable_left_as_it_was_restarts_nothing() {
        for cable in ["HDMI=0\n", "HDMI=1\n"] {
            let (_dir, mut watch) = rig(cable);
            for t in 0..5 {
                assert!(!watch.poll(t * 1000));
            }
        }
    }

    #[test]
    fn a_cable_put_in_or_pulled_restarts_once_it_holds() {
        let (dir, mut watch) = rig("HDMI=0\n");
        assert!(!watch.poll(0));
        plug(&dir, "HDMI=1\n");
        assert!(!watch.poll(1000), "one look is a wiggle");
        assert!(!watch.poll(1500), "not looked at again yet");
        assert!(watch.poll(2000));
        assert!(watch.poll(3000), "still due while a restart is put off");
        let (dir, mut watch) = rig("HDMI=1\n");
        plug(&dir, "HDMI=0\n");
        assert!(!watch.poll(0));
        assert!(watch.poll(1000), "a cable pulled out too");
    }

    #[test]
    fn a_wiggle_that_settles_resets_the_count() {
        let (dir, mut watch) = rig("HDMI=0\n");
        plug(&dir, "HDMI=1\n");
        assert!(!watch.poll(0));
        plug(&dir, "HDMI=0\n");
        assert!(!watch.poll(1000));
        plug(&dir, "HDMI=1\n");
        assert!(!watch.poll(2000));
    }

    #[test]
    fn an_output_that_did_not_switch_does_not_loop() {
        // the cable is in but hdmi.sh could not switch: the new slot starts with the cable in
        // and leaves it be
        let (_dir, mut watch) = rig("HDMI=1\n");
        for t in 0..60 {
            assert!(!watch.poll(t * 1000));
        }
    }

    #[test]
    fn a_handheld_without_the_port_never_restarts() {
        let mut watch = Hotplug::at("/nonexistent/extcon".into());
        for t in 0..4 {
            assert!(!watch.poll(t * 1000));
        }
    }
}
