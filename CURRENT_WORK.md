# No active work

advance.sp is renamed throughout (PR #6, `be330d6`), and the regression workflows pass again (PR #8, `5b6de7f`, released as `build-2026.11.23-5b6de7f`). History: WORKLOG.md, sections "HDMI out — 2026-11-22" (rename and release entries at its end) and "CI regression fixes — 2026-11-23".

Pending device checks: install `5b6de7f` from General → Updates on the card running `be330d6` (first OTA using `advance.sp-System.zip`), the boot handoff, the Updates page layout, and file sharing with user `advance`.

Known local-only test failures (CI does not run them): e2e `the_whole_pass_from_boot_to_resume`, four layout tests, render_link_screen (2), swap (timing), slot-ui letters (1).
