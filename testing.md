# Phase 18 Hardware Validation Checklist

> Deferred from phase 18 execution. Complete before milestone v2.0 sign-off.

## Setup

```bash
# Serve canary firmware
cd /home/ben/github.com/bharrisau/esp32-gnssmqtt
python3 -m http.server 8080
```

SHA-256: `a395675b9d8fc951070100dfedacedc27881eb0585be11a6d52543aeac611dda`

> **Note:** The canary binary (`esp32-gnssmqtt-canary.bin`) was built during phase 18 execution.
> If it no longer exists, rebuild with `cargo build --release` and update the SHA-256 above.

---

## Part A — OTA Firmware Update (MAINT-03)

Subscribe first:
- `gnss/FFFEB5/ota/status`
- `gnss/FFFEB5/log`
- `gnss/FFFEB5/heartbeat`

Publish OTA trigger (retained, QoS 1):
```
Topic:   gnss/FFFEB5/ota/trigger
Payload: {"url":"http://<YOUR_IP>:8080/esp32-gnssmqtt-canary.bin","sha256":"a395675b9d8fc951070100dfedacedc27881eb0585be11a6d52543aeac611dda"}
```

Expected sequence:
1. `/ota/status` → `downloading`
2. Device reboots
3. `/log` → `esp32-gnssmqtt v2.0-ota-canary` (version line after reboot)
4. `/log` → `mark_running_slot_valid` (no rollback)
5. Device reconnects and publishes heartbeat

Clear retained trigger after success:
```
Topic:   gnss/FFFEB5/ota/trigger
Payload: (empty)
Retain:  true
```

### Checklist
- [ ] OTA download completed without error
- [ ] Device rebooted and reconnected to MQTT
- [ ] `esp32-gnssmqtt v2.0-ota-canary` in `/log` after reboot
- [ ] `mark_running_slot_valid` in `/log` (confirms no rollback)
- [ ] Retained trigger cleared after validation

---

## Part B — Heartbeat GNSS Fields (TELEM-01)

Subscribe to `gnss/FFFEB5/heartbeat` and verify the JSON includes:

```json
{
  "fix_type": 4,
  "satellites": 12,
  "hdop": 0.8
}
```

- `fix_type`: 0=No fix, 1=GPS, 2=DGPS, 4=RTK Fixed, 5=RTK Float, 6=Dead reckoning
- When no GGA received yet: all three fields should be `null`

### Checklist
- [ ] Heartbeat published after OTA reboot
- [ ] `fix_type`, `satellites`, `hdop` fields present in heartbeat JSON
- [ ] Fields show `null` before GNSS lock (if testable)
- [ ] `fix_type` shows correct value (4 = RTK Fixed when correction active)

---

## Part C — SoftAP Captive Portal (deferred from Phase 17)

Trigger SoftAP mode:
- Option A: Hold GPIO9 low for 3 seconds
- Option B: Publish `"softap"` to `gnss/FFFEB5/ota/trigger`

On a mobile device (iOS or Android):
1. Connect to `GNSS-Setup` WiFi (open, no password)
2. Verify captive portal notification appears OR web browser auto-opens
3. Confirm provisioning form renders correctly
4. Allow 300-second no-client timeout to restore normal operation (or reboot)

### Checklist
- [ ] Mobile device showed captive portal notification on connect
- [ ] Provisioning form rendered correctly in mobile browser
- [ ] Device returned to normal operation after timeout/reboot

---

## Decision: Canary Version Line

After OTA validation, decide:
- **Keep**: Leave `esp32-gnssmqtt v2.0-ota-canary` marker as permanent version string
- **Revert**: Remove canary suffix from `src/main.rs` version line before milestone tag

---

## Sign-Off

All items above must be checked before tagging v2.0.
