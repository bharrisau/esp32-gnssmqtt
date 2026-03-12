---
phase: 25-web-ui-remaining-gap-crate-skeletons
plan: 01
subsystem: ui
tags: [axum, websocket, nmea, serde_json, tower-http, broadcast, svg, html]

# Dependency graph
requires:
  - phase: 24-rinex-files-gnss-ota-gap-crate
    provides: gnss-server crate baseline with RTCM+RINEX pipeline and MqttMessage types
provides:
  - GsvAccumulator, SatInfo, SatelliteState types in nmea_parse.rs
  - tag_heartbeat utility for heartbeat JSON tagging
  - run_web_server async fn and AppState in web_server.rs
  - static/index.html with skyplot SVG, SNR bar chart, health panel, WebSocket client
  - http_port field on ServerConfig (default 8080)
affects: [25-02-PLAN.md]

# Tech tracking
tech-stack:
  added:
    - axum 0.7 (ws feature) — HTTP + WebSocket server
    - tower-http 0.5 — HTTP middleware (fs feature for future use)
    - serde_json 1 — JSON serialization for WebSocket messages
    - nmea 0.7 — NMEA GSV sentence parsing
  patterns:
    - broadcast::Sender<String> fan-out for WebSocket multi-client
    - include_str! for compile-time embedding of static HTML
    - GsvAccumulator multi-sentence state machine pattern
    - TDD: failing tests written before implementation

key-files:
  created:
    - gnss-server/src/nmea_parse.rs
    - gnss-server/src/web_server.rs
    - gnss-server/static/index.html
  modified:
    - gnss-server/Cargo.toml
    - gnss-server/src/config.rs
    - gnss-server/src/main.rs

key-decisions:
  - "GP talker (not GN) used in nmea_parse tests — nmea 0.7 parse_gsv rejects GN talker (UnknownGnssType error); GN is a combined-constellation talker not mapped by the crate"
  - "index.html embedded via include_str! (single binary, no runtime file dependency) — tower-http ServeDir not used"
  - "Dead code allows on public items in nmea_parse.rs and web_server.rs — plan 25-02 wires them; clippy -D warnings satisfied without suppressing the lint globally"
  - "broadcast::Sender<String> is the fan-out primitive — matches plan's stated contract for 25-02 wiring"

patterns-established:
  - "GsvAccumulator.feed() returns None for non-GSV sentences and for mid-group sentences; emits SatelliteState only when sentence_num == number_of_sentences"
  - "WebSocket handle_socket: tokio::select! on rx.recv() and socket.recv(); Lagged errors skipped silently; Closed and send errors break loop"

requirements-completed: [UI-01, UI-02, UI-03, UI-04]

# Metrics
duration: 10min
completed: 2026-03-12
---

# Phase 25 Plan 01: Web UI Infrastructure Summary

**axum 0.7 HTTP+WebSocket server with NMEA GSV accumulator, broadcast fan-out, and self-contained browser skyplot/SNR/health UI embedded via include_str!**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-12T09:06:33Z
- **Completed:** 2026-03-12T09:16:00Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- GsvAccumulator parses multi-sentence GSV groups and emits SatelliteState with all satellite info
- axum web server with broadcast fan-out to all WebSocket clients via AppState.ws_tx
- Browser HTML page with polar skyplot SVG renderer, SNR bar chart, health panel, auto-reconnect WebSocket
- 38 total tests passing; clippy -D warnings clean

## Task Commits

Each task was committed atomically:

1. **Task 1: NMEA GSV accumulator and satellite state types** - `3a09420` (feat)
2. **Task 2: axum web server module and browser HTML/JS** - `0795bd2` (feat)

**Plan metadata:** (docs commit — see below)

## Files Created/Modified
- `gnss-server/src/nmea_parse.rs` — GsvAccumulator, SatInfo, SatelliteState, tag_heartbeat; 6 unit tests
- `gnss-server/src/web_server.rs` — run_web_server, AppState, handle_socket, ws_handler, index_handler; compile-time HTML test
- `gnss-server/static/index.html` — drawSkyplot, drawSnrChart, gnssColor, updateHealth, WebSocket client JS
- `gnss-server/Cargo.toml` — added axum 0.7 (ws), tower-http 0.5, serde_json 1 (nmea 0.7 already present)
- `gnss-server/src/config.rs` — added http_port field (default 8080)
- `gnss-server/src/main.rs` — added mod nmea_parse and mod web_server declarations

## Decisions Made
- **nmea 0.7 GN talker unsupported:** `parse_gsv` returns `Err(UnknownGnssType("GN"))` for combined-constellation `$GN` talker sentences. Test sentences use `GP` talker; in production, GN sentences gracefully return `None` via `.ok()?`. This is correct behaviour — each satellite's actual constellation is set per-satellite in GsvData anyway.
- **include_str! over ServeDir:** Static HTML embedded at compile time; no runtime file dependency; simplest path for single-binary deployment.
- **Dead code allows on public items:** nmea_parse.rs and web_server.rs items are forward-compat public API for plan 25-02. `#[allow(dead_code)]` per item (not global) keeps clippy clean without silencing real issues.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] nmea 0.7 GN talker not supported by parse_gsv**
- **Found during:** Task 1 (TDD RED phase)
- **Issue:** Plan specified `$GNGSV` test sentences; nmea 0.7 `parse_gsv` does not map `GN` talker to a GnssType variant — returns `UnknownGnssType("GN")` error
- **Fix:** Changed test sentences to use `GP` talker with recalculated checksums; production feed() handles this via `.ok()?` (returns None for unparseable sentences)
- **Files modified:** gnss-server/src/nmea_parse.rs
- **Verification:** All 6 nmea_parse tests pass after fix
- **Committed in:** 3a09420 (Task 1 commit)

**2. [Rule 1 - Bug] Clippy useless_conversion on Message::Text**
- **Found during:** Task 2 (clippy pass)
- **Issue:** `Message::Text(msg.into())` — `.into()` is a no-op (String to String); clippy -D warnings flags it
- **Fix:** Changed to `Message::Text(msg)` directly
- **Files modified:** gnss-server/src/web_server.rs
- **Committed in:** 0795bd2 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 Rule 1 bugs)
**Impact on plan:** Both fixes necessary for correctness and clippy compliance. No scope creep.

## Issues Encountered
- nmea::parse module is `pub(crate)` — `parse_nmea_sentence` must be imported from the crate root (`nmea::parse_nmea_sentence`) not `nmea::parse::parse_nmea_sentence`. Caught at first compile.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plan 25-02 can wire `AppState.ws_tx` into `run_decode_task` and start `run_web_server`
- `GsvAccumulator.feed()` and `tag_heartbeat()` are ready for use in 25-02
- All public APIs are stable; no breaking changes expected

---
*Phase: 25-web-ui-remaining-gap-crate-skeletons*
*Completed: 2026-03-12*
