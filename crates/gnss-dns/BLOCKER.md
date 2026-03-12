# gnss-dns — Gap Documentation

## Background

The captive portal DNS responder (`firmware/src/provisioning.rs`) binds a
`std::net::UdpSocket` to port 53 during SoftAP mode. It intercepts DNS queries from
connecting clients and responds with the portal IP address (`192.168.4.1`), causing
all HTTP traffic to be redirected to the provisioning form.

The ESP-IDF implementation uses `std::net::UdpSocket` (std only).

---

## Gap — No Turnkey Captive-Portal DNS Crate (NOT a Blocker — SOLVABLE)

**Classification:** SOLVABLE. This is a maturity gap (no crate exists), not a
fundamental impossibility.

**Issue:** No turnkey captive-portal DNS crate exists for `embassy-net` as of
2026-03-12. However, the implementation is straightforward and does not require any
external library beyond `embassy-net` (which is already required for networking).

**Implementation outline (~50 lines):**

1. Bind an `embassy-net` UDP socket to port 53.
2. Receive a UDP datagram (raw DNS query bytes).
3. Parse the QNAME from DNS wire format:
   - Skip the 12-byte DNS header
   - Read length-prefixed labels until a zero-length label is encountered
4. Construct a minimal DNS A record response (~50 bytes):
   - Copy the transaction ID from the query
   - Set QR=1 (response), AA=1 (authoritative), RCODE=0 (no error)
   - Echo the question section
   - Add one answer: A record pointing to `portal_ip` with TTL=60
5. Send the response UDP datagram back to the query source.

**No parsing complexity:** Only DNS A queries need to be answered. Other query types
(AAAA, PTR, etc.) can be responded to with NXDOMAIN or ignored — client behaviour is
acceptable either way for captive portal detection.

**embassy-net UDP sockets:** Production-ready and used in many embassy projects. The
async UDP socket API is stable.

**Reference:** Phase 22 nostd-audit.md §9 classifies this DNS gap as SOLVABLE.

**Status as of 2026-03-12:** No blocking issue exists. Implementation effort is ~50
lines of custom DNS response construction.

---

## Recommended Path Forward

Implement `CaptiveDnsResponder` inline in a `gnss-dns-embassy` backend crate when the
SoftAP portal is ported to embassy. The implementation requires:

- `embassy-net` UDP socket (already available in the embassy networking stack)
- ~50 lines of DNS wire-format response construction (no external crate needed)
- No unsafe code required

This crate and trait exist as a documentation boundary — the gap is library availability,
not implementation feasibility.
