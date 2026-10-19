# ADR 1508: Explicit, short-lived first-enrollment discovery

- Status: Accepted for controlled Alpha
- Date: 2026-10-01

Existing CPYD discovery advertises a paired control endpoint, not a PIN endpoint.
Do not reinterpret its port or flags. A separate enrollment hint announces only
while an explicitly opened welcome window waits for its single attempt.

Use IPv4 multicast 239.255.67.81:54786, TTL 1, one announcement per second, bound
to the selected local address. A user-initiated scan joins that group on the chosen
interface for four seconds. Retain at most 16 candidates and parse at most 256
datagrams per scan; each datagram is bounded to 512 bytes. Hints contain protocol
version, public node ID/name and pairing port, never a PIN or credential. The
candidate IP comes from the received datagram source. Ignore self advertisements.

Neither discovery nor selecting a hint authorizes a connection or changes trust.
Users still enter the PIN and both confirm the PAKE-authenticated identities.
The scanner is native-owned, asynchronously bounded, and does not alter ongoing
control discovery or hardware Routes. No new Android permission, multicast lock,
system network rule, cloud registry or dependency is added. Multicast may be
unavailable on a VPN, filtered Wi-Fi or particular Android device; expose empty
results/errors truthfully and preserve a manual endpoint fallback.

Public `capyio://pair` address text is an optional manual alternative. It carries
only a numeric endpoint, not the PIN or trust material; parsing it only fills the
form. It is not a new OS URL handler or an automatically followed deep link.
