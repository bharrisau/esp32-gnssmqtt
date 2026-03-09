---
phase: 19-pre-2-0-bugfix
plan: "01"
subsystem: provisioning
tags: [softap, dhcp, dns, captive-portal, bug-fix]
dependency_graph:
  requires: []
  provides: [BUG-1-fix, BUG-2-unblocked]
  affects: [src/provisioning.rs, src/main.rs]
tech_stack:
  added: []
  patterns:
    - EspNetif::new_with_conf with RouterConfiguration.dns for pre-start DHCP DNS config
    - WifiDriver::new + EspWifi::wrap_all instead of EspWifi::new for custom netif injection
key_files:
  created: []
  modified:
    - src/main.rs
    - src/provisioning.rs
decisions:
  - "RouterConfiguration.dns field triggers set_dns + OFFER_DNS during EspNetif construction — only lifecycle point that survives wifi.start()"
  - "Use WifiDriver::new + EspWifi::wrap_all to inject pre-configured ap_netif before wifi.start()"
  - "Remove post-wait_netif_up unsafe DHCP block entirely — superseded by construction-time config"
metrics:
  duration_minutes: 5
  tasks_completed: 2
  files_modified: 2
  completed_date: "2026-03-09"
---

# Phase 19 Plan 01: SoftAP DHCP DNS Fix Summary

**One-liner:** Pre-configured EspNetif via wrap_all injects DNS=192.168.71.1 into DHCP before wifi.start(), replacing the broken post-start unsafe block.

## What Was Built

Fixed BUG-1 (DHCP DNS override not surviving wifi.start) by replacing the SoftAP WiFi construction pattern in `main.rs` with `WifiDriver::new + EspWifi::wrap_all` and injecting a pre-configured `ap_netif` built via `EspNetif::new_with_conf`. The `RouterConfiguration { dns: Some(Ipv4Addr::new(192, 168, 71, 1)) }` field triggers `set_dns()` and `esp_netif_dhcps_option(OFFER_DNS)` during construction — before `wifi.start()` — which is the only lifecycle point that survives ESP-IDF's DHCP server reinitialisation on start.

BUG-2 (Android captive portal 302) was already committed and requires no code change; it is now unblocked because DNS will resolve correctly and probes will reach the HTTP handler.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Replace SoftAP WiFi construction in main.rs with WifiDriver+wrap_all | f714508 | src/main.rs |
| 2 | Remove unsafe DHCP block from provisioning.rs run_softap_portal | 38cded3 | src/provisioning.rs |

## Decisions Made

1. **Construction-time DNS config is the only correct approach** — `EspNetif::new_with_conf` with `RouterConfiguration.dns` calls `set_dns()` AND `esp_netif_dhcps_option(OFFER_DNS)` before netif is attached. Post-start calls via `esp_netif_dhcps_stop/set_dns_info/start` are discarded by ESP-IDF on wifi.start().

2. **WifiDriver::new + EspWifi::wrap_all** — This is the canonical pattern from esp-idf-svc for injecting custom netif objects. The STA netif uses the default `EspNetif::new(NetifStack::Sta)` since it is not used in AP mode.

3. **`..Default::default()` removed from RouterConfiguration** — clippy `-D warnings` flags `needless_update` when all fields are explicitly specified. All four RouterConfiguration fields (subnet, dhcp_enabled, dns, secondary_dns) are named explicitly.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed `..Default::default()` from RouterConfiguration**
- **Found during:** Task 1 clippy verification
- **Issue:** `clippy::needless_update` fired because all four fields of `RouterConfiguration` were explicitly specified — the struct update syntax was redundant
- **Fix:** Removed `..Default::default()` from the `RouterConfiguration { }` literal
- **Files modified:** src/main.rs
- **Commit:** f714508 (fixed before commit)

## Verification Results

- `cargo clippy -- -D warnings`: PASSED — zero warnings
- `cargo build --release`: PASSED
- `provisioning.rs::run_softap_portal`: no unsafe block for DHCP override (remaining unsafe calls are `esp_restart()` and `esp_wifi_ap_get_sta_list()` — unrelated, correct)
- `main.rs` SoftAP path: uses `WifiDriver::new` + `EspWifi::wrap_all` with pre-configured `ap_netif`
- `ap_netif` has `dns: Some(Ipv4Addr::new(192, 168, 71, 1))` in `RouterConfiguration`
- `/generate_204` handler: returns 302 Found with `Location: http://192.168.71.1/` (verified intact)
- `esp_netif_dhcps_stop` / `esp_netif_set_dns_info`: not present in provisioning.rs

## Self-Check: PASSED

- [x] src/main.rs — modified, committed at f714508
- [x] src/provisioning.rs — modified, committed at 38cded3
- [x] Both commits verified: `git log --oneline -3` shows f714508 and 38cded3
- [x] `/generate_204` 302 handler intact at provisioning.rs lines 259-262
