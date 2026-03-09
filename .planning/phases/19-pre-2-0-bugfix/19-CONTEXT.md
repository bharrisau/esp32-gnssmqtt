# Phase 19: pre-2.0-bugfix - Context

**Gathered:** 2026-03-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Fix known bugs blocking v2.0 milestone close, and deliver the boot button rework (FEAT-1). Scope:
- BUG-1: DHCP DNS override in SoftAP mode (portal IP not served to clients)
- BUG-2: Android captive portal detection (blocked by BUG-1; 302 fix untested)
- BUG-3/BUG-4: NVS schema — TLS default wrong after OTA from old firmware
- FEAT-1: Boot button rework (3s/10s hold thresholds)

Hardware validation (testing.md checklist) is NOT a Phase 19 gate — it happens after code ships. New bugs found during hardware testing become Phase 20.

</domain>

<decisions>
## Implementation Decisions

### BUG-1: DHCP DNS override
- Must be fixed properly — research the correct hook point in esp-idf-svc/esp-idf-sys
- Current approach (`esp_netif_dhcps_stop/set_dns_info/start` after `wait_netif_up()`) is wrong — DHCP server is reinitialised by esp-idf-svc after that point
- Investigate `swap_netif_ap()` with a pre-configured `EspNetif` before `wifi.start()`, or find the correct lifecycle hook
- If no clean `esp-idf-svc` solution exists after thorough research: hand off to user rather than implement a hack
- Unsafe `esp_netif_*` FFI is acceptable if it is the correct API and is isolated to `provisioning.rs`

### BUG-2: Android captive portal
- Unblocked once BUG-1 is fixed (DNS must resolve to portal IP before HTTP probe reaches handler)
- 302 redirect fix for `/generate_204` is already committed but untested — validate it works post BUG-1 fix
- Android HTTPS probe (port 443) may also fail (no cert) — if HTTP probe passes after BUG-1 fix, HTTPS failure is acceptable

### BUG-3/BUG-4: NVS versioning
- Fix the TLS default directly: wherever `mqtt_tls` key is read from NVS, ensure `unwrap_or(0)` (TLS off) not `unwrap_or(1)`
- Add a `config_ver: u8` key to the `"prov"` NVS namespace — initially set to `1` on every save
- `config_ver` is a convention for future breaking schema changes only — no migration logic needed now
- Fields added in this phase must default to `false`/`0`/off when absent from NVS

### FEAT-1: Boot button rework
- Hold GPIO9 for 3s → LED starts flashing (same GPIO15 active-low LED)
- Release while flashing → enter SoftAP mode (existing behaviour)
- Continue holding to 10s → LED stops flashing, turns off (danger zone signal)
- Release at 10s → erase NVS partition + reboot (factory reset — all credentials cleared)
- Factory reset = NVS erase only; does NOT revert OTA slot

### Phase completion gate
- Phase 19 closes when all code fixes are implemented and compile clean
- Hardware validation (testing.md) happens after as an informal session
- Bugs surfaced by hardware testing → Phase 20

### Claude's Discretion
- Exact LED flash rate/pattern during 3s–10s hold (any distinct flash pattern)
- How to handle button debounce in the state machine
- Whether to refactor existing GPIO9 polling into a cleaner state machine or patch in place

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/provisioning.rs`: GPIO9 polling loop (100ms interval, 3s threshold) — extend to handle 10s threshold
- `src/provisioning.rs`: NVS namespace `"prov"` — add `config_ver` and fix TLS default here
- `src/led.rs`: LED state machine with existing patterns — add flash pattern for button-hold state
- `esp_netif_dhcps_stop/start` + raw `esp_netif_set_dns_info` already imported in `provisioning.rs`

### Established Patterns
- NVS: individual typed keys per field (`get_u8`, `get_str`) — no serialisation; new keys default to absent
- DHCP DNS: currently set after `wait_netif_up()` — this is the wrong lifecycle point
- Button polling: 100ms sleep loop in `provisioning.rs`; 3s counted as 30 iterations

### Integration Points
- `load_mqtt_config()` in `provisioning.rs` — add TLS default fix here
- `save_mqtt_config()` / `save_wifi_networks()` in `provisioning.rs` — write `config_ver = 1` on save
- `src/led.rs` LED state — add new state for button-hold warning flash
- `main.rs` boot path — factory reset (NVS erase) must happen before any NVS reads

</code_context>

<specifics>
## Specific Ideas

- User wants BUG-1 researched properly — not a workaround. If esp-idf-svc doesn't expose the right hook, hand it back rather than hacking it.
- `config_ver` is a forward-looking convention, not active migration logic — just write it on save, read it for future use.

</specifics>

<deferred>
## Deferred Ideas

- Hardware validation checklist (testing.md) — informal session after Phase 19 ships; new bugs → Phase 20
- HTTPS captive portal probe handling (port 443/DoT) — acceptable to leave broken if HTTP probe works

</deferred>

---

*Phase: 19-pre-2-0-bugfix*
*Context gathered: 2026-03-09*
