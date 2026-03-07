# Pitfalls Research

**Domain:** Adding RTCM binary relay + OTA firmware update to existing ESP32-C6 Rust firmware (esp-idf-svc/hal/sys, UM980 GNSS)
**Researched:** 2026-03-07
**Confidence:** HIGH for OTA/partition pitfalls (verified against ESP-IDF v5.3.3 source in .embuild/); MEDIUM for RTCM parsing pitfalls (verified against RTCM3 spec via web; no production Rust embedded RTCM parser reference implementation found); MEDIUM for UM980 baud rate sequencing (verified command manual exists; PDF not machine-readable in this session)

---

## Critical Pitfalls

### Pitfall 1: Existing Partition Table Has Zero Space for OTA

**What goes wrong:**
The current `partitions.csv` allocates the entire 4MB flash to a single factory partition (0x3E0000 = ~3.9MB), leaving only 24KB of unused flash. Adding OTA requires two app partitions plus an `otadata` partition. There is no room — attempting to add OTA partitions to the existing layout produces an overlap or exceeds flash size.

**Why it happens:**
The factory layout was designed for single-image deployment without OTA. The factory partition consumes all available space after NVS and PHY. This is the correct approach for single-partition firmware but creates a hard blocker for OTA.

**How to avoid:**
The entire partition table must be redesigned before writing any OTA code. The new layout must:
- Remove the factory partition (or shrink it — but without factory, the bootloader boots ota_0 or ota_1 on first flash, which is acceptable)
- Add `otadata` (data, ota) of exactly 0x2000 bytes (required by ESP-IDF; two flash sectors for power-loss protection)
- Allocate two OTA app partitions (`ota_0` and `ota_1`) of equal size
- For 4MB flash: overhead is boot(0x8000) + nvs(0x10000) + otadata(0x2000) + phy(0x1000) = 0x1B000 (~112KB), leaving 3984KB for two app partitions = ~1992KB each (align to 4KB: 0x1F2000)

Reference layout for this project's 4MB flash:
```
nvs,      data, nvs,     0x9000,  0x10000,
otadata,  data, ota,     0x19000, 0x2000,
phy_init, data, phy,     0x1B000, 0x1000,
ota_0,    app,  ota_0,   0x1C000, 0x1F2000,
ota_1,    app,  ota_1,   0x20E000, 0x1F2000,
```
Verify total (0x9000 + 0x10000 + 0x2000 + 0x1000 + 0x1F2000 + 0x1F2000) does not exceed 0x400000 (4MB).

**Warning signs:**
- `partitions.csv` contains a `factory` app partition — must be removed or shrunk
- `espflash` reports partition table overlap on flash
- `esp_ota_begin()` returns `ESP_ERR_OTA_PARTITION_CONFLICT`

**Phase to address:** Partition table redesign must be Phase 1 of the RTCM+OTA milestone, before any OTA or RTCM code is written. The new partition table requires a full flash erase and re-flash (`espflash erase-flash`).

---

### Pitfall 2: OTA Write Is NOT Atomic — Interrupted Download Leaves Corrupt Partition

**What goes wrong:**
`esp_ota_write()` writes firmware chunks directly to the inactive OTA partition as they arrive. If the HTTP download is interrupted mid-way (WiFi drops, server timeout, power loss), the partition contains a partial image. The partition is NOT marked valid until `esp_ota_end()` + `esp_ota_set_boot_partition()` complete. However, the partial write is not cleaned up automatically — the partition contains garbage data that may confuse the bootloader on the next OTA attempt.

**Why it happens:**
ESP-IDF's OTA write path (confirmed in `esp_ota_ops.c`) erases flash sectors just before writing each chunk (sequential erase mode) or pre-erases the whole partition at `esp_ota_begin()`. If the download aborts, subsequent calls to `esp_ota_begin()` on the same partition will erase and restart correctly — BUT only if `esp_ota_begin()` is called cleanly. If the process panics or reboots mid-download, the `ota_ops_entry_t` state is lost (it is heap-allocated, not persisted to flash). The `otadata` partition is NOT updated until `esp_ota_set_boot_partition()` is called, so the bootloader will not attempt to boot the corrupt partition — but the next OTA attempt must call `esp_ota_begin()` to erase and restart.

**How to avoid:**
- Always wrap the entire OTA sequence in a single function that calls `esp_ota_begin()`, streams all chunks, calls `esp_ota_end()`, then calls `esp_ota_set_boot_partition()`. Never partially complete this sequence.
- On failure, call `esp_ota_abort()` to release the handle (confirmed in `esp_ota_ops.h`).
- The NEXT OTA attempt will call `esp_ota_begin()` which re-erases the partition — no manual cleanup needed.
- Do NOT call `esp_ota_set_boot_partition()` until `esp_ota_end()` returns `ESP_OK`. This is the only point where the partition is committed as bootable.
- SHA256 verification: `esp_ota_end()` calls `esp_image_verify()` internally (confirmed in `esp_ota_ops.c` line 405). This verifies the ESP image header magic, segment checksums, and — if secure boot is enabled — signature. It does NOT verify a user-supplied SHA256 of the raw binary. For SHA256 integrity of the download, implement streaming SHA256 during the HTTP download and compare against a known-good hash before calling `esp_ota_end()`.

**Warning signs:**
- OTA task panics or reboots mid-download; next OTA attempt fails with `ESP_ERR_OTA_VALIDATE_FAILED` (stale partial data)
- Download completes but `esp_ota_end()` returns `ESP_ERR_OTA_VALIDATE_FAILED` (corrupted HTTP body, not partial write)
- Boot partition is set before `esp_ota_end()` returns OK (logic error in OTA state machine)

**Phase to address:** OTA implementation phase — the OTA state machine must enforce the correct call sequence. Treat any error from `esp_ota_write()` or `esp_ota_end()` as fatal to that OTA attempt; call `esp_ota_abort()` and do not call `esp_ota_set_boot_partition()`.

---

### Pitfall 3: Boot Loop Without Rollback — New Firmware Must Call `mark_app_valid` or Device Is Stuck

**What goes wrong:**
If `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` is set and the new firmware boots but never calls `esp_ota_mark_app_valid_cancel_rollback()` (or `EspOta::mark_running_slot_valid()` in esp-idf-svc), the bootloader sees the app as unconfirmed (`ESP_OTA_IMG_PENDING_VERIFY`). On the next reboot (watchdog reset, power cycle, panic), the bootloader rolls back to the previous firmware. If the previous firmware also has a problem, the device oscillates between two broken images — the "double boot loop" failure mode.

**Why it happens:**
Rollback is opt-in at the `sdkconfig` level (`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`). If rollback is enabled but the application does not explicitly call the mark-valid function, the ESP-IDF bootloader treats every reboot as a failed validation. Developers often enable rollback for safety but forget to add the mark-valid call, or add it too late in the startup sequence (after a component that could crash or hang).

**How to avoid:**
- Call `esp_ota_mark_app_valid_cancel_rollback()` (or the esp-idf-svc wrapper) as early as possible in `main()` — before MQTT connect, before WiFi connect, ideally right after basic hardware init succeeds.
- "Valid" means "the firmware started and basic hardware works," not "the firmware successfully connected to the broker." Network connectivity issues should not trigger a rollback.
- If rollback is NOT enabled (`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` not set): OTA writes are immediately permanent — there is no automatic safety net. Verify the new image before calling `esp_ota_set_boot_partition()`.
- A safe strategy for this project: enable rollback, call mark-valid after UART init succeeds (GNSS pipeline working), before network connection attempts.

**Warning signs:**
- Device reboots once after OTA then reverts to old firmware — rollback triggered because mark-valid was never called
- `esp_ota_get_running_partition()` returns the old partition after a reboot following OTA (bootloader rolled back)
- Log shows `I (xxx) boot: Falling back to previous version` on boot

**Phase to address:** OTA implementation phase — add the mark-valid call in the initial implementation. Never add it as a follow-up task.

---

### Pitfall 4: Task Watchdog Firing During OTA Flash Erase

**What goes wrong:**
Erasing a 1.9MB OTA partition at `esp_ota_begin()` takes approximately 4-8 seconds (SPI flash erase is slow: ~50ms per 4KB sector, ~200 sectors). The default ESP-IDF task watchdog timeout is 5 seconds (`CONFIG_ESP_TASK_WDT_TIMEOUT_S=5`). The OTA task erasing a large partition can miss the watchdog feed, triggering a reboot mid-erase.

**Why it happens:**
`esp_ota_begin()` called with `OTA_SIZE_UNKNOWN` pre-erases the full partition. Flash erase is blocking in the SPI flash driver. The FreeRTOS task watchdog (`TWDT`) monitors tasks subscribed to it; if the OTA task is subscribed (or if the IDLE task is subscribed and starved), the TWDT fires.

**How to avoid:**
- Use sequential erase mode: pass `OTA_WITH_SEQUENTIAL_WRITES` as the image_size to `esp_ota_begin()`. This erases sectors just before writing them, spreading the erase time across the download (each sector erase is ~50ms, interleaved with HTTP reads).
- Alternatively, increase `CONFIG_ESP_TASK_WDT_TIMEOUT_S` to 30 seconds in `sdkconfig.defaults` for OTA builds.
- In esp-idf-svc Rust: the `EspOta::begin()` method maps to `esp_ota_begin()`. Pass the firmware size (if known from HTTP Content-Length header) to enable bulk-erase-then-write mode; pass `OTA_WITH_SEQUENTIAL_WRITES` (the value -1, or use the constant) if size is unknown.
- Feed the TWDT explicitly during long erase operations: `esp_task_wdt_reset()` from the OTA task.

**Warning signs:**
- Device reboots during OTA with `Task watchdog got triggered` in log
- OTA works on small test images (<500KB) but fails on production firmware (1.5MB+)
- Reboot occurs at consistent byte offsets corresponding to sector boundaries

**Phase to address:** OTA implementation phase — use sequential erase mode from the start. Do not assume the default watchdog timeout is sufficient.

---

### Pitfall 5: RTCM Frame Sync Loss — Buffer Must Drain to Next 0xD3 Preamble

**What goes wrong:**
The current `gnss.rs` RX loop accumulates bytes into a 512-byte line buffer, treating `\n` as the frame delimiter. RTCM3 frames do not contain `\n` — they are binary, and the payload can contain any byte value including `\n` (0x0A). When the GNSS module outputs RTCM frames, the line-based parser treats the binary payload as a "non-NMEA line" and discards it. Worse, if a byte that happens to be `\n` occurs mid-RTCM-frame, the parser splits the frame at that byte, logs a "non-NMEA line dropped" warning, and tries to interpret the remainder as a new line — permanently losing sync on subsequent bytes until the next valid NMEA sentence resets the buffer.

**Why it happens:**
NMEA and RTCM3 share the same UART stream. NMEA is line-oriented (ASCII, `\r\n` terminated). RTCM3 is binary, length-prefixed (3-byte header: 0xD3 preamble + 6 reserved bits + 10-bit length; then payload; then 3-byte CRC24Q). The current parser has no awareness of binary frames. When RTCM is enabled on the UM980, the current parser will misinterpret every RTCM frame as a sequence of garbage NMEA lines.

**How to avoid:**
The RX loop must become a dual-mode parser:
1. If accumulator is empty and next byte is `$` (0x24): enter NMEA mode, accumulate until `\n`
2. If accumulator is empty and next byte is `0xD3`: enter RTCM mode, read 2 more bytes to determine payload length (10-bit field in bytes 1-2: `len = ((byte1 & 0x03) << 8) | byte2`), then accumulate exactly `len + 6` bytes total (3 header + len payload + 3 CRC), then validate CRC24Q
3. If CRC fails: discard the frame and scan forward for next `0xD3` or `$` byte — do NOT assume the byte after the bad frame is a valid start

The resync strategy after CRC failure: scan byte-by-byte for the next `0xD3` or `$`. This is the approach used by RTKLIB (`rtcm.c`) and Python implementations (pyrtcm). Do NOT try to re-parse from byte 1 of the failed frame (the preamble byte itself might be a false positive from payload data).

**Warning signs:**
- Logs flood with "non-NMEA line dropped" warnings after RTCM is enabled on the UM980
- NMEA sentences stop arriving after RTCM is enabled (parser stuck on binary data)
- MQTT GNSS topics go silent when RTCM output rate is high (> NMEA output rate)

**Phase to address:** RTCM parser phase — the RX loop in `gnss.rs` must be refactored to a state-machine parser before enabling RTCM output on the UM980.

---

### Pitfall 6: RTCM Buffer Too Small for MSM Messages

**What goes wrong:**
The current RX thread uses a 512-byte `line_buf` stack array. RTCM3 MSM (Multiple Signal Message) frames — types 1071-1127, used for RTK corrections from multi-constellation receivers like the UM980 — can be up to 1023 bytes of payload plus 6 bytes of overhead = 1029 bytes total per frame. A 512-byte buffer silently truncates any MSM frame larger than 512 bytes. The truncated frame will fail CRC and be discarded. The RTCM relay delivers no data to downstream RTK clients.

**Why it happens:**
The RTCM3 spec allows payload up to 1023 bytes (10-bit length field, max value 1023). MSM7 messages (highest precision MSM, used by UM980 in RTK mode) for multi-constellation configurations (GPS+GLONASS+Galileo+BeiDou) routinely exceed 500 bytes and can approach the 1023-byte limit. The 512-byte line buffer was sized for NMEA (longest UM980 proprietary NMEA sentences are ~200 bytes).

**How to avoid:**
- Size the RTCM frame buffer at 1029 bytes minimum (3 header + 1023 payload + 3 CRC)
- Use 1100 bytes to allow some margin
- This buffer only needs to exist during active RTCM frame accumulation — it does not need to be permanently allocated. Options:
  - Stack allocation in the RX thread: `[u8; 1100]` on the existing 8KB thread stack — acceptable (1.1KB of 8KB stack)
  - Separate fixed-size `[u8; 1100]` alongside the NMEA `[u8; 512]` buffer — clearest intent
  - Do NOT use `Vec<u8>` for this buffer unless heap fragmentation is acceptable (embedded heap is shared with WiFi and MQTT stacks)

**Warning signs:**
- RTCM frames arrive from UM980 (confirmed via UM980 monitor output) but downstream RTK client receives no corrections
- CRC failures logged on every RTCM frame when MSM7 is enabled
- Frame length field in header bytes shows values > 512 (check with debug logging before CRC check)

**Phase to address:** RTCM parser phase — size buffers correctly from the start. Do not attempt to tune buffer sizes after initial implementation.

---

### Pitfall 7: OTA Download Must Be Independent of MQTT Connection

**What goes wrong:**
If the OTA firmware download is implemented inside the MQTT event handler or a task that holds the MQTT client lock, an MQTT disconnect mid-download will abort the OTA. More critically, if the MQTT keep-alive timeout fires during a long flash erase or HTTP read, the MQTT connection drops, which may interrupt the OTA state machine if OTA cleanup is triggered by MQTT disconnect.

**Why it happens:**
A 1.9MB firmware download at typical HTTP speeds (100KB/s over WiFi) takes ~20 seconds. MQTT keep-alive is typically 60-120 seconds — sufficient. But if the HTTP server is slow or the WiFi link degrades, the download can take longer. If MQTT keep-alive fires and the firmware does not respond (OTA task is blocking on HTTP read), the broker closes the connection.

**How to avoid:**
- Run the HTTP download in a dedicated OTA task, separate from the MQTT pump task. The OTA task communicates with MQTT only to receive the trigger URL and to publish completion status.
- The OTA trigger arrives via MQTT (e.g., topic `gnss/{device_id}/ota` with payload containing the firmware URL). The MQTT handler extracts the URL, sends it to the OTA task via a channel, and returns immediately. The OTA task handles the download independently.
- Keep the MQTT client alive during OTA by ensuring the MQTT pump task continues running. The MQTT task must NOT be blocked waiting for OTA to complete.
- After OTA succeeds (before reboot), publish a status message on `gnss/{device_id}/ota/status` with "success" payload. After OTA fails, publish "failed". Then reboot (for success) or resume normal operation (for failure).
- GNSS relay should pause during OTA to free memory (RTCM frames + HTTP buffer + OTA write buffer can exceed available heap). Use a shared `AtomicBool` flag `ota_in_progress`.

**Warning signs:**
- MQTT drops mid-OTA and the OTA attempt silently fails with no log
- Heap exhaustion during OTA (GNSS relay + HTTP + OTA buffers overlap)
- OTA task blocks the MQTT pump, causing broker keep-alive timeout

**Phase to address:** OTA implementation phase — the task architecture (OTA task independent of MQTT pump) must be designed before implementation.

---

### Pitfall 8: UM980 Baud Rate Change — Wrong Order Causes UART Desync

**What goes wrong:**
If the baud rate change command is sent to the UM980, and then the ESP32 UART baud rate is changed immediately, the UM980 may not have processed and applied the command before the ESP32 switches baud rates. The result is that the UM980 responds at the new baud rate while the ESP32 is still at the old rate (or vice versa), producing garbled communication that appears as a stream of wrong bytes. Recovery requires power cycling the UM980 or sending the baud-reset command at the correct rate.

**Why it happens:**
The UM980 command processing pipeline is not instantaneous. After receiving `COM COM1 921600` (or similar), the UM980 switches its UART hardware shortly after sending the `OK` acknowledgment at the old baud rate. If the ESP32 changes its UART driver baud rate before the UM980 sends the `OK` (or without waiting for `OK`), the sequencing is wrong.

**How to avoid:**
- Send the baud rate change command at the current baud rate
- Wait for the `OK\r\n` response at the current baud rate
- THEN reconfigure the ESP32 UART driver to the new baud rate
- THEN verify communication at the new baud rate by sending a known query (e.g., `VERSION`) and checking for a valid response
- If no valid response at the new rate within 500ms, reconfigure back to the old rate and retry
- The UM980 command is `COM COM1 <baudrate>` (the port name must match the physical UART; COM1 is the default UART on most UM980 boards)
- Persistence: per existing project decision, do NOT call `CONFIGSAVE` — the baud rate change will be lost on UM980 power cycle. This is acceptable if the baud rate change is always re-applied at boot via the MQTT retained config mechanism.

**Warning signs:**
- All UART reads return 0xFF or random bytes after baud rate change attempt
- UM980 stops outputting NMEA after baud rate command (ESP32 is reading at wrong rate)
- ESP32 UART `read()` returns non-zero data but `from_utf8()` fails on every read (baud mismatch)

**Phase to address:** Baud rate change is an advanced configuration feature; if pursued, implement as an explicit test plan with hardware verification on device FFFEB5 before shipping.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Skip `otadata` partition in partitions.csv | Simpler layout | OTA bootloader cannot function; device silently boots factory always | Never — `otadata` is mandatory for OTA |
| Use `OTA_SIZE_UNKNOWN` with bulk erase | Simpler `esp_ota_begin()` call | Watchdog fires on large partitions; 4-8s blocking erase | Never for production; OK for test with short WDT increase |
| Use `Vec<u8>` for RTCM frame buffer | Dynamic sizing | Heap fragmentation with WiFi+MQTT+OTA competing; potential allocation failure mid-download | Acceptable in test; replace with fixed-size `[u8; 1100]` before shipping |
| Call `esp_ota_set_boot_partition()` before `esp_ota_end()` | Shorter code path | Boot loop on corrupt image — OTA writes an unvalidated partition as boot target | Never |
| Implement OTA inside MQTT handler | Less code | MQTT blocks during download; 20s download blocks keep-alive | Never |
| Omit mark-valid call when rollback is enabled | Less code | Every reboot after OTA triggers rollback | Never |
| Keep factory partition and shrink to 1MB | Preserves recovery path | Only ~0.95MB each for ota_0 and ota_1 — barely fits current 1MB firmware; no growth headroom | Only if current binary is confirmed <900KB and will not grow |
| Reuse NMEA line_buf for RTCM accumulation | Reuses existing buffer | 512 bytes too small for MSM7 frames; silent data loss | Never |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| OTA partition table | Adding `otadata` with wrong size (e.g., 0x1000) | `otadata` must be exactly 0x2000 (two flash sectors); ESP-IDF bootloader requires both sectors for power-loss safety |
| OTA binary format | Downloading raw ELF or raw binary | Must download ESP app image format (`espflash save-image` output); `esp_ota_end()` validates magic byte 0xE9 at offset 0 |
| HTTP OTA + espflash flash | Flashing with `espflash flash` after OTA changes boot partition | `espflash flash` writes the app at 0x10000 (default) — this overwrites `ota_0` only; `otadata` still points to `ota_1`; device boots wrong partition. Always use `espflash flash --partition-table partitions.csv` |
| RTCM parser CRC | Using CRC16 or CRC32 for RTCM frame check | RTCM3 uses CRC24Q (a specific 24-bit polynomial); using the wrong CRC algorithm produces false pass/fail on every frame |
| RTCM + NMEA same channel | Forwarding both to same MQTT topic | RTCM is binary; MQTT payload can carry binary but subscribers must handle binary. Use a separate MQTT topic (`gnss/{id}/rtcm`) and publish as raw bytes, not as UTF-8 string |
| OTA trigger via MQTT | Using QoS 0 for OTA trigger message | Use QoS 1 for OTA trigger; QoS 0 can be dropped on broker restart. OTA trigger must be reliable |
| OTA trigger with retain | Retained OTA trigger re-triggers on every reconnect | After OTA completes successfully, publish an empty retained message to `gnss/{id}/ota` to clear the trigger |
| UM980 RTCM output + NMEA output | Enabling high-rate RTCM at 115200 baud without checking bandwidth | At 115200 baud (~11,500 bytes/sec), a 1029-byte MSM7 frame takes ~90ms. If RTCM rate is 1Hz and NMEA rate is 10Hz for 5 sentence types (~100 bytes each = 500 bytes/s), total is ~1530 bytes/s — well within 115200 baud capacity. Problems arise only above ~5Hz RTCM |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| RTCM frame copy to heap on every forward | Heap fragmentation, allocation latency spike | Use fixed-size stack buffer for RTCM accumulation; only allocate when forwarding via MQTT | After 10K+ RTCM frames if heap is fragmented |
| HTTP OTA chunk buffer too large | Heap exhaustion during OTA (WiFi + MQTT + HTTP + OTA buffers) | Use 4KB-16KB HTTP read chunks; verify total heap usage with `esp_get_free_heap_size()` before starting OTA | Immediately if total buffers exceed available heap (~200KB on ESP32-C6) |
| GNSS RX thread running during OTA | Competes for heap and CPU with OTA HTTP download | Pause GNSS relay thread during OTA using `AtomicBool ota_in_progress`; resume after reboot | If RTCM forwarding is active and consuming large buffers simultaneously with OTA |
| SHA256 of entire downloaded image in memory | Cannot fit 1.9MB in RAM | Stream SHA256: use `sha2` crate with `update()` per chunk during download, `finalize()` at end | Immediately — ESP32-C6 has ~400KB of usable heap |
| String formatting for RTCM MQTT topic on every frame | Stack allocation per publish | Pre-format RTCM topic string at startup, store as `String` or `Arc<str>` | Not critical at 1Hz RTCM; noticeable at 10Hz+ |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| OTA firmware URL from MQTT without authentication | Attacker publishes malicious URL to OTA topic; device downloads arbitrary firmware | No TLS in v1 (per project constraints) — document risk; in v2 add TLS for MQTT and HTTP. Mitigation in v1: verify SHA256 of downloaded image against a hash published alongside the URL in the MQTT message |
| OTA with no firmware version check | Downgrade attack: attacker triggers OTA to older vulnerable firmware | Check `esp_app_get_description()->version` of new image before calling `esp_ota_set_boot_partition()`; reject if version is older than current |
| RTCM binary forwarded without size validation | Malformed RTCM from UM980 (hardware glitch) could be forwarded as oversized MQTT payload | Validate frame length field before accumulating: if `len > 1023`, discard and resync |
| No rate limiting on OTA trigger | Repeated OTA triggers cause repeated download+flash cycles, wearing flash | Track last OTA timestamp; reject triggers within 60 seconds of a previous attempt |

---

## "Looks Done But Isn't" Checklist

- [ ] **Partition table:** Contains `otadata` of type `data, ota` with size exactly 0x2000 — verify with `espflash partition-table` command
- [ ] **Partition table:** `ota_0` and `ota_1` both present and sized to fit current firmware with growth headroom — verify total does not exceed 0x400000
- [ ] **OTA state machine:** Calls `esp_ota_abort()` (or equivalent) on ANY error before `esp_ota_end()` — verify error paths in code review
- [ ] **OTA rollback:** Calls `mark_running_slot_valid()` (or `esp_ota_mark_app_valid_cancel_rollback()`) early in `main()` — verify it is called before any blocking network operation
- [ ] **RTCM parser:** Handles `\n` bytes inside binary RTCM payload correctly (does NOT treat them as line terminators) — verify with synthetic test frame containing 0x0A in payload
- [ ] **RTCM buffer:** Frame accumulation buffer is >= 1029 bytes — verify constant definition before implementing
- [ ] **RTCM CRC:** Uses CRC24Q algorithm (specific 24-bit polynomial 0x1864CFB), not CRC16/CRC32 — verify against reference implementation
- [ ] **OTA MQTT topic:** Cleared (empty retained message) after successful OTA to prevent re-trigger on reconnect — verify in OTA completion handler
- [ ] **OTA binary format:** Build pipeline produces ESP app image (`.bin`), not ELF — verify first byte of download is 0xE9 before calling `esp_ota_end()`
- [ ] **Watchdog:** OTA task feeds watchdog during flash erase OR sequential erase mode is used — verify with 1.9MB test image

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Wrong partition table (no OTA partitions) | HIGH | Full flash erase (`espflash erase-flash`), reflash with new partition table and firmware; device offline during recovery |
| Boot loop after OTA (mark-valid not called) | MEDIUM | Add mark-valid call, rebuild, reflash via USB; if rollback is enabled and old firmware is valid, device auto-recovers |
| Corrupt OTA partition after interrupted download | LOW | Next `esp_ota_begin()` call re-erases the partition automatically; no manual intervention needed |
| RTCM parser stuck (sync loss) | LOW | Parser resync by scanning for next 0xD3 or `$` byte; implement as part of initial parser |
| UART desync after failed baud rate change | LOW | Power cycle UM980; UM980 returns to last saved baud rate (115200 if CONFIGSAVE was not called) |
| Heap exhaustion during OTA | MEDIUM | Pause GNSS relay before OTA, reduce HTTP chunk size, add heap check before starting download |
| Watchdog during OTA flash erase | LOW | Switch to sequential erase mode (`esp_ota_begin()` with `OTA_WITH_SEQUENTIAL_WRITES`) |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Partition table has no OTA space | Phase 1: Partition table redesign | `espflash partition-table` shows ota_0, ota_1, otadata; total fits 4MB |
| Interrupted OTA leaves corrupt partition | Phase 2: OTA implementation | Simulate WiFi drop mid-download; verify device boots old firmware and accepts new OTA trigger |
| Boot loop without mark-valid | Phase 2: OTA implementation | Flash OTA update, hard reboot mid-boot (power cycle); verify device boots correct partition after rollback |
| Watchdog during flash erase | Phase 2: OTA implementation | Download 1.9MB test image; verify no watchdog reset in logs |
| RTCM binary treated as NMEA lines | Phase 3: RTCM parser | Enable RTCM output on UM980; verify zero "non-NMEA line dropped" warnings |
| RTCM buffer too small for MSM7 | Phase 3: RTCM parser | Enable MSM7 on UM980; verify CRC pass rate is >99% over 100 frames |
| OTA and MQTT on same task | Phase 2: OTA task architecture | During OTA download, verify MQTT heartbeat continues publishing at expected interval |
| UM980 baud rate change desync | Phase N: Baud rate change (future) | Send baud change, wait for OK at old rate, switch ESP32 rate, verify VERSION response at new rate |
| RTCM CRC algorithm wrong | Phase 3: RTCM parser | Compare CRC output against known-good RTCM frame (use pyrtcm or RTKLIB reference) |
| OTA trigger not cleared after success | Phase 2: OTA implementation | Trigger OTA; after success and reboot, verify device does not re-trigger OTA on MQTT reconnect |

---

## Sources

- ESP-IDF v5.3.3 source: `.embuild/espressif/esp-idf/v5.3.3/components/app_update/esp_ota_ops.c` — HIGH confidence (direct source inspection); confirms `esp_ota_end()` calls `esp_image_verify()`, sequential erase behavior, and handle lifecycle
- ESP-IDF v5.3.3 header: `.embuild/espressif/esp-idf/v5.3.3/components/app_update/include/esp_ota_ops.h` — HIGH confidence; confirms `esp_ota_mark_app_valid_cancel_rollback()`, `esp_ota_abort()`, rollback state machine
- ESP-IDF OTA documentation: https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/ota.html — HIGH confidence; confirms otadata 0x2000 requirement, rollback behavior, watchdog concerns
- esp-rs esp-idf-svc OTA API: https://docs.esp-rs.org/esp-idf-svc/esp_idf_svc/ota/struct.EspOta.html — HIGH confidence (verified via web search results); confirms `mark_running_slot_valid()` wraps `esp_ota_mark_app_valid_cancel_rollback()`
- esp-ota crate (faern): https://github.com/faern/esp-ota — MEDIUM confidence; confirms binary must be ESP app image format, `set_as_boot_partition()` + `mark_app_valid()` pattern
- RTCM3 frame structure: https://docs.emlid.com/reachrs3/specifications/rtcm3-format/ and swiftnav docs — HIGH confidence; frame is 3B header + 0-1023B payload + 3B CRC24Q = max 1029 bytes
- pyrtcm library (resync strategy): https://github.com/semuconsulting/pyrtcm — MEDIUM confidence; confirms scan-for-preamble resync after CRC failure
- RTKLIB rtcm.c (resync strategy): https://github.com/tomojitakasu/RTKLIB — MEDIUM confidence; canonical RTCM3 parser reference
- Partition layout arithmetic: verified locally — current factory partition leaves 24KB free; dual OTA without factory gives 1992KB per slot on 4MB flash
- Unicore UM980 baud rate command: search results confirm `CONFIG COM1 <rate>` syntax; CONFIGSAVE persistence requirement — MEDIUM confidence (PDF not machine-readable in this session; verify against official Unicore Reference Commands Manual)
- Existing project context: `src/gnss.rs` (512-byte line_buf, `\n`-delimited parser), `partitions.csv` (single factory, no OTA), `src/config.rs` (4096-byte UART RX buffer) — HIGH confidence (direct file inspection)

---
*Pitfalls research for: RTCM binary relay + OTA firmware update on ESP32-C6 Rust firmware*
*Researched: 2026-03-07*
