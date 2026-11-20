use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slot_input::{Action, Btn, InputSource, Millis, RawEvent};

use super::evdev::{
    decode, device_name, is_gamepad, layout_of, pad_extra, pick_devices, to_raw_for,
    trigger_axes, Ev, FaceButtons, Hat, Layout, PadAxes, PadExtra, Range, ABS_X, ABS_Y,
    EVENT_BYTES, EV_ABS, EV_KEY, EV_SYN,
};
use super::trace;

const DEV: &str = "/dev/input";
const SYS: &str = "/sys/class/input";
const PSY: &str = "/sys/class/power_supply";

const LID_POLL_MS: Millis = 150;

/// How often `/dev/input` is looked at for a node that was not there before: a Bluetooth pad
/// that connects while slot runs brings a node of its own.
const WATCH_EVERY: Duration = Duration::from_secs(1);

/// How long Share must be held before it becomes the power button: shorter is a screenshot.
pub const SHARE_POWER_MS: Millis = 500;

pub struct DeviceInput {
    pending: Arc<Mutex<Vec<RawEvent>>>,
    stick: Arc<AtomicBool>,
    by_position: Arc<AtomicBool>,
    pads: Arc<Mutex<Vec<String>>>,
    extras: Arc<Mutex<Vec<PadExtra>>>,
    arrived: Arc<AtomicBool>,
    last_source: Arc<AtomicU8>,
    share_since: Option<Millis>,
    share_power: bool,
    actions: Vec<Action>,
    testing: bool,
    tested: Vec<(&'static str, bool)>,
    hall: Option<PathBuf>,
    lid_shut: Option<bool>,
    next_lid_poll: Millis,
}

impl DeviceInput {
    pub fn open(root: &Path) -> Self {
        DeviceInput::open_in(Path::new(DEV), Path::new(SYS), root, trace::enabled())
    }

    pub fn open_in(dev: &Path, sys: &Path, root: &Path, trace: bool) -> Self {
        let trace = trace.then(|| Trace::start(root, dev, sys)).flatten();
        DeviceInput::open_traced(dev, sys, trace)
    }

    fn open_traced(dev: &Path, sys: &Path, trace: Option<Arc<Trace>>) -> Self {
        let nodes = Nodes {
            dev: dev.to_path_buf(),
            sys: sys.to_path_buf(),
            pending: Arc::new(Mutex::new(Vec::new())),
            opened: Arc::new(Mutex::new(HashSet::new())),
            stick: Arc::new(AtomicBool::new(false)),
            by_position: Arc::new(AtomicBool::new(true)),
            pads: Arc::new(Mutex::new(Vec::new())),
            extras: Arc::new(Mutex::new(Vec::new())),
            arrived: Arc::new(AtomicBool::new(false)),
            last_source: Arc::new(AtomicU8::new(0)),
            trace,
        };
        nodes.open_new(false);
        let input = DeviceInput {
            pending: nodes.pending.clone(),
            stick: nodes.stick.clone(),
            by_position: nodes.by_position.clone(),
            pads: nodes.pads.clone(),
            extras: nodes.extras.clone(),
            arrived: nodes.arrived.clone(),
            last_source: nodes.last_source.clone(),
            share_since: None,
            share_power: false,
            actions: Vec::new(),
            testing: false,
            tested: Vec::new(),
            hall: find_hall(Path::new(PSY)),
            lid_shut: None,
            next_lid_poll: 0,
        };
        let spawned = std::thread::Builder::new()
            .name("slot-input-watch".into())
            .spawn(move || loop {
                std::thread::sleep(WATCH_EVERY);
                nodes.open_new(true);
            });
        if let Err(e) = spawned {
            eprintln!("slot: input watch: {e}");
        }
        input
    }
}

struct Nodes {
    dev: PathBuf,
    sys: PathBuf,
    pending: Arc<Mutex<Vec<RawEvent>>>,
    opened: Arc<Mutex<HashSet<PathBuf>>>,
    stick: Arc<AtomicBool>,
    by_position: Arc<AtomicBool>,
    pads: Arc<Mutex<Vec<String>>>,
    extras: Arc<Mutex<Vec<PadExtra>>>,
    arrived: Arc<AtomicBool>,
    last_source: Arc<AtomicU8>,
    trace: Option<Arc<Trace>>,
}

impl Nodes {
    /// Starts a reader for each wanted node that has none yet. `later` marks nodes found after
    /// startup, whose gamepads count as arriving (to wake the handheld from light sleep).
    fn open_new(&self, later: bool) {
        for node in pick_devices(&self.dev, &self.sys) {
            let fresh = self
                .opened
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(node.clone());
            if !fresh {
                continue;
            }
            let layout = layout_of(&self.sys, &node);
            let name = device_name(&self.sys, &node);
            let pad = layout == Layout::Pad && is_gamepad(&self.sys, &node);
            eprintln!(
                "slot: input {} ({name}){}",
                node.display(),
                if pad { " as a gamepad" } else { "" }
            );
            quicken_poll(&self.sys, &node);
            if pad {
                lock(&self.pads).push(name.clone());
                if later {
                    self.arrived.store(true, Ordering::Relaxed);
                }
            }
            let reader = Reader {
                layout,
                triggers: trigger_axes(&self.sys, &node),
                queue: self.pending.clone(),
                steers: self.stick.clone(),
                by_position: self.by_position.clone(),
                extras: pad.then(|| self.extras.clone()),
                last_source: self.last_source.clone(),
                trace: self.trace.clone(),
            };
            let opened = self.opened.clone();
            let pads = self.pads.clone();
            let label = node.display().to_string();
            let spawned = std::thread::Builder::new()
                .name(format!("slot-input-{label}"))
                .spawn(move || {
                    reader.read(&node);
                    if pad {
                        eprintln!("slot: input {} ({name}) disconnected", node.display());
                        let mut pads = lock(&pads);
                        if let Some(i) = pads.iter().position(|p| *p == name) {
                            pads.remove(i);
                        }
                    }
                    // gone: a node of the same name later is a new device
                    lock(&opened).remove(&node);
                });
            if let Err(e) = spawned {
                eprintln!("slot: input {label}: {e}");
            }
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

const POLL_MS: u32 = 10;

fn quicken_poll(sys: &Path, node: &Path) {
    let Some(name) = node.file_name() else {
        return;
    };
    let attr = sys.join(name).join("device/poll");
    let Some(now) = std::fs::read_to_string(&attr)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    else {
        return;
    };
    if now > POLL_MS {
        match std::fs::write(&attr, POLL_MS.to_string()) {
            Ok(()) => eprintln!(
                "slot: input {}: poll {now} ms -> {POLL_MS} ms",
                node.display()
            ),
            Err(e) => eprintln!("slot: input {}: poll: {e}", node.display()),
        }
    }
}

/// How far axis `code` of the open node goes, as its driver says (EVIOCGABS), else `default`.
fn axis_range(file: &File, code: u16, default: Range) -> Range {
    let mut info = [0i32; 6];
    let request = 0x8018_4540u64 + u64::from(code);
    // SAFETY: EVIOCGABS fills a struct input_absinfo, six 32-bit integers, and nothing else
    let ok = unsafe { libc::ioctl(file.as_raw_fd(), request as _, info.as_mut_ptr()) } == 0;
    if ok && info[2] > info[1] {
        Range {
            min: info[1],
            max: info[2],
        }
    } else {
        default
    }
}

struct Reader {
    layout: Layout,
    triggers: [u16; 2],
    queue: Arc<Mutex<Vec<RawEvent>>>,
    steers: Arc<AtomicBool>,
    by_position: Arc<AtomicBool>,
    /// A gamepad's buttons the handheld does not have.
    extras: Option<Arc<Mutex<Vec<PadExtra>>>>,
    /// Who pressed last: 1 the handheld, 2 a gamepad.
    last_source: Arc<AtomicU8>,
    trace: Option<Arc<Trace>>,
}

impl Reader {
    fn read(&self, node: &Path) {
        let mut held: Vec<Btn> = Vec::new();
        self.read_into(node, &mut held);
        // a pad that drops mid-press must not leave its buttons held
        if !held.is_empty() {
            lock(&self.queue).extend(held.into_iter().map(RawEvent::Up));
        }
        if let Some(extras) = &self.extras {
            lock(extras).extend([
                PadExtra::Share(false),
                PadExtra::Rewind(false),
                PadExtra::FastForward(false),
            ]);
        }
    }

    fn read_into(&self, node: &Path, held: &mut Vec<Btn>) {
        let label = node
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| node.display().to_string());
        let pad = self.layout == Layout::Pad;
        let mut hat = Hat::default();
        let mut file = match File::open(node) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("slot: input {}: {e}", node.display());
                return;
            }
        };
        let axes = PadAxes::new(
            [ABS_X, ABS_Y].map(|c| axis_range(&file, c, Range::STICK)),
            self.triggers,
            self.triggers.map(|c| axis_range(&file, c, Range::TRIGGER)),
        );
        let right = axes.right_codes().map(|c| axis_range(&file, c, Range::STICK));
        let mut axes = axes.with_right_stick(right);
        let mut buf = [0u8; EVENT_BYTES * 16];
        loop {
            let read = match file.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    eprintln!("slot: input {}: {e}", node.display());
                    return;
                }
            };
            let faces = if self.by_position.load(Ordering::Relaxed) {
                FaceButtons::Positions
            } else {
                FaceButtons::Labels
            };
            let mut events = Vec::new();
            for ev in buf[..read].chunks_exact(EVENT_BYTES).filter_map(decode) {
                if let Some(trace) = &self.trace {
                    trace.event(&label, ev);
                }
                if (ev.kind == EV_KEY && ev.value == 1) || (ev.kind == EV_ABS && ev.value != 0) {
                    crate::latency::read();
                }
                if ev.kind == EV_KEY && ev.value == 1 {
                    let source = if self.extras.is_some() { 2 } else { 1 };
                    self.last_source.store(source, Ordering::Relaxed);
                }
                if let Some(extras) = &self.extras {
                    if let Some(extra) = pad_extra(ev) {
                        lock(extras).push(extra);
                        continue;
                    }
                    if ev.kind == EV_ABS {
                        if let Some(shortcuts) = axes.right_stick(ev) {
                            if shortcuts.iter().any(|e| e.held().1) {
                                self.last_source.store(2, Ordering::Relaxed);
                            }
                            lock(extras).extend(shortcuts);
                            continue;
                        }
                        let left = axes.left_stick(ev);
                        if left.iter().any(|e| e.held().1) {
                            self.last_source.store(2, Ordering::Relaxed);
                        }
                        lock(extras).extend(left);
                    }
                }
                match ev.kind {
                    EV_ABS if pad => {
                        // an external pad's stick always steers: it is the pad's d-pad too
                        hat.steer_with_stick(true);
                        events.extend(axes.feed(&mut hat, ev));
                    }
                    EV_ABS => {
                        hat.steer_with_stick(self.steers.load(Ordering::Relaxed));
                        events.extend(hat.feed(ev));
                    }
                    _ => events.extend(to_raw_for(self.layout, faces, ev)),
                }
            }
            if events.is_empty() {
                continue;
            }
            for e in &events {
                match *e {
                    RawEvent::Down(b) if !held.contains(&b) => held.push(b),
                    RawEvent::Up(b) => held.retain(|h| *h != b),
                    _ => {}
                }
            }
            lock(&self.queue).extend(events);
        }
    }
}

struct Trace {
    out: Mutex<File>,
    began: Instant,
}

impl Trace {
    fn start(root: &Path, dev: &Path, sys: &Path) -> Option<Arc<Trace>> {
        let dir = crate::root::logs_dir(root);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(trace::TRACE_FILE);
        // keep the previous session's trace: a restart must not lose what went wrong before it
        let _ = std::fs::rename(&path, dir.join(format!("{}.1", trace::TRACE_FILE)));
        let mut file = match File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("slot: {}: {e}", path.display());
                return None;
            }
        };
        for line in trace::survey(dev, sys) {
            let _ = writeln!(file, "{line}");
        }
        let _ = writeln!(file, "--");
        let _ = file.flush();
        Some(Arc::new(Trace {
            out: Mutex::new(file),
            began: Instant::now(),
        }))
    }

    fn event(&self, node: &str, ev: Ev) {
        if ev.kind == EV_SYN {
            return;
        }
        let line = trace::event_line(node, self.began.elapsed().as_millis(), ev);
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

impl DeviceInput {
    pub fn steer_with_stick(&self, on: bool) {
        self.stick.store(on, Ordering::Relaxed);
    }

    /// Reads external pads' face buttons by position (east is A) rather than by label.
    pub fn pad_faces_by_position(&self, on: bool) {
        self.by_position.store(on, Ordering::Relaxed);
    }

    /// The names of the gamepads connected now, in the order they came.
    pub fn gamepads(&self) -> Vec<String> {
        lock(&self.pads).clone()
    }

    /// What a gamepad's extra buttons asked for since the last call: a screenshot, or saving or
    /// loading a state.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// Whether the last button pressed was a gamepad's rather than the handheld's.
    pub fn pad_pressed_last(&self) -> bool {
        self.last_source.load(Ordering::Relaxed) == 2
    }

    /// Whether a gamepad connected since the last call (after startup).
    pub fn take_pad_arrival(&self) -> bool {
        self.arrived.swap(false, Ordering::Relaxed)
    }

    /// A tap of the power button, as if pressed: wakes the handheld from light sleep.
    pub fn press_power(&self) {
        lock(&self.pending).extend([RawEvent::Down(Btn::Power), RawEvent::Up(Btn::Power)]);
    }

    /// While the button checker is open, a gamepad's extra buttons only report themselves:
    /// no screenshot, state or power button.
    pub fn set_testing(&mut self, on: bool) {
        if on && !self.testing {
            self.share_since = None;
            self.share_power = false;
        }
        self.testing = on;
    }

    /// The extra buttons pressed or let go while the button checker is open.
    pub fn take_tested(&mut self) -> Vec<(&'static str, bool)> {
        std::mem::take(&mut self.tested)
    }

    fn pad_extras(&mut self, now: Millis, out: &mut Vec<RawEvent>) {
        for extra in std::mem::take(&mut *lock(&self.extras)) {
            if self.testing {
                self.tested.push(extra.held());
                continue;
            }
            match extra {
                PadExtra::Share(true) => {
                    if self.share_since.is_none() {
                        self.share_since = Some(now);
                        self.share_power = false;
                    }
                }
                PadExtra::Share(false) => {
                    if self.share_since.take().is_some() {
                        if std::mem::take(&mut self.share_power) {
                            out.push(RawEvent::Up(Btn::Power));
                        } else {
                            self.actions.push(Action::Screenshot);
                        }
                    }
                }
                PadExtra::SaveState(true) => self.actions.push(Action::SaveState),
                PadExtra::LoadState(true) => self.actions.push(Action::LoadState),
                PadExtra::Rewind(on) => self.actions.push(if on { Action::RewindStart } else { Action::RewindStop }),
                PadExtra::FastForward(on) => self.actions.push(if on { Action::FfStart } else { Action::FfStop }),
                PadExtra::ShaderNext(true) => self.actions.push(Action::ShaderNext),
                PadExtra::ShaderPrev(true) => self.actions.push(Action::ShaderPrev),
                PadExtra::SaveState(false)
                | PadExtra::LoadState(false)
                | PadExtra::ShaderNext(false)
                | PadExtra::ShaderPrev(false)
                | PadExtra::LeftStick(..) => {}
            }
        }
        if let Some(since) = self.share_since.filter(|_| !self.testing) {
            if !self.share_power && now.saturating_sub(since) >= SHARE_POWER_MS {
                self.share_power = true;
                out.push(RawEvent::Down(Btn::Power));
            }
        }
    }

    fn read_lid(&self) -> Option<bool> {
        let raw = std::fs::read_to_string(self.hall.as_ref()?).ok()?;
        match raw.trim() {
            "0" => Some(true),
            "1" => Some(false),
            _ => None,
        }
    }
}

impl InputSource for DeviceInput {
    fn poll(&mut self, now: Millis) -> Vec<RawEvent> {
        let mut out = std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()));
        self.pad_extras(now, &mut out);
        if self.hall.is_some() && now >= self.next_lid_poll {
            self.next_lid_poll = now + LID_POLL_MS;
            if let Some(shut) = self.read_lid() {
                if self.lid_shut != Some(shut) {
                    self.lid_shut = Some(shut);
                    out.push(if shut {
                        RawEvent::Down(Btn::Lid)
                    } else {
                        RawEvent::Up(Btn::Lid)
                    });
                }
            }
        }
        out
    }
}

fn find_hall(psy: &Path) -> Option<PathBuf> {
    let mut supplies: Vec<PathBuf> = std::fs::read_dir(psy)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    supplies.sort();
    supplies
        .into_iter()
        .map(|d| d.join("hallkey"))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> (tempfile::TempDir, DeviceInput) {
        let d = tempfile::tempdir().unwrap();
        let input = DeviceInput::open_in(&d.path().join("dev"), &d.path().join("sys"), d.path(), false);
        (d, input)
    }

    #[test]
    fn share_held_is_the_power_button() {
        let (_d, mut input) = input();
        lock(&input.extras).push(PadExtra::Share(true));
        assert!(input.poll(1000).is_empty());
        assert!(input.poll(1000 + SHARE_POWER_MS - 1).is_empty());
        assert_eq!(input.poll(1000 + SHARE_POWER_MS), vec![RawEvent::Down(Btn::Power)]);
        assert!(input.poll(3000).is_empty(), "the power button stays held: slot's own hold powers off");
        lock(&input.extras).push(PadExtra::Share(false));
        assert_eq!(input.poll(3100), vec![RawEvent::Up(Btn::Power)]);
        assert!(input.take_actions().is_empty(), "a hold is no screenshot");
    }

    #[test]
    fn a_release_without_a_press_does_nothing() {
        let (_d, mut input) = input();
        lock(&input.extras).push(PadExtra::Share(false));
        assert!(input.poll(0).is_empty());
        assert!(input.take_actions().is_empty());
    }

    #[test]
    fn a_power_tap_is_queued_for_waking() {
        let (_d, mut input) = input();
        assert!(!input.take_pad_arrival());
        input.arrived.store(true, Ordering::Relaxed);
        assert!(input.take_pad_arrival());
        assert!(!input.take_pad_arrival(), "taken once");
        input.press_power();
        assert_eq!(input.poll(0), vec![RawEvent::Down(Btn::Power), RawEvent::Up(Btn::Power)]);
    }
}

#[cfg(test)]
mod checker_tests {
    use super::*;

    #[test]
    fn while_testing_the_extras_only_report_themselves() {
        let d = tempfile::tempdir().unwrap();
        let mut input = DeviceInput::open_in(&d.path().join("dev"), &d.path().join("sys"), d.path(), false);
        input.set_testing(true);
        lock(&input.extras).extend([PadExtra::Share(true), PadExtra::SaveState(true)]);
        assert!(input.poll(0).is_empty());
        assert!(input.poll(5000).is_empty(), "a held Share is no power button while testing");
        assert!(input.take_actions().is_empty());
        assert_eq!(input.take_tested(), vec![("Share", true), ("L3", true)]);
        input.set_testing(false);
        lock(&input.extras).push(PadExtra::Share(false));
        assert!(input.poll(5100).is_empty(), "the press began while testing: no screenshot");
        assert!(input.take_actions().is_empty());
    }
}
