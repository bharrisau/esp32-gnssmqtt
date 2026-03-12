# gnss-softap — Blocker Documentation

## Background

The SoftAP portal (`firmware/src/provisioning.rs`) starts a WiFi access point, serves an
HTML configuration form over HTTP, and collects WiFi + MQTT + NTRIP credentials from the
user. The ESP-IDF implementation uses `EspHttpServer` + `BlockingWifi` from `esp-idf-svc`,
both of which require `std`.

This document records what currently prevents a complete no_std implementation.

---

## Blocker 1 — SoftAP WPA2 Password (RESOLVED)

**Issue:** Previously, `esp-wifi` (now `esp-radio`) did not support WPA2-PSK for SoftAP
mode. The access point could only be opened (no password), which is a security concern for
production devices.

**Resolution:** `esp-radio 0.16.x` added WPA2-Personal support for SoftAP via:

```rust
AccessPointConfig::default()
    .with_password("my-password")
    .with_auth_method(AuthMethod::WPA2Personal)
```

**Status as of 2026-03-12:** RESOLVED. This blocker is no longer an obstacle for a no_std
SoftAP implementation.

**Impact:** None — this gap has closed.

---

## Blocker 2 — No no_std HTTP Server with Multi-Field Form POST Parsing (ACTIVE)

**Issue:** The provisioning portal collects 8+ fields from a single HTML form submitted
as `application/x-www-form-urlencoded` POST body. A no_std HTTP server must be able to:

1. Accept an incoming TCP connection
2. Parse the HTTP request line and headers
3. Read and URL-decode a multi-field POST body (e.g. `ssid=MyNet&pass=abc&mqtt_host=...`)

**Candidate libraries:**

- **picoserve** — The primary no_std/async HTTP server for embassy. However, multi-field
  form POST body parsing (needed to collect 8+ provisioning fields) is not confirmed as
  production-ready in picoserve as of 2026-03-12. Check picoserve GitHub issues for form
  parsing support status.

- **nanofish** — A smaller alternative (client + server) but its form parsing status is
  also unconfirmed.

**Tracking:** Check picoserve GitHub for form body parsing issues/PRs.

**Status as of 2026-03-12:** ACTIVE. Neither picoserve nor nanofish has confirmed
production-ready multi-field form POST parsing.

**Impact:** The full SoftApPortal trait cannot be implemented in a no_std context until
a no_std HTTP server with form parsing is available or a manual fallback is written.

---

## Recommended Path Forward

1. Evaluate picoserve's current form parsing capabilities — check GitHub issues and examples
   for `application/x-www-form-urlencoded` POST body handling.

2. If picoserve form parsing is adequate: implement `SoftApPortal` using picoserve +
   esp-radio SoftAP in a `gnss-softap-embassy` backend crate.

3. If picoserve form parsing is inadequate: implement a minimal URL-encoded form body parser
   inline (~50 lines: split on `&`, split each pair on `=`, percent-decode). This is a
   feasible fallback that removes the library dependency entirely.

4. DNS hijacking for the captive portal is handled by the `gnss-dns` crate (see its
   `BLOCKER.md` — classified SOLVABLE).
