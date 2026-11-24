use std::fmt;
use std::path::Path;

use crate::link::Link;
use crate::rumble::Rumble;

pub const GBA_W: u32 = 240;
pub const GBA_H: u32 = 160;

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub struct ButtonMask(pub u16);

impl ButtonMask {
    pub const B: u16 = 1 << 0;
    pub const Y: u16 = 1 << 1;
    pub const SELECT: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const DOWN: u16 = 1 << 5;
    pub const LEFT: u16 = 1 << 6;
    pub const RIGHT: u16 = 1 << 7;
    pub const A: u16 = 1 << 8;
    pub const X: u16 = 1 << 9;
    pub const L: u16 = 1 << 10;
    pub const R: u16 = 1 << 11;

    pub fn without_turbo(self) -> ButtonMask {
        ButtonMask(self.0 & !(Self::X | Self::Y))
    }

    pub fn turbo(self, frame: u32) -> ButtonMask {
        let mut mask = self.0 & !(Self::X | Self::Y);
        if (frame / 3).is_multiple_of(2) {
            if self.0 & Self::X != 0 {
                mask |= Self::A;
            }
            if self.0 & Self::Y != 0 {
                mask |= Self::B;
            }
        }
        ButtonMask(mask)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct AvInfo {
    pub fps: f64,
    pub sample_rate: f64,
}

#[derive(Debug)]
pub enum CoreError {
    Io(std::io::Error),
    Load(String),
    Unsupported(String),
    State(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Io(e) => write!(f, "io: {e}"),
            CoreError::Load(m) => write!(f, "load: {m}"),
            CoreError::Unsupported(m) => write!(f, "unsupported: {m}"),
            CoreError::State(m) => write!(f, "state: {m}"),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::Io(e)
    }
}

/// How a snapshot is arranged: three regions placed at RetroAchievements addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AchievementLayout {
    /// IWRAM at 0, EWRAM at 0x8000, save RAM at 0x48000 (RA console 5).
    Gba,
    /// The $0000-$FFFF bus, CGB work RAM banks 2-7 at 0x10000 and cartridge RAM
    /// banks 1-15 at 0x16000 (RA consoles 4 and 6).
    Gb,
}

impl AchievementLayout {
    /// Bytes a snapshot buffer needs; the largest layout decides the buffer size.
    pub const SNAPSHOT_SIZE: usize = 0x58000;

    /// RetroAchievements address of each region's first byte.
    pub const fn starts(self) -> [usize; 3] {
        match self {
            AchievementLayout::Gba => [0, 0x8000, 0x48000],
            AchievementLayout::Gb => [0, 0x10000, 0x16000],
        }
    }

    /// Largest size of each region.
    pub const fn capacity(self) -> [usize; 3] {
        match self {
            AchievementLayout::Gba => [0x8000, 0x40000, 0x10000],
            AchievementLayout::Gb => [0x10000, 0x6000, 0x1e000],
        }
    }

    /// Work RAM must be complete before a frame may be evaluated.
    pub const fn complete(self, valid: &[usize; 3]) -> bool {
        match self {
            AchievementLayout::Gba => valid[0] >= 0x8000 && valid[1] >= 0x40000,
            AchievementLayout::Gb => valid[0] >= 0x10000,
        }
    }
}

pub trait RetroCore: Send {
    /// Only committed forward play is eligible for achievements.
    fn achievement_tracking(&mut self, _on: bool) {}
    /// Copy achievement RAM in `layout` on the owning thread; unsupported cores return
    /// no valid regions.
    fn achievement_memory(&self, _layout: AchievementLayout, _ram: &mut [u8]) -> [usize; 3] { [0; 3] }
    fn load(&mut self, rom: &Path) -> Result<(), CoreError>;
    fn run_frame(&mut self, input: ButtonMask);
    fn run_frame_linked(&mut self, p1: ButtonMask, _p2: ButtonMask) {
        self.run_frame(p1);
    }
    fn set_option(&mut self, _key: &str, _value: &str) {}
    fn set_frame_skip(&mut self, _skip: bool) {}
    fn video_xrgb8888(&self) -> &[u8];
    fn take_audio(&mut self) -> Vec<i16>;
    fn serialize(&mut self) -> Result<Vec<u8>, CoreError>;
    fn unserialize(&mut self, data: &[u8]) -> Result<(), CoreError>;
    fn save_ram(&self) -> Option<Vec<u8>>;
    fn load_save_ram(&mut self, data: &[u8]) -> Result<(), CoreError>;
    fn av_info(&self) -> AvInfo;
    fn rumble(&self) -> Rumble {
        Rumble::default()
    }
    fn net(&self) -> Link {
        Link::default()
    }
    fn start_link(&mut self, _client_id: u16) {}
    fn pump_link(&mut self) {}
    fn stop_link(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder(Vec<ButtonMask>);

    impl RetroCore for Recorder {
        fn load(&mut self, _rom: &Path) -> Result<(), CoreError> {
            Ok(())
        }
        fn run_frame(&mut self, input: ButtonMask) {
            self.0.push(input);
        }
        fn video_xrgb8888(&self) -> &[u8] {
            &[]
        }
        fn take_audio(&mut self) -> Vec<i16> {
            Vec::new()
        }
        fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
            Ok(Vec::new())
        }
        fn unserialize(&mut self, _data: &[u8]) -> Result<(), CoreError> {
            Ok(())
        }
        fn save_ram(&self) -> Option<Vec<u8>> {
            None
        }
        fn load_save_ram(&mut self, _data: &[u8]) -> Result<(), CoreError> {
            Ok(())
        }
        fn av_info(&self) -> AvInfo {
            AvInfo {
                fps: 60.0,
                sample_rate: 48_000.0,
            }
        }
    }

    #[test]
    fn a_core_without_link_mode_runs_player_1_alone() {
        let mut core = Recorder::default();
        core.run_frame_linked(ButtonMask(ButtonMask::A), ButtonMask(ButtonMask::B));
        assert_eq!(core.0, vec![ButtonMask(ButtonMask::A)]);
    }
}
