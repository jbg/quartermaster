# Authenticated Mobile Pairing

Quartermaster setup QR codes transfer only the server URL, and invite links transfer only invite context. Authenticated session handoff is a separate account-access flow.

The implemented flow uses a one-time handoff token:

- The signed-in source device creates a short-lived handoff request tied to its user, household, session, creation time, and target device label.
- The source device shows a QR payload containing the server URL, handoff id, and token secret.
- The target device presents an explicit confirmation screen before accepting the handoff.
- The server consumes the token once and issues a normal access/refresh token pair for the target device.
- Expired, consumed, cancelled, wrong-token, or source-session-revoked requests fail closed and never fall back to URL-only pairing.

Do not extend URL-only server pairing into authentication transfer. Server pairing is configuration; authenticated handoff is account access.

The QR payload uses the custom app scheme:

```text
quartermaster://handoff?server=https%3A%2F%2Fquartermaster.example.com&id=<handoff-id>&token=<secret>
```

The `server` value is the target API origin the receiving device should use. The handoff id and token are not useful without the server-side row and expire quickly. Accepting a handoff preserves the source session's active household for the new target-device session.
