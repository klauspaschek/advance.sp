//! Run against the real bundled cores on the ARM64 PR builder.
use slot_retro::{AchievementLayout, ButtonMask, LibretroCore, RetroCore};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Libretro allows one open core per process; the tests take turns.
static CORE: Mutex<()> = Mutex::new(());
fn rom() -> PathBuf {
    let mut data = vec![0u8; 0x8000];
    data[..4].copy_from_slice(&0xea00002eu32.to_le_bytes());
    data[0xa0..0xac].copy_from_slice(b"SLOT MEMMAP\0");
    data[0xac..0xb0].copy_from_slice(b"SLMM");
    data[0xb0..0xb2].copy_from_slice(b"00");
    data[0xb2] = 0x96;
    let sum = data[0xa0..0xbd].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    data[0xbd] = 0u8.wrapping_sub(sum).wrapping_sub(0x19);
    data[0xc0..0xc4].copy_from_slice(&0xeafffffeu32.to_le_bytes());
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("achievement-memory.gba");
    std::fs::write(&path, data).unwrap();
    path
}
#[test]
fn both_bundled_gba_cores_expose_complete_work_ram_snapshots() {
    let _turn = CORE.lock().unwrap_or_else(|e| e.into_inner());
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor");
    for name in ["mgba", "gpsp"] {
        let path = vendor.join(format!(
            "{name}_libretro.{}",
            std::env::consts::DLL_EXTENSION
        ));
        if !path.exists() {
            assert!(
                std::env::var_os("SLOT_REQUIRE_CORES").is_none(),
                "missing bundled core: {}",
                path.display()
            );
            continue;
        }
        let mut core = LibretroCore::open(&path).unwrap();
        core.load(&rom()).unwrap();
        for _ in 0..3 {
            core.run_frame(ButtonMask(0));
        }
        let mut ram = vec![0; 0x58000];
        let valid = core.achievement_memory(AchievementLayout::Gba, &mut ram);
        assert_eq!(
            &valid[..2],
            &[0x8000, 0x40000],
            "{name} is missing achievement work RAM"
        );
    }
}

/// A CGB-compatible MBC1 cart with 32 KiB RAM that stores 0x5a at $C123, then spins.
fn gb_rom() -> PathBuf {
    const LOGO: [u8; 48] = [
        0xce, 0xed, 0x66, 0x66, 0xcc, 0x0d, 0x00, 0x0b, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0c, 0x00,
        0x0d, 0x00, 0x08, 0x11, 0x1f, 0x88, 0x89, 0x00, 0x0e, 0xdc, 0xcc, 0x6e, 0xe6, 0xdd, 0xdd,
        0xd9, 0x99, 0xbb, 0xbb, 0x67, 0x63, 0x6e, 0x0e, 0xec, 0xcc, 0xdd, 0xdc, 0x99, 0x9f, 0xbb,
        0xb9, 0x33, 0x3e,
    ];
    let mut data = vec![0u8; 0x8000];
    data[0x100..0x104].copy_from_slice(&[0x00, 0xc3, 0x50, 0x01]);
    data[0x104..0x134].copy_from_slice(&LOGO);
    data[0x134..0x13f].copy_from_slice(b"SLOT MEMMAP");
    data[0x143] = 0x80;
    data[0x147] = 0x03;
    data[0x149] = 0x03;
    data[0x14d] = data[0x134..0x14d].iter().fold(0u8, |x, b| x.wrapping_sub(*b).wrapping_sub(1));
    data[0x150..0x157].copy_from_slice(&[0x3e, 0x5a, 0xea, 0x23, 0xc1, 0x18, 0xfe]);
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("achievement-memory.gbc");
    std::fs::write(&path, data).unwrap();
    path
}

#[test]
fn bundled_mgba_exposes_the_retroachievements_gb_address_space() {
    let _turn = CORE.lock().unwrap_or_else(|e| e.into_inner());
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../vendor/mgba_libretro.{}", std::env::consts::DLL_EXTENSION));
    if !path.exists() {
        assert!(std::env::var_os("SLOT_REQUIRE_CORES").is_none(), "missing bundled core: {}", path.display());
        return;
    }
    let mut core = LibretroCore::open(&path).unwrap();
    core.load(&gb_rom()).unwrap();
    for _ in 0..3 {
        core.run_frame(ButtonMask(0));
    }
    let mut ram = vec![0xffu8; 0x58000];
    let valid = core.achievement_memory(AchievementLayout::Gb, &mut ram);
    assert_eq!(valid, [0x10000, 0x6000, 0x6000], "bus, CGB work RAM banks 2-7, cart RAM banks 1-3");
    assert_eq!(&ram[0x104..0x108], &[0xce, 0xed, 0x66, 0x66], "ROM bank 0 at $0000");
    assert_eq!(ram[0xc123], 0x5a, "work RAM at $C000");
    assert_eq!(ram[0xe123], 0x5a, "echo RAM mirrors $C000");
    assert!(ram[0x34000..].iter().all(|b| *b == 0xff), "nothing beyond the GB address space");
}
