# Hardware Validation & Known Issues

> Tracks deferred hardware tests and known bugs for resolution before v2.0 milestone close.

---

## Known Bugs (next phase)

### BUG-1: DHCP DNS not overriding to portal IP
**Symptom:** Devices connecting to `GNSS-Setup` SoftAP receive `8.8.8.8` as DNS server from DHCP, not `192.168.71.1`. DNS queries for random hostnames time out instead of being hijacked to the portal.
**Attempted fix:** `esp_netif_dhcps_stop` / `set_dns_info` / `esp_netif_dhcps_start` after `wait_netif_up()` — had no effect. `esp-idf-svc` appears to reinitialise the DHCP server after `wait_netif_up()`, overwriting the configuration.
**Likely fix:** Inject a pre-configured `EspNetif` via `swap_netif_ap()` before calling `wifi.start()`, or find the correct hook point after DHCP server is finalised. Needs investigation.
**Impact:** Android captive portal detection fails (DNS timeout); DNS hijack thread on port 53 is bypassed.

### BUG-2: Android captive portal notification not triggering
**Symptom:** Connecting Android device to `GNSS-Setup` does not show "Sign in to network" notification.
**Root causes:**
1. `/generate_204` was returning `200 OK` with HTML — fixed (now returns `302` redirect). Untested post-fix.
2. DNS not resolving to portal IP (see BUG-1) — Android probe to `connectivitycheck.gstatic.com` times out before reaching the HTTP handler.
3. Android 7+ also sends an HTTPS probe; TLS will fail (no valid cert for gstatic.com). If the HTTP probe works after BUG-1 is fixed, this may not matter. If it still fails, consider responding to port 853 (DoT) with an immediate close to force fallback.
**Blocked by:** BUG-1.

### BUG-3: NVS versioning — new fields default to wrong values after OTA
**Symptom:** OTA from old firmware succeeded but device could not connect to MQTT post-reboot. Root cause: TLS-enabled flag was absent from old NVS config and defaulted to `true`, causing mbedTLS handshake failure (reported as `select()` error).
**Fix required:**
- Add NVS schema version key (e.g. `config_ver: u8`) to the "prov" namespace
- On deserialise, check version and apply correct field defaults for any keys absent in older saves
- All future NVS schema changes must increment the version
- Fields added in this milestone that may be affected: TLS enabled flag (default should be `false` for plain MQTT)

### BUG-4: OTA post-reboot MQTT failure (NVS issue)
**Symptom:** OTA completed successfully (download, SHA-256 verify, reboot, `mark_running_slot_valid`), but device could not connect to MQTT on the new image.
**Root cause:** BUG-3 (NVS versioning). Old config had no TLS field → defaulted to enabled → MQTT broker connection failed with TLS error.
**Fix:** Resolve BUG-3.

---

## Feature Requests (next phase)

### FEAT-1: Boot button rework
**Current behaviour:** Hold GPIO9 for 3 seconds → trigger SoftAP.
**Requested behaviour:**
- Hold 3s → LED flashes (warning indication)
- Release while flashing → enter SoftAP mode
- Continue holding to 10s → LED turns off (danger zone)
- Release at 10s → clear NVS and reboot to factory reset

---

## Hardware Validation Checklist (deferred from Phase 18)

These require a hardware session on device FFFEB5 once the bugs above are resolved.

### Part A — OTA Firmware Update (MAINT-03)

Subscribe first: `gnss/FFFEB5/ota/status`, `gnss/FFFEB5/log`, `gnss/FFFEB5/heartbeat`

Serve firmware:
```bash
cd /home/ben/github.com/bharrisau/esp32-gnssmqtt
python3 -m http.server 8080
```

Publish OTA trigger (retained, QoS 1):
```
Topic:   gnss/FFFEB5/ota/trigger
Payload: {"url":"http://<YOUR_IP>:8080/esp32-gnssmqtt-canary.bin","sha256":"a395675b9d8fc951070100dfedacedc27881eb0585be11a6d52543aeac611dda"}
```

Expected: `downloading` → reboot → `mark_running_slot_valid` in `/log` → heartbeat with GNSS fields.

Clear retained trigger after success (empty payload, retained).

- [ ] OTA download completed without error
- [ ] Device rebooted and reconnected to MQTT
- [ ] `mark_running_slot_valid` in `/log`
- [ ] Retained trigger cleared
- [ ] **Requires BUG-3/BUG-4 fix first**

### Part B — Heartbeat GNSS Fields (TELEM-01)

Subscribe to `gnss/FFFEB5/heartbeat` and verify:
```json
{ "fix_type": 4, "satellites": 12, "hdop": 0.8 }
```
- `fix_type`: 0=No fix, 1=GPS, 2=DGPS, 4=RTK Fixed, 5=RTK Float, 6=Dead reckoning
- All three fields `null` before first GGA sentence received

- [ ] `fix_type`, `satellites`, `hdop` present in heartbeat JSON
- [ ] Fields `null` before GNSS lock
- [ ] `fix_type=4` when RTK correction active

### Part C — SoftAP Captive Portal

Trigger SoftAP: hold GPIO9 low 3s, or publish `"softap"` to `gnss/FFFEB5/ota/trigger`

On Android and iOS:
1. Connect to `GNSS-Setup` (open, no password)
2. Verify "Sign in to network" notification appears
3. Confirm provisioning form renders in browser
4. Allow 300s timeout or reboot to restore normal operation

- [ ] Android: captive portal notification shown — **requires BUG-1 + BUG-2 fix**
- [ ] iOS: captive portal notification shown
- [ ] Provisioning form renders correctly on mobile browser

### Decision: Canary Version Line

After OTA validation:
- **Keep**: leave `esp32-gnssmqtt v2.0-ota-canary` in `src/main.rs`
- **Revert**: remove canary suffix before milestone tag

---

## Post-v2.0 Backlog

### POST-1: espflash does not boot newly flashed app after OTA
**Symptom:** After an OTA update, `cargo espflash flash` writes to slot 0 but the bootloader continues to boot slot 1 (the OTA slot). The newly flashed binary does not run until the flash is erased first.
**Workaround:** `cargo espflash erase-flash` before flashing. Documented in README.
**Fix required:** Investigate whether `espflash` can reset the OTA boot selection (e.g. by writing a valid `otadata` partition that marks slot 0 as active), or whether the partition table / bootloader needs a flag. Alternatively, expose a firmware command that clears `otadata` and reboots so a subsequent flash takes effect without erasing.
