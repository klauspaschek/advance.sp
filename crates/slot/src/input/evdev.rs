use std::path::{Path, PathBuf};

use slot_input::{Btn, RawEvent};

pub const EVENT_BYTES: usize = 24;

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_SW: u16 = 0x05;
pub const SW_LID: u16 = 0x00;

pub const EV_ABS: u16 = 0x03;
pub const ABS_HAT0X: u16 = 0x10;
pub const ABS_HAT0Y: u16 = 0x11;
pub const ABS_LSX: u16 = 0x02;
pub const ABS_LSY: u16 = 0x03;

pub const STICK_DEADZONE: i32 = 2048;

const LEFT_STICK_TARGETS: [&str; 6] = [
    "rg34xxsp",
    "rg35xxh",
    "rg35xxpro",
    "rg40xxh",
    "rg40xxv",
    "rgcubexx",
];

pub fn has_left_stick(target: &str) -> bool {
    LEFT_STICK_TARGETS.contains(&target)
}

#[derive(Default)]
pub struct Hat {
    hat: [i32; 2],
    stick: [i32; 2],
    steers: bool,
}

impl Hat {
    pub fn steer_with_stick(&mut self, on: bool) {
        self.steers = on;
    }

    pub fn feed(&mut self, ev: Ev) -> Vec<RawEvent> {
        let tilt = |v: i32| match v {
            v if v > STICK_DEADZONE => 1,
            v if v < -STICK_DEADZONE => -1,
            _ => 0,
        };
        let steer = |v: i32| if self.steers { tilt(v) } else { 0 };
        let (axis, from_stick, value) = match ev.code {
            ABS_HAT0X => (0, false, ev.value.signum()),
            ABS_HAT0Y => (1, false, ev.value.signum()),
            ABS_LSX => (0, true, steer(ev.value)),
            ABS_LSY => (1, true, steer(ev.value)),
            _ => return Vec::new(),
        };
        let was = self.held(axis);
        match from_stick {
            true => self.stick[axis] = value,
            false => self.hat[axis] = value,
        }
        let now = self.held(axis);
        if now == was {
            return Vec::new();
        }
        let ends = [[Btn::Left, Btn::Right], [Btn::Up, Btn::Down]][axis];
        let btn = |v: i32| ends[usize::from(v > 0)];
        let mut out = Vec::new();
        if was != 0 {
            out.push(RawEvent::Up(btn(was)));
        }
        if now != 0 {
            out.push(RawEvent::Down(btn(now)));
        }
        out
    }

    fn held(&self, axis: usize) -> i32 {
        match self.hat[axis] {
            0 => self.stick[axis],
            hat => hat,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Ev {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

pub fn decode(bytes: &[u8]) -> Option<Ev> {
    let packet: &[u8; EVENT_BYTES] = bytes.get(..EVENT_BYTES)?.try_into().ok()?;
    Some(Ev {
        kind: u16::from_le_bytes([packet[16], packet[17]]),
        code: u16::from_le_bytes([packet[18], packet[19]]),
        value: i32::from_le_bytes([packet[20], packet[21], packet[22], packet[23]]),
    })
}

pub fn to_raw(ev: Ev) -> Option<RawEvent> {
    let btn = match ev.kind {
        EV_KEY => code_to_btn(ev.code)?,
        EV_SW if ev.code == SW_LID => Btn::Lid,
        _ => return None,
    };
    match ev.value {
        0 => Some(RawEvent::Up(btn)),
        1 => Some(RawEvent::Down(btn)),
        _ => None,
    }
}

pub fn code_to_btn(code: u16) -> Option<Btn> {
    Some(match code {
        0x130 => Btn::A,
        0x131 => Btn::B,
        0x132 => Btn::Y,
        0x133 => Btn::X,
        0x134 => Btn::L1,
        0x135 => Btn::R1,
        0x136 => Btn::Select,
        0x137 => Btn::Start,
        0x138 => Btn::Menu,
        0x13a => Btn::L2,
        0x13b => Btn::R2,
        115 => Btn::VolUp,
        114 => Btn::VolDown,
        116 => Btn::Power,
        _ => return None,
    })
}

const WANTED: [u16; 16] = [
    0x130, 0x131, 0x132, 0x133, 0x134, 0x135, 0x136, 0x137, 0x138, 0x13a, 0x13b, 0x13c, 0x162, 115,
    114, 116,
];

pub use slot_power::has_bit;

pub fn pick_devices(dev: &Path, sys: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dev) else {
        return Vec::new();
    };
    let mut nodes: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("event"))
        })
        .collect();
    nodes.sort();
    nodes.retain(|node| {
        let Some(name) = node.file_name() else {
            return false;
        };
        wanted_node(&sys.join(name).join("device"))
    });
    nodes
}

fn wanted_node(device: &Path) -> bool {
    let cap = |file: &str| std::fs::read_to_string(device.join("capabilities").join(file));
    let keys = cap("key").unwrap_or_default();
    let lid = cap("sw").is_ok_and(|sw| has_bit(&sw, SW_LID));
    lid || WANTED.iter().any(|bit| has_bit(&keys, *bit))
}

pub fn device_name(sys: &Path, node: &Path) -> String {
    node.file_name()
        .and_then(|n| std::fs::read_to_string(sys.join(n).join("device/name")).ok())
        .map(|n| n.trim().to_string())
        .unwrap_or_else(|| node.display().to_string())
}

// External gamepads (Bluetooth, classic or LE) report Linux's standard gamepad codes, not the
// handheld's own numbering. Ported from cartridge's input layer and adapted for slot: a pad is
// player one alongside the handheld's buttons.

pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_Z: u16 = 0x02;
pub const ABS_RX: u16 = 0x03;
pub const ABS_RY: u16 = 0x04;
pub const ABS_RZ: u16 = 0x05;
pub const ABS_GAS: u16 = 0x09;
pub const ABS_BRAKE: u16 = 0x0a;

pub const BTN_SOUTH: u16 = 0x130;
pub const KEY_BACK: u16 = 158;
pub const KEY_HOMEPAGE: u16 = 172;
/// The Share button of current Xbox controllers.
pub const KEY_RECORD: u16 = 167;
pub const BTN_THUMBL: u16 = 0x13d;
pub const BTN_THUMBR: u16 = 0x13e;

/// Whose codes a node sends: the handheld's own, or an external pad's.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Layout {
    Handheld,
    Pad,
}

/// A node on the Bluetooth bus (0005), or one made through `uhid` (Bluetooth LE pads, such as an
/// Xbox controller on current firmware), or a USB pad (0003), is an external pad.
pub fn layout_of(sys: &Path, node: &Path) -> Layout {
    let Some(name) = node.file_name() else {
        return Layout::Handheld;
    };
    let device = sys.join(name).join("device");
    let bus = std::fs::read_to_string(device.join("id/bustype")).unwrap_or_default();
    let real = std::fs::canonicalize(&device).unwrap_or_default();
    let bus = bus.trim();
    if bus == "0005" || bus == "0003" || real.to_string_lossy().contains("uhid") {
        Layout::Pad
    } else {
        Layout::Handheld
    }
}

/// Whether a node is a gamepad and not the media keys of headphones: it has the south button.
pub fn is_gamepad(sys: &Path, node: &Path) -> bool {
    let Some(name) = node.file_name() else {
        return false;
    };
    std::fs::read_to_string(sys.join(name).join("device/capabilities/key"))
        .is_ok_and(|keys| has_bit(&keys, BTN_SOUTH))
}

/// How a pad's face buttons are read: by where they sit (the east button is A and the north one
/// X, as on the handheld and a Nintendo pad), or by the letter printed on them (Xbox A is A).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum FaceButtons {
    #[default]
    Positions,
    Labels,
}

/// An external pad's key code as a button.
pub fn pad_code_to_btn(code: u16, faces: FaceButtons) -> Option<Btn> {
    let by_position = faces == FaceButtons::Positions;
    Some(match code {
        0x130 => {
            if by_position {
                Btn::B
            } else {
                Btn::A
            }
        }
        0x131 => {
            if by_position {
                Btn::A
            } else {
                Btn::B
            }
        }
        // Xbox X sits west and Y north: the other way round from the handheld
        0x133 => {
            if by_position {
                Btn::Y
            } else {
                Btn::X
            }
        }
        0x134 => {
            if by_position {
                Btn::X
            } else {
                Btn::Y
            }
        }
        0x136 => Btn::L1,
        0x137 => Btn::R1,
        0x138 => Btn::L2,
        0x139 => Btn::R2,
        0x13a | KEY_BACK => Btn::Select,
        0x13b => Btn::Start,
        0x13c | KEY_HOMEPAGE => Btn::Menu,
        // volume keys of Bluetooth headphones and pads
        115 => Btn::VolUp,
        114 => Btn::VolDown,
        _ => return None,
    })
}

/// A pad button the handheld does not have, handled by slot itself rather than as a button.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PadExtra {
    /// Share pressed (true) or released (false): a tap takes a screenshot, a hold is the power
    /// button.
    Share(bool),
    /// The left stick pressed in (saves a state) or let go.
    SaveState(bool),
    /// The right stick pressed in (loads a state) or let go.
    LoadState(bool),
    /// The right stick held left (true) or let go (false).
    Rewind(bool),
    /// The right stick held right (true) or let go (false).
    FastForward(bool),
    /// The right stick pushed up (next shader) or let go.
    ShaderNext(bool),
    /// The right stick pushed down (previous shader) or let go.
    ShaderPrev(bool),
    /// The left stick pushed one way (0 left, 1 right, 2 up, 3 down) or let go: it steers the
    /// d-pad anyway, this is for the button checker only.
    LeftStick(u8, bool),
}

impl PadExtra {
    /// The control's name and whether it is now held, for the button checker.
    pub fn held(self) -> (&'static str, bool) {
        match self {
            PadExtra::Share(on) => ("Share", on),
            PadExtra::SaveState(on) => ("L3", on),
            PadExtra::LoadState(on) => ("R3", on),
            PadExtra::Rewind(on) => ("R←", on),
            PadExtra::FastForward(on) => ("R→", on),
            PadExtra::ShaderNext(on) => ("R↑", on),
            PadExtra::ShaderPrev(on) => ("R↓", on),
            PadExtra::LeftStick(dir, on) => (["L←", "L→", "L↑", "L↓"][usize::from(dir.min(3))], on),
        }
    }
}

pub fn pad_extra(ev: Ev) -> Option<PadExtra> {
    if ev.kind != EV_KEY {
        return None;
    }
    match (ev.code, ev.value) {
        (KEY_RECORD, 1) => Some(PadExtra::Share(true)),
        (KEY_RECORD, 0) => Some(PadExtra::Share(false)),
        (BTN_THUMBL, 0 | 1) => Some(PadExtra::SaveState(ev.value == 1)),
        (BTN_THUMBR, 0 | 1) => Some(PadExtra::LoadState(ev.value == 1)),
        _ => None,
    }
}

pub fn to_raw_for(layout: Layout, faces: FaceButtons, ev: Ev) -> Option<RawEvent> {
    if layout == Layout::Handheld || ev.kind != EV_KEY {
        return to_raw(ev);
    }
    let btn = pad_code_to_btn(ev.code, faces)?;
    match ev.value {
        0 => Some(RawEvent::Up(btn)),
        1 => Some(RawEvent::Down(btn)),
        _ => None,
    }
}

/// The reach of an axis, as the driver reports it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Range {
    pub min: i32,
    pub max: i32,
}

impl Range {
    pub const STICK: Range = Range {
        min: -32768,
        max: 32767,
    };
    pub const TRIGGER: Range = Range { min: 0, max: 255 };

    /// `value` on the handheld stick's scale, -4096 to 4096 with the rest at 0.
    pub fn stick(self, value: i32) -> i32 {
        let span = i64::from(self.max - self.min).max(1);
        let mid = (i64::from(self.max) + i64::from(self.min)) / 2;
        let scaled = (i64::from(value) - mid) * 8192 / span;
        scaled.clamp(-4096, 4096) as i32
    }

    /// Whether a trigger reading is past half of its travel.
    pub fn pressed(self, value: i32) -> bool {
        (i64::from(value) - i64::from(self.min)) * 2 > i64::from(self.max - self.min)
    }
}

/// Which axes carry a pad's analog triggers, left then right. Pads on the HID gamepad layout
/// (an Xbox controller over Bluetooth LE) send them as brake and gas, with the right stick on
/// Z and RZ; older drivers send the triggers on Z and RZ themselves.
pub fn trigger_axes(sys: &Path, node: &Path) -> [u16; 2] {
    let abs = node
        .file_name()
        .and_then(|n| std::fs::read_to_string(sys.join(n).join("device/capabilities/abs")).ok())
        .unwrap_or_default();
    if has_bit(&abs, ABS_BRAKE) || has_bit(&abs, ABS_GAS) {
        [ABS_BRAKE, ABS_GAS]
    } else {
        [ABS_Z, ABS_RZ]
    }
}

/// Which axes carry a pad's right stick, across then down: Z and RZ on the HID gamepad layout
/// (triggers on brake and gas), RX and RY otherwise.
pub fn right_stick_axes(triggers: [u16; 2]) -> [u16; 2] {
    if triggers == [ABS_BRAKE, ABS_GAS] {
        [ABS_Z, ABS_RZ]
    } else {
        [ABS_RX, ABS_RY]
    }
}

/// How far the right stick is pushed before it acts, and how far back it must come to let go.
const RIGHT_PRESS: i32 = 2048;
const RIGHT_LET_GO: i32 = 1024;

/// An external pad's axes: the left stick and the hat steer the d-pad, an analog trigger past
/// half its travel presses L2 or R2, and the right stick is a set of shortcuts (rewind and
/// fast-forward across, the shader up and down).
pub struct PadAxes {
    stick: [Range; 2],
    trigger_codes: [u16; 2],
    triggers: [Range; 2],
    held: [bool; 2],
    right_codes: [u16; 2],
    right: [Range; 2],
    right_held: [i32; 2],
    left_held: [i32; 2],
}

impl PadAxes {
    pub fn new(stick: [Range; 2], trigger_codes: [u16; 2], triggers: [Range; 2]) -> Self {
        PadAxes {
            stick,
            trigger_codes,
            triggers,
            held: [false; 2],
            right_codes: right_stick_axes(trigger_codes),
            right: [Range::STICK; 2],
            right_held: [0; 2],
            left_held: [0; 2],
        }
    }

    /// The right stick's reach, as the driver reports it.
    pub fn with_right_stick(mut self, right: [Range; 2]) -> Self {
        self.right = right;
        self
    }

    pub fn right_codes(&self) -> [u16; 2] {
        self.right_codes
    }

    /// The left stick's direction for the button checker, as it changes.
    pub fn left_stick(&mut self, ev: Ev) -> Vec<PadExtra> {
        let axis = match ev.code {
            ABS_X => 0,
            ABS_Y => 1,
            _ => return Vec::new(),
        };
        let value = self.stick[axis].stick(ev.value);
        let held = self.left_held[axis];
        let reach = if held == 0 { RIGHT_PRESS } else { RIGHT_LET_GO };
        let now = if value.abs() >= reach { value.signum() } else { 0 };
        if now == held {
            return Vec::new();
        }
        self.left_held[axis] = now;
        let dir = |v: i32| if axis == 0 { u8::from(v > 0) } else { 2 + u8::from(v > 0) };
        let mut out = Vec::new();
        if held != 0 {
            out.push(PadExtra::LeftStick(dir(held), false));
        }
        if now != 0 {
            out.push(PadExtra::LeftStick(dir(now), true));
        }
        out
    }

    /// The right stick's shortcuts, or `None` for an axis that is not the right stick.
    pub fn right_stick(&mut self, ev: Ev) -> Option<Vec<PadExtra>> {
        let axis = self.right_codes.iter().position(|c| *c == ev.code)?;
        let value = self.right[axis].stick(ev.value);
        let held = self.right_held[axis];
        let reach = if held == 0 { RIGHT_PRESS } else { RIGHT_LET_GO };
        let now = if value.abs() >= reach { value.signum() } else { 0 };
        if now == held {
            return Some(Vec::new());
        }
        self.right_held[axis] = now;
        let mut out = Vec::new();
        if axis == 0 {
            match held {
                -1 => out.push(PadExtra::Rewind(false)),
                1 => out.push(PadExtra::FastForward(false)),
                _ => {}
            }
            match now {
                -1 => out.push(PadExtra::Rewind(true)),
                1 => out.push(PadExtra::FastForward(true)),
                _ => {}
            }
        } else {
            // up is negative
            match held {
                -1 => out.push(PadExtra::ShaderNext(false)),
                1 => out.push(PadExtra::ShaderPrev(false)),
                _ => {}
            }
            match now {
                -1 => out.push(PadExtra::ShaderNext(true)),
                1 => out.push(PadExtra::ShaderPrev(true)),
                _ => {}
            }
        }
        Some(out)
    }

    pub fn feed(&mut self, hat: &mut Hat, ev: Ev) -> Vec<RawEvent> {
        if let Some(t) = self.trigger_codes.iter().position(|c| *c == ev.code) {
            let now = self.triggers[t].pressed(ev.value);
            if now == self.held[t] {
                return Vec::new();
            }
            self.held[t] = now;
            let btn = [Btn::L2, Btn::R2][t];
            return vec![if now {
                RawEvent::Down(btn)
            } else {
                RawEvent::Up(btn)
            }];
        }
        let (i, code) = match ev.code {
            ABS_X => (0, ABS_LSX),
            ABS_Y => (1, ABS_LSY),
            ABS_HAT0X | ABS_HAT0Y => return hat.feed(ev),
            _ => return Vec::new(),
        };
        hat.feed(Ev {
            kind: ev.kind,
            code,
            value: self.stick[i].stick(ev.value),
        })
    }
}
