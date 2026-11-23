# No active work

Latest release `build-2026.11.23-d8383a6`: the 75% boot logo redrawn with its scanlines and colour fringe intact (PR #10; WORKLOG section "Boot logo redrawn at 75% — 2026-11-23").


advance.sp is renamed throughout (PR #6, `be330d6`), and the regression workflows pass again (PR #8, `5b6de7f`, released as `build-2026.11.23-5b6de7f`). History: WORKLOG.md, sections "HDMI out — 2026-11-22" (rename and release entries at its end) and "CI regression fixes — 2026-11-23".

Pending device checks: the redrawn boot logo, the boot handoff, the Updates page layout, and file sharing with user `advance`.

Known local-only test failures (CI does not run them): e2e `the_whole_pass_from_boot_to_resume`, four layout tests, render_link_screen (2), swap (timing), slot-ui letters (1).
