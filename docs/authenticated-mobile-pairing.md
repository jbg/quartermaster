# Authenticated Mobile Pairing

Quartermaster setup QR codes currently transfer only the server URL, and invite links transfer only invite context. Authenticated session handoff should stay a separate flow.

If this is implemented later, use a one-time pairing token:

- The signed-in source device creates a short-lived handoff request tied to its user, household, session, creation time, and target device label.
- The source device shows a QR code containing only the server URL plus the handoff token identifier.
- The target device presents an explicit confirmation screen before accepting the handoff.
- The server consumes the token once, writes an audit row, and issues a normal access/refresh token pair for the target device.
- Expired, consumed, or cancelled tokens fail closed and never fall back to URL-only pairing.

Do not extend URL-only server pairing into authentication transfer. Server pairing is configuration; authenticated handoff is account access.
