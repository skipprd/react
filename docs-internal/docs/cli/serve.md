# WebSocket Serving

WebSocket serving runs through host binaries and daemons built on the shared `react` runtime:

- the shared `react` runtime library exposes reusable WebSocket server machinery
- product binaries or dedicated daemons define their own suites, config resolution, and provider wiring
- third-party hosts can depend on `react` + `react-core` and call the library entrypoints directly

For reference implementations, see:

- `goggles-reactd`
