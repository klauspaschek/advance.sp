//! A settings row's value never runs into its label, even when the lit label is set larger.
use slot_ui::{parent_settings_face_on, Canvas, UndoFace};

const ROWS: [(&str, &str); 4] = [
    ("Check for updates", "Up to date"),
    ("Installed advance.sp", "2026.11.23-ac46c35"),
    ("Installed BaseOS", "1.3.0"),
    ("BaseOS check", "Compatible"),
];

fn face(width: u32, selected: usize, at: usize, label: &str, value: &str) -> UndoFace {
    let rows: Vec<(String, String, bool)> = ROWS
        .iter()
        .enumerate()
        .map(|(i, (l, v))| if i == at { (label.into(), value.into(), false) } else { ((*l).into(), (*v).into(), false) })
        .collect();
    parent_settings_face_on(Canvas { w: width as f32, h: 480.0 }, &rows, selected, 0, "", 0.0)
}

/// Columns where `a` and `b` differ, within the rows of the screen given.
fn changed(a: &UndoFace, b: &UndoFace, rows: std::ops::Range<u32>) -> Vec<u32> {
    (0..a.w)
        .filter(|&x| rows.clone().any(|y| {
            let i = ((y * a.w + x) * 4) as usize;
            a.rgba[i..i + 4] != b.rgba[i..i + 4]
        }))
        .collect()
}

#[test]
fn a_long_value_keeps_a_gap_after_its_label_lit_or_not() {
    for width in [640, 720] {
        for selected in 0..ROWS.len() {
            for (at, (label, value)) in ROWS.iter().enumerate() {
                let blank = face(width, selected, at, "", "");
                let named = face(width, selected, at, label, " ");
                let full = face(width, selected, at, label, value);
                let all = changed(&full, &blank, 0..480);
                let (top, bottom) = (0..480u32)
                    .filter(|&y| (0..width).any(|x| {
                        let i = ((y * width + x) * 4) as usize;
                        full.rgba[i..i + 4] != blank.rgba[i..i + 4]
                    }))
                    .fold((480, 0), |(t, b), y| (t.min(y), b.max(y)));
                assert!(!all.is_empty() && top <= bottom, "row {at} drew nothing");
                let label_end = *changed(&named, &blank, top..bottom + 1).last().unwrap();
                let value_start = *changed(&full, &named, top..bottom + 1).first().unwrap();
                assert!(
                    value_start >= label_end + 12,
                    "{width}px, row {selected} lit: {value:?} starts at {value_start}, {label:?} ends at {label_end}"
                );
                if std::env::var_os("SLOT_VALUE_FIT_DUMP").is_some() && at == 0 {
                    let path = format!("{}/updates-{width}-{selected}.rgba", std::env::var("SLOT_VALUE_FIT_DUMP").unwrap());
                    std::fs::write(path, &full.rgba).unwrap();
                }
            }
        }
    }
}
