---
phase: 07-rtcm-relay
plan: 02
subsystem: gnss
tags: [rtcm3, uart, state-machine, crc, mqtt, esp32, embedded-rust]

# Dependency graph
requires:
  - phase: 05-nmea-relay
    provides: nmea_relay.rs spawn_relay pattern and gnss.rs channel architecture
provides:
  - RxState four-state machine in gnss.rs handling mixed NMEA+RTCM byte streams
  - CRC-24Q verification function for RTCM3 frame validation
  - RTCM channel: Receiver<(u16, Vec<u8>)> as third return from spawn_gnss
  - rtcm_relay.rs with spawn_relay consuming the RTCM receiver
affects:
  - 07-03 (main.rs wiring of rtcm_rx into rtcm_relay::spawn_relay)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "RxState enum state machine: byte-at-a-time dispatch replacing line-buffer accumulator"
    - "Box<[u8; N]> for large fixed-size buffers inside enum variants (avoids stack overflow)"
    - "CRC-24Q: polynomial 0x864CFB, computed over header+payload, verified before channel send"
    - "Bounded mpsc::sync_channel with try_send + warn/error log on Full/Disconnected"

key-files:
  created:
    - src/rtcm_relay.rs
  modified:
    - src/gnss.rs

key-decisions:
  - "Box<[u8; 1029]> for RtcmBody buffer: 1029-byte array on stack risks overflow at 8192 stack; heap allocation is the safe approach even with 12288 stack"
  - "Complete RTCM frame published (preamble+header+payload+CRC), not just payload: downstream consumers can independently verify CRC"
  - "Stack size increased from 8192 to 12288 for RX thread: Belt-and-suspenders given Box usage"
  - "RTCM channel bounded at 32 slots: at 1-4 frames/sec, full channel means relay is stalled"

patterns-established:
  - "State machine pattern: match state { RxState::X => ... } returns new state each byte"
  - "relay.rs pattern: spawn_relay(client, device_id, rx) -> anyhow::Result<()>, thread per relay"

requirements-completed: [RTCM-01, RTCM-02, RTCM-03, RTCM-04]

# Metrics
duration: 6min
completed: 2026-03-07
---

# Phase 7 Plan 02: RTCM Relay Core Implementation Summary

**Four-state RxState machine in gnss.rs handles mixed NMEA+RTCM byte streams with CRC-24Q verification; rtcm_relay.rs publishes verified frames to gnss/{device_id}/rtcm/{message_type}**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-07T03:11:07Z
- **Completed:** 2026-03-07T03:17:25Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Replaced line-buffer RX loop with RxState byte-at-a-time state machine (Idle/NmeaLine/RtcmHeader/RtcmBody)
- Implemented CRC-24Q (polynomial 0x864CFB) for RTCM3 frame verification before channel send
- Added bounded RTCM channel as third return value from spawn_gnss; updated return type accordingly
- Created rtcm_relay.rs mirroring nmea_relay.rs pattern exactly, publishing complete raw frames

## Task Commits

Each task was committed atomically:

1. **Task 1: Replace gnss.rs line-buffer loop with RxState state machine** - `7c01622` (feat)
2. **Task 2: Create rtcm_relay.rs** - `f102c93` (feat)

**Plan metadata:** (docs commit — see below)

## Files Created/Modified
- `src/gnss.rs` - RxState enum, crc24q(), updated spawn_gnss return type with rtcm_rx third element
- `src/rtcm_relay.rs` - spawn_relay consuming Receiver<(u16, Vec<u8>)>, publishes to MQTT

## Decisions Made
- Used `Box<[u8; 1029]>` for the RtcmBody frame buffer to avoid stack overflow; even with 12288 stack headroom, heap allocation is safer for 1029-byte array inside an enum variant
- Published complete RTCM frame (preamble + header + payload + CRC bytes) not just payload, so downstream consumers can independently verify the CRC
- Stack size increased from 8192 to 12288 for additional headroom per research recommendation
- RTCM channel bounded at 32 slots (at 1-4 frames/sec, full = stalled relay)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- `rustfmt` not available for active nightly toolchain; used `rustfmt +stable` successfully — file formatted correctly.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- gnss.rs and rtcm_relay.rs are ready; main.rs currently fails to compile because spawn_gnss now returns a 3-tuple
- Plan 03 wires rtcm_rx into rtcm_relay::spawn_relay in main.rs to resolve the compile error
- All RTCM-01 through RTCM-04 requirements are implemented in this plan

---
*Phase: 07-rtcm-relay*
*Completed: 2026-03-07*
