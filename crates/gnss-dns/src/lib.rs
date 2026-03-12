#![no_std]

/// Captive portal DNS responder.
///
/// Replies to all DNS A queries with a fixed IP address (the portal IP),
/// causing clients to redirect to the captive portal page.
///
/// # Implementations
///
/// - ESP-IDF: `std::net::UdpSocket` bound to port 53 (in `provisioning.rs`);
///   receives raw DNS query bytes, extracts QNAME, responds with A record.
/// - nostd: SOLVABLE — see `BLOCKER.md` for implementation guidance.
pub trait CaptiveDnsResponder {
    /// Error type for DNS operations.
    type Error: core::fmt::Debug;

    /// Start listening on UDP port 53 and responding with `portal_ip` for all A queries.
    ///
    /// `portal_ip` is an IPv4 address in network byte order, e.g. `[192, 168, 4, 1]`.
    fn start(&mut self, portal_ip: [u8; 4]) -> Result<(), Self::Error>;

    /// Process one pending DNS query. Returns `Ok(true)` if a query was answered.
    ///
    /// Call in a loop while the captive portal is active. Non-blocking: returns
    /// `Ok(false)` immediately if no query is pending.
    fn poll(&mut self) -> Result<bool, Self::Error>;

    /// Stop listening and release the UDP socket.
    fn stop(&mut self) -> Result<(), Self::Error>;
}
