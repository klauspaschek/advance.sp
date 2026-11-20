use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slot_input::{Btn, InputSource, Millis, RawEvent};

use super::evdev::{
    decode, device_name, is_gamepad, layout_of, pick_devices, to_raw_for, trigger_axes, Ev,
    FaceButtons, Hat, Layout, PadAxes, Range, ABS_X, ABS_Y, EVENT_BYTES, EV_ABS, EV_KEY, EV_SYN,
};
use super::trace;

const DEV: &str = "/dev/input";
const SYS: &str = "/sys/class/input";
const PSY: &str = "/sys/class/power_supply";

const LID_POLL_MS: Millis = 150;

/// How often `/dev/input` is looked at for a node that was not there before: a Bluetooth pad
/// that connects while slot runs brings a node of its own.
const WATCH_EVERY: Duration = Duration::from_secs(1);

pub struct DeviceInput {
    pending: Arc<Mutex<Vec<RawEvent>>>,
    stick: Arc<AtomicBool>,
    by_position: Arc<AtomicBool>,
    pads: Arc<Mutex<Vec<String>>>,
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
            by_position: Arc::new(AtomicBool::new(false)),
            pads: Arc::new(Mutex::new(Vec::new())),
            trace,
        };
        nodes.open_new();
        let input = DeviceInput {
            pending: nodes.pending.clone(),
            stick: nodes.stick.clone(),
            by_position: nodes.by_position.clone(),
            pads: nodes.pads.clone(),
            hall: find_hall(Path::new(PSY)),
            lid_shut: None,
            next_lid_poll: 0,
        };
        let spawned = std::thread::Builder::new()
            .name("slot-input-watch".into())
            .spawn(move || loop {
                std::thread::sleep(WATCH_EVERY);
                nodes.open_new();
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
    trace: Option<Arc<Trace>>,
}

impl Nodes {
    /// Starts a reader for each wanted node that has none yet.
    fn open_new(&self) {
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
            }
            let reader = Reader {
                layout,
                triggers: trigger_axes(&self.sys, &node),
                queue: self.pending.clone(),
                steers: self.stick.clone(),
                by_position: self.by_position.clone(),
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
        let mut axes = PadAxes::new(
            [ABS_X, ABS_Y].map(|c| axis_range(&file, c, Range::STICK)),
            self.triggers,
            self.triggers.map(|c| axis_range(&file, c, Range::TRIGGER)),
        );
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
