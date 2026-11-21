//! Optional AVRCP volume through BlueALSA. Never write BlueZ's transport directly:
//! only its owner (BlueALSA) is allowed to do that.
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct Link {
    pcm: String,
    transport: String,
}
/// One look at the headset's volume.
#[derive(Debug)]
struct Synced {
    link: Link,
    /// The headset's own volume (0 to 127) as it reported it last.
    raw: u8,
    /// slot's volume in effect at the headset.
    level: u8,
    /// The headset changed its volume itself, with its own buttons.
    remote: bool,
}
struct Reply {
    generation: u64,
    requested: u8,
    result: Result<Synced, &'static str>,
    /// The D-Bus error behind a failed link, for the log.
    detail: String,
}
#[derive(Default)]
pub struct Volume {
    address: Option<String>,
    generation: u64,
    link: Option<Link>,
    observed: Option<u8>,
    /// The headset's own volume as last read, to tell its own buttons from slot's writes.
    raw: Option<u8>,
    pending: Option<Receiver<Reply>>,
    next: Option<Instant>,
    /// Why the headset's volume could not be linked, as last logged: a reason is logged
    /// once, when it changes, not on every retry.
    reason: Option<&'static str>,
}
impl Volume {
    pub fn software_volume(&self, address: Option<&str>, volume: u8) -> u8 {
        if volume != 0 && address == self.address.as_deref() && self.link.is_some() {
            100
        } else {
            volume
        }
    }

    // One bounded worker at a time; late replies never affect a new route, and
    // a remote observation never overwrites a newer press of the volume buttons.
    pub fn poll(&mut self, address: Option<&str>, requested: u8) -> Option<u8> {
        if address != self.address.as_deref() {
            self.address = address.map(str::to_owned);
            self.generation = self.generation.wrapping_add(1);
            self.link = None;
            self.observed = None;
            self.raw = None;
            self.reason = None;
            // a fresh connection needs a moment before its volume can be read
            self.next = Some(Instant::now() + Duration::from_secs(2));
        }
        let mut feedback = None;
        if let Some(reply) = self.pending.as_ref().and_then(|r| r.try_recv().ok()) {
            self.pending = None;
            if reply.generation == self.generation {
                match reply.result {
                    Ok(synced) => {
                        if self.link.is_none() {
                            // once per connection: which volume control the headset offers
                            eprintln!(
                                "slot: bluetooth volume: the headset's own volume follows slot's, now {}%",
                                synced.level
                            );
                        }
                        self.reason = None;
                        self.link = Some(synced.link);
                        self.raw = Some(synced.raw);
                        self.observed = Some(synced.level);
                        if synced.remote && requested == reply.requested {
                            feedback = Some(synced.level);
                        }
                        self.next = Some(Instant::now() + Duration::from_millis(750));
                    }
                    Err(why) => {
                        if self.reason != Some(why) {
                            let detail = if reply.detail.is_empty() { String::new() } else { format!(": {}", reply.detail) };
                            eprintln!("slot: bluetooth volume: not linked ({why}{detail}): slot lowers the sound itself, the headset's own volume stays as set on it; retrying every 5 s");
                            self.reason = Some(why);
                        }
                        self.link = None;
                        self.observed = None;
                        self.raw = None;
                        self.next = Some(Instant::now() + Duration::from_secs(5));
                    }
                }
            }
        }
        let desired = feedback.unwrap_or(requested);
        let changed = self.link.is_some() && self.observed != Some(desired);
        if cfg!(all(feature = "device", not(test)))
            && self.pending.is_none()
            && (changed || self.next.is_none_or(|at| Instant::now() >= at))
        {
            if let Some(address) = self.address.clone() {
                let link = self.link.clone();
                let raw = self.raw;
                let generation = self.generation;
                let (tx, rx) = mpsc::channel();
                self.pending = Some(rx);
                std::thread::spawn(move || {
                    FIRST_ERROR.with(|e| e.borrow_mut().clear());
                    let result = sync(&address, link, raw, desired, changed);
                    let detail = FIRST_ERROR.with(|e| std::mem::take(&mut *e.borrow_mut()));
                    let _ = tx.send(Reply {
                        generation,
                        requested: desired,
                        result,
                        detail,
                    });
                });
            }
        }
        feedback
    }
}

thread_local! {
    /// The first D-Bus error of one attempt: what BlueZ or BlueALSA said when it refused.
    static FIRST_ERROR: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}
fn call(destination: &str, path: &str, args: &[&str]) -> Result<String, ()> {
    let output = std::process::Command::new("timeout")
        .args(["2", "dbus-send", "--system", "--print-reply"])
        .arg(format!("--dest={destination}"))
        .arg(path)
        .args(args)
        .output()
        .map_err(|_| ())?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.lines().next().unwrap_or("timed out").trim().chars().take(160).collect::<String>();
        FIRST_ERROR.with(|e| {
            let mut e = e.borrow_mut();
            if e.is_empty() {
                *e = format!("{} on {path}: {said}", args.first().copied().unwrap_or(""));
            }
        });
        return Err(());
    }
    String::from_utf8(output.stdout).map_err(|_| ())
}
type Bus = dyn Fn(&str, &str, &[&str]) -> Result<String, ()>;

fn get(
    bus: &Bus,
    destination: &str,
    path: &str,
    interface: &str,
    property: &str,
) -> Result<String, ()> {
    bus(
        destination,
        path,
        &[
            "org.freedesktop.DBus.Properties.Get",
            &format!("string:{interface}"),
            &format!("string:{property}"),
        ],
    )
}
fn set(bus: &Bus, link: &Link, property: &str, value: &str) -> Result<(), ()> {
    bus(
        "org.bluealsa",
        &link.pcm,
        &[
            "org.freedesktop.DBus.Properties.Set",
            "string:org.bluealsa.PCM1",
            &format!("string:{property}"),
            value,
        ],
    )
    .map(|_| ())
}
fn uint16(text: &str) -> Result<u16, ()> {
    let words: Vec<_> = text.split_whitespace().collect();
    words
        .windows(2)
        .find(|w| w[0] == "uint16")
        .and_then(|w| w[1].parse().ok())
        .ok_or(())
}
fn remote(bus: &Bus, link: &Link) -> Result<u8, ()> {
    let value = uint16(&get(
        bus,
        "org.bluez",
        &link.transport,
        "org.bluez.MediaTransport1",
        "Volume",
    )?)?;
    u8::try_from(value).ok().filter(|v| *v <= 127).ok_or(())
}
// Keep Slot's existing squared volume curve, but apply it once, at the headset.
fn native(level: u8) -> u8 {
    ((u32::from(level.min(100)).pow(2) * 127 + 5000) / 10000) as u8
}
fn percent(value: u8) -> u8 {
    ((f32::from(value) / 127.0).sqrt() * 100.0).round() as u8
}
fn transport(text: &str, address: &str) -> Option<String> {
    let suffix = format!("/dev_{}/", address.replace(':', "_").to_ascii_uppercase());
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("object path \""))
        .filter_map(|line| line.strip_suffix('"'))
        .find(|path| {
            path.starts_with("/org/bluez/hci")
                && path.contains(&suffix)
                && path.rsplit('/').next().is_some_and(|s| {
                    s.starts_with("fd") && s[2..].bytes().all(|b| b.is_ascii_digit())
                })
        })
        .map(str::to_owned)
}
fn discover(bus: &Bus, address: &str) -> Result<Link, ()> {
    if !crate::bluetooth::valid_address(address) {
        return Err(());
    }
    let objects = bus(
        "org.bluez",
        "/",
        &["org.freedesktop.DBus.ObjectManager.GetManagedObjects"],
    )?;
    let transport = transport(&objects, address).ok_or(())?;
    let pcm = pcm_path(&transport, address).ok_or(())?;
    Ok(Link { pcm, transport })
}
/// BlueALSA's PCM of the device whose BlueZ transport this is. BlueZ files a transport under
/// the device (`dev_XX/fd0`) or, once it knows the headset's audio endpoints, under one of them
/// (`dev_XX/sep1/fd0`): BlueALSA's PCM is always under the device.
fn pcm_path(transport: &str, address: &str) -> Option<String> {
    let component = format!("/dev_{}", address.replace(':', "_").to_ascii_uppercase());
    let end = transport.find(&format!("{component}/"))? + component.len();
    let device = &transport[..end];
    Some(format!(
        "{}/a2dpsrc/sink",
        device.replacen("/org/bluez/", "/org/bluealsa/", 1)
    ))
}
/// Headsets keep fewer volume steps than AVRCP's 128, commonly 16: they confirm the step
/// nearest to the volume set, up to one step away.
const HEADSET_STEP: u8 = 8;
fn write(bus: &Bus, link: &Link, value: u8) -> Result<u8, ()> {
    let packed = u16::from(value) * 257;
    set(bus, link, "Volume", &format!("variant:uint16:{packed}"))?;
    // BlueALSA can swallow a failed native write. Require BlueZ readback before
    // removing software attenuation. The headset confirms a moment later, in its own steps.
    let mut actual = remote(bus, link)?;
    for _ in 0..5 {
        if actual.abs_diff(value) <= HEADSET_STEP {
            return Ok(actual);
        }
        if !cfg!(test) {
            std::thread::sleep(Duration::from_millis(200));
        }
        actual = remote(bus, link)?;
    }
    if actual.abs_diff(value) <= HEADSET_STEP { Ok(actual) } else { Err(()) }
}
fn sync(
    address: &str,
    existing: Option<Link>,
    raw: Option<u8>,
    level: u8,
    changed: bool,
) -> Result<Synced, &'static str> {
    sync_with(&call, address, existing, raw, level, changed)
}
/// Links the headset's own volume to slot's. slot's volume wins when the headset connects, and
/// whenever slot's buttons change it; the headset's own buttons change slot's. Nothing here
/// ever scales the headset's volume by slot's: a link that fails and is retried must leave the
/// headset's volume as it was, not lower it on every retry.
fn sync_with(
    bus: &Bus,
    address: &str,
    existing: Option<Link>,
    known: Option<u8>,
    level: u8,
    changed: bool,
) -> Result<Synced, &'static str> {
    let first = existing.is_none();
    let link = match existing {
        Some(link) => link,
        None => discover(bus, address).map_err(|_| "no Bluetooth audio transport for it yet")?,
    };
    let result = (|| {
        // Optional property: absence means no native support (or none negotiated yet).
        let current = remote(bus, &link)
            .map_err(|_| "the headset reports no volume of its own (AVRCP absolute volume)")?;
        if first {
            set(bus, &link, "SoftVolume", "variant:boolean:false")
                .map_err(|_| "BlueALSA refused to hand the volume to the headset")?;
        }
        if first || changed {
            let raw = write(bus, &link, native(level))
                .map_err(|_| "the headset did not take the volume set")?;
            return Ok(Synced { link: link.clone(), raw, level, remote: false });
        }
        if known.is_some_and(|known| known != current) {
            return Ok(Synced { link: link.clone(), raw: current, level: percent(current), remote: true });
        }
        Ok(Synced { link: link.clone(), raw: current, level, remote: false })
    })();
    if result.is_err() {
        // Fall back to Slot's original software gain. Never raise remote volume.
        if set(bus, &link, "SoftVolume", "variant:boolean:true").is_ok() {
            let _ = set(bus, &link, "Volume", "variant:uint16:32639");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scripted(
        steps: Vec<(&'static str, Result<&'static str, ()>)>,
    ) -> impl Fn(&str, &str, &[&str]) -> Result<String, ()> {
        let steps = std::cell::RefCell::new(std::collections::VecDeque::from(steps));
        move |_, _, args| {
            let (expected, reply) = steps
                .borrow_mut()
                .pop_front()
                .expect("unexpected D-Bus request");
            assert!(
                args.join(" ").contains(expected),
                "{args:?} should contain {expected}"
            );
            reply.map(str::to_owned)
        }
    }
    const OBJECTS: &str = "object path \"/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/fd0\"";
    #[test]
    fn connecting_sets_the_headset_to_slots_volume_in_its_own_steps() {
        let bus = scripted(vec![
            ("GetManagedObjects", Ok(OBJECTS)),
            ("string:Volume", Ok("variant uint16 64")),
            ("variant:boolean:false", Ok("")),
            ("variant:uint16:8224", Ok("")), // slot's 50% = 32
            ("string:Volume", Ok("variant uint16 64")), // not confirmed yet
            ("string:Volume", Ok("variant uint16 34")), // the headset's nearest step
        ]);
        let synced = sync_with(&bus, "AA:BB:CC:DD:EE:FF", None, None, 50, false).unwrap();
        assert_eq!((synced.raw, synced.level, synced.remote), (34, 50, false));
    }
    #[test]
    fn a_retried_link_never_lowers_the_headset_again() {
        // Each failed attempt falls back to software volume. The next attempt used to read
        // that as a migration and scale the headset's volume by slot's, every 10 s, until
        // the headset was all but silent.
        for _ in 0..3 {
            let mut steps = vec![
                ("GetManagedObjects", Ok(OBJECTS)),
                ("string:Volume", Ok("variant uint16 100")),
                ("variant:boolean:false", Ok("")),
                ("variant:uint16:8224", Ok("")), // always slot's 50%, never less
            ];
            steps.extend(std::iter::repeat_n(("string:Volume", Ok("variant uint16 100")), 6));
            steps.push(("variant:boolean:true", Ok("")));
            steps.push(("variant:uint16:32639", Ok("")));
            assert!(sync_with(&scripted(steps), "AA:BB:CC:DD:EE:FF", None, None, 50, false).is_err());
        }
    }
    #[test]
    fn native_write_must_be_confirmed_before_bypassing_software() {
        let mut steps = vec![
            ("string:Volume", Ok("variant uint16 64")),
            ("variant:uint16:8224", Ok("")), // desired 50% = 32
        ];
        steps.extend(std::iter::repeat_n(("string:Volume", Ok("variant uint16 64")), 6)); // ignored
        steps.push(("variant:boolean:true", Ok("")));
        steps.push(("variant:uint16:32639", Ok(""))); // software unity, no remote change
        assert!(sync_with(&scripted(steps), "unused", linked().link, Some(64), 50, true).is_err());
    }
    #[test]
    fn headphone_buttons_are_read_without_writing_back() {
        let bus = scripted(vec![("string:Volume", Ok("variant uint16 90"))]);
        let synced = sync_with(&bus, "unused", linked().link, Some(32), 50, false).unwrap();
        assert_eq!((synced.level, synced.remote), (percent(90), true));
        // slot's own write, confirmed in the headset's step, is not mistaken for its buttons
        let bus = scripted(vec![("string:Volume", Ok("variant uint16 34"))]);
        let synced = sync_with(&bus, "unused", linked().link, Some(34), 50, false).unwrap();
        assert_eq!((synced.level, synced.remote), (50, false));
    }
    #[test]
    fn unsupported_transport_retains_software_control() {
        let bus = scripted(vec![
            ("GetManagedObjects", Ok(OBJECTS)),
            ("string:Volume", Err(())),
            ("variant:boolean:true", Ok("")),
            ("variant:uint16:32639", Ok("")),
        ]);
        assert!(sync_with(&bus, "AA:BB:CC:DD:EE:FF", None, None, 50, false).is_err());
    }
    #[test]
    fn the_pcm_is_the_devices_also_when_the_transport_is_under_an_endpoint() {
        let address = "AA:BB:CC:DD:EE:FF";
        let pcm = "/org/bluealsa/hci0/dev_AA_BB_CC_DD_EE_FF/a2dpsrc/sink";
        for transport_path in [
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/fd0",
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/sep1/fd3",
        ] {
            let text = format!("object path \"/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/sep1\"\nobject path \"{transport_path}\"");
            let found = transport(&text, address).unwrap();
            assert_eq!(found, transport_path);
            assert_eq!(pcm_path(&found, address).as_deref(), Some(pcm));
        }
    }
    #[test]
    fn native_curve_and_roundtrip() {
        assert_eq!(native(0), 0);
        assert_eq!(native(100), 127);
        assert_eq!(native(50), 32);
        for value in 0..=127 {
            assert!(native(percent(value)).abs_diff(value) <= 1);
        }
    }
    #[test]
    fn transport_is_selected_device_only() {
        let text = "object path \"/org/bluez/hci1/dev_AA_BB_CC_DD_EE_FF\"\nobject path \"/org/bluez/hci1/dev_AA_BB_CC_DD_EE_FF/fd2\"";
        assert_eq!(
            transport(text, "AA:BB:CC:DD:EE:FF").unwrap(),
            "/org/bluez/hci1/dev_AA_BB_CC_DD_EE_FF/fd2"
        );
        assert!(transport(text, "00:11:22:33:44:55").is_none());
        assert_eq!(uint16("variant uint16 63"), Ok(63));
        assert!(uint16("variant boolean true").is_err());
    }
    fn linked() -> Volume {
        Volume {
            address: Some("AA:BB:CC:DD:EE:FF".into()),
            link: Some(Link {
                pcm: "pcm".into(),
                transport: "transport".into(),
            }),
            observed: Some(50),
            ..Default::default()
        }
    }
    #[test]
    fn mute_and_other_outputs_never_bypass_software() {
        let v = linked();
        assert_eq!(v.software_volume(v.address.as_deref(), 50), 100);
        assert_eq!(v.software_volume(v.address.as_deref(), 0), 0);
        assert_eq!(v.software_volume(None, 20), 20);
        assert_eq!(v.software_volume(Some("other"), 20), 20);
    }
    #[test]
    fn newer_button_press_wins_over_remote_feedback() {
        let mut v = linked();
        let (tx, rx) = mpsc::channel();
        tx.send(Reply {
            generation: 0,
            requested: 50,
            result: Ok(Synced { link: v.link.clone().unwrap(), raw: 51, level: 40, remote: true }),
            detail: String::new(),
        })
        .unwrap();
        v.pending = Some(rx);
        assert_eq!(v.poll(Some("AA:BB:CC:DD:EE:FF"), 60), None);
    }
    #[test]
    fn stale_route_reply_is_discarded() {
        let mut v = linked();
        let (tx, rx) = mpsc::channel();
        tx.send(Reply {
            generation: 0,
            requested: 50,
            result: Ok(Synced { link: v.link.clone().unwrap(), raw: 103, level: 90, remote: true }),
            detail: String::new(),
        })
        .unwrap();
        v.pending = Some(rx);
        assert_eq!(v.poll(None, 20), None);
        assert!(v.link.is_none());
        assert_eq!(v.software_volume(None, 20), 20);
    }
}
