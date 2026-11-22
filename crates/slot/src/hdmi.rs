//! HDMI out. The display engine's output is switched by `System/hdmi.sh` before slot starts, so
//! a cable going into the HDMI port, or out of it, means slot starts again: [`Hotplug::poll`] says
//! when, and slot saves the game and runs the launcher again in its own place ([`relaunch`]),
//! which switches the output and starts a fresh slot that resumes the game.
//!
//! Ported from Cartridge (github.com/NukeBomB3000/cartridge), GPL-3.0-or-later.

use std::path::{Path, PathBuf};

use slot_gfx::hdmi_active;
use slot_store::{Shader, SlotState};

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

/// Where the sound goes when no headphones have it: the TV's speakers while HDMI has the
/// picture, else the handheld's own (the empty route).
pub fn default_route() -> &'static str {
    if active() {
        HDMI_ROUTE
    } else {
        ""
    }
}

/// The handheld's own shaders, kept while slot is on a TV: the TV gets none, and the
/// handheld gets back what it had, whatever was chosen on the TV.
pub const LCD_SHADERS_FILE: &str = "Config/hdmi-lcd-shaders.txt";

/// Starting on a TV puts the handheld's shaders aside (once: a restart on the TV keeps the
/// first copy) and turns them off; starting on the handheld's screen puts them back. Says
/// whether `state` changed and should be saved. The copy is written before the shaders change
/// and removed only after they are back, so power lost in between loses neither.
pub fn swap_shaders(root: &Path, state: &mut SlotState, on_tv: bool) -> bool {
    let file = root.join(LCD_SHADERS_FILE);
    if on_tv {
        if !file.exists() {
            let kept = format!(
                "shader_gba={}\nshader_gb={}\nshader_gbc={}\n",
                state.shader_gba.name(),
                state.shader_gb.name(),
                state.shader_gbc.name()
            );
            if let Err(e) = slot_store::atomic_write(&file, kept.as_bytes()) {
                eprintln!("advance: hdmi: could not keep the handheld's shaders: {e}");
                return false;
            }
        }
        let off = [state.shader_gba, state.shader_gb, state.shader_gbc] != [Shader::Off; 3];
        state.shader_gba = Shader::Off;
        state.shader_gb = Shader::Off;
        state.shader_gbc = Shader::Off;
        return off;
    }
    let Ok(kept) = std::fs::read_to_string(&file) else {
        return false;
    };
    for line in kept.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(shader) = Shader::parse(value) else {
            continue;
        };
        match key.trim() {
            "shader_gba" if Shader::GBA.contains(&shader) => state.shader_gba = shader,
            "shader_gb" if Shader::GB.contains(&shader) => state.shader_gb = shader,
            "shader_gbc" if Shader::GB.contains(&shader) => state.shader_gbc = shader,
            _ => {}
        }
    }
    true
}

/// Removes the copy [`swap_shaders`] put back, once the state holding them is saved.
pub fn forget_kept_shaders(root: &Path) {
    let _ = std::fs::remove_file(root.join(LCD_SHADERS_FILE));
}

/// Saves nothing itself: once the game is saved, replaces this process with
/// `System/launch_frontend.sh`, which runs `hdmi.sh sync` and starts slot on the other output.
/// The process keeps its place under BaseOS, whatever BaseOS does when a frontend ends. Every
/// descriptor but stdio is closed on the way, so the GPU, framebuffer and sound devices are free
/// for the new slot. `left_tv` tells the new slot that the cable went out, so a lid shut over the
/// TV waits a moment ([`LEFT_TV`]). Returns only if the launcher could not be run.
pub fn relaunch(root: &std::path::Path, left_tv: bool) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    let launcher = root.join("System/launch_frontend.sh");
    slot_power::close_on_exec();
    let mut command = std::process::Command::new("/bin/sh");
    command.arg(launcher);
    if left_tv {
        command.env(LEFT_TV, "1");
    } else {
        command.env_remove(LEFT_TV);
    }
    command.exec()
}

/// Set for a slot started again because the HDMI cable went out.
pub const LEFT_TV: &str = "SLOT_LEFT_TV";

/// Whether this slot was started again because the HDMI cable went out.
pub fn left_tv() -> bool {
    std::env::var_os(LEFT_TV).is_some()
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
    fn the_tv_gets_no_shader_and_the_handheld_gets_its_own_back() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("Config")).unwrap();
        let mut state = SlotState::default();
        state.shader_gba = Shader::Grid;
        state.shader_gb = Shader::Simpletex;
        state.shader_gbc = Shader::Lcd3x;
        assert!(swap_shaders(root.path(), &mut state, true));
        assert_eq!(
            [state.shader_gba, state.shader_gb, state.shader_gbc],
            [Shader::Off; 3]
        );
        // a shader chosen on the TV, then a restart on the TV: the handheld's copy stays
        state.shader_gba = Shader::Dot;
        assert!(swap_shaders(root.path(), &mut state, true));
        assert!(!swap_shaders(root.path(), &mut state, true), "already off");
        // back on the handheld: its own shaders, whatever the TV had
        state.shader_gba = Shader::Dot;
        assert!(swap_shaders(root.path(), &mut state, false));
        assert_eq!(
            [state.shader_gba, state.shader_gb, state.shader_gbc],
            [Shader::Grid, Shader::Simpletex, Shader::Lcd3x]
        );
        assert!(
            root.path().join(LCD_SHADERS_FILE).exists(),
            "kept until the state is saved"
        );
        forget_kept_shaders(root.path());
        // on the handheld with nothing kept, nothing changes
        state.shader_gba = Shader::Dot;
        assert!(!swap_shaders(root.path(), &mut state, false));
        assert_eq!(state.shader_gba, Shader::Dot);
    }

    #[test]
    fn a_handheld_without_the_port_never_restarts() {
        let mut watch = Hotplug::at("/nonexistent/extcon".into());
        for t in 0..4 {
            assert!(!watch.poll(t * 1000));
        }
    }
}
