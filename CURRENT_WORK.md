# No active work

HDMI out is finished: merged into main as PR #1 (`8f061d13`) and released as `build-2026.11.22-8f061d1`, tested on an RG SP with a 1080p TV. With Bluetooth controllers and HDMI, advance.sp's "slot.sp on the couch" goal is complete. History: WORKLOG.md, section "HDMI out — 2026-11-22".

Known pre-existing CI failures on main, unrelated to HDMI: "Shortcut guide regression checks" (`shortcut_reference::adult_reference_is_reachable_from_both_menus_and_returns_without_changing_settings`) and "Launcher quality of life checks" (`examples/network_review.rs:70`). Locally also: e2e, four layout tests, render_link_screen (2), swap (timing), slot-ui letters (1).
