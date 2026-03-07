---
phase: 10-memory-diagnostics
plan: "02"
subsystem: gnss
tags: [rust, esp32, rtcm, mpsc, buffer-pool, heap, embedded]

# Dependency graph
requires:
  - phase: 10-01
    provides: stack HWM logging at all thread entry points
  - phase: 07-rtcm-relay
    provides: Box<[u8; 1029]> RtcmBody buffer, rtcm_relay spawn_relay, gnss::spawn_gnss 3-tuple return
provides:
  - Pre-allocated RTCM buffer pool (4 x Box<[u8; 1029]> = 4116 bytes, allocated once at init)
  - RtcmFrame type alias (u16, Box<[u8; 1029]>, usize) replacing (u16, Vec<u8>)
  - Pool-backed RTCM relay path: zero per-frame heap allocation in steady state
  - Pool exhaustion drop path with warn log; CRC-fail and channel-full paths return buffer to pool
affects:
  - phase 13 health telemetry (heap churn baseline now eliminated; pool starvation counter future work)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Fixed buffer pool via sync_channel seeded at init: avoids heap churn in hot path"
    - "Buffer lifecycle: acquire from pool in RX thread, release in relay thread after publish"
    - "Clone sender into closure for error return path (free_pool_tx_clone)"

key-files:
  created: []
  modified:
    - src/gnss.rs
    - src/rtcm_relay.rs
    - src/main.rs

key-decisions:
  - "RTCM_POOL_SIZE = 4: at 1-4 MSM7 frames/sec, 4 buffers provide ample headroom before relay drains; pool memory = 4116 bytes fixed"
  - "Pool exhaustion drops incoming frame + log::warn — no panic, no dynamic fallback allocation"
  - "Buffer returned to pool on ALL exit paths: channel full, channel disconnected, CRC mismatch, mutex poisoned"
  - "free_pool_tx cloned before RX closure for TrySendError::Full and Disconnected return paths; original returned to caller for rtcm_relay"
  - "frame_len passed as third tuple element (valid_byte_count) so relay slices frame_buf[..frame_len] without extra copy"

patterns-established:
  - "Pool via sync_channel: seed N buffers at init, take via try_recv before use, return via send after use"
  - "Every error path that owns a pool buffer must return it before transitioning to Idle"

requirements-completed:
  - HARD-03

# Metrics
duration: 4min
completed: 2026-03-07
---

# Phase 10 Plan 02: RTCM Buffer Pool Summary

**Pre-allocated fixed pool of 4 x Box<[u8; 1029]> buffers eliminates per-frame Vec heap allocation in the RTCM relay hot path**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-03-07T11:19:40Z
- **Completed:** 2026-03-07T11:23:27Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Eliminated `Vec::from(&buf[..expected])` and `Box::new([0u8; 1029])` per-frame allocations in gnss.rs RtcmBody path
- Introduced `RtcmFrame = (u16, Box<[u8; 1029]>, usize)` type alias; pool buffer circulates between GNSS RX and RTCM relay threads
- Pool exhaustion gracefully drops incoming frame with `log::warn!` instead of panicking or falling back to dynamic allocation
- All error paths (channel full, disconnect, CRC mismatch, mutex poisoned) return the buffer to the pool to prevent starvation

## Task Commits

Each task was committed atomically:

1. **Task 1: Add pool init and update gnss.rs channel type + state machine** - `9d401d3` (feat)
2. **Task 2: Update rtcm_relay.rs and main.rs for new channel type** - `b987d7e` (feat)

## Files Created/Modified

- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/gnss.rs` - Added `RtcmFrame` type alias, `RTCM_POOL_SIZE` constant, free pool channel seeded with 4 buffers, pool try_recv in RtcmHeader arm, direct Box send in RtcmBody arm; updated `spawn_gnss` to return 4-tuple
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/rtcm_relay.rs` - Updated `spawn_relay` signature to accept `Receiver<RtcmFrame>` and `SyncSender<Box<[u8; 1029]>>`; destructure 3-tuple, slice to `frame_len`, return buffer on all paths
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/main.rs` - Destructure 4-tuple from `spawn_gnss`; pass `free_pool_tx` to `rtcm_relay::spawn_relay`

## Decisions Made

- `RTCM_POOL_SIZE = 4`: at 1-4 MSM7 frames/sec, 4 buffers provide ample headroom; pool memory = 4 x 1029 = 4116 bytes allocated once at init
- Pool exhaustion drops frame + `log::warn!` — no panic, no dynamic fallback allocation (HARD-03 requirement)
- Buffer returned to pool on ALL exit paths to prevent starvation: channel full, channel disconnected, CRC mismatch, mutex poisoned
- `free_pool_tx` cloned before RX closure (`free_pool_tx_clone`) for error return paths; original sent back to caller for rtcm_relay
- `frame_len` carried as third tuple element so relay can slice `&frame_buf[..frame_len]` without any copy

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Return buffer to pool on CRC mismatch and Disconnected paths**

- **Found during:** Task 1 (gnss.rs state machine update)
- **Issue:** Plan's code template for the Disconnected arm discarded the buffer (`_`), and the CRC-fail path did not return the buffer at all — both would silently drain the pool over time, eventually causing all frames to be dropped with "pool exhausted"
- **Fix:** Added `free_pool_tx_clone.try_send(returned_buf)` in Disconnected arm; added `free_pool_tx_clone.try_send(buf)` after CRC mismatch log before `RxState::Idle`
- **Files modified:** `src/gnss.rs`
- **Verification:** All error paths audited; `cargo build --release` passes with zero errors
- **Committed in:** `9d401d3` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - correctness bug in error path)
**Impact on plan:** Fix required for correct pool operation — without it, pool would drain after CRC failures or channel reconnects.

## Issues Encountered

None beyond the deviation above. Build succeeded on first attempt after the fix.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- HARD-03 requirement satisfied: pre-allocated pool, no per-frame heap allocation, exhaustion drops with warn log
- Pool starvation counter (how many frames dropped due to exhaustion) not yet exposed — Phase 13 health telemetry can read this via atomic counter if needed
- Phase 10 complete (both plans done); Phase 11 watchdog heartbeat ready to proceed

---
*Phase: 10-memory-diagnostics*
*Completed: 2026-03-07*
