# Headless Execution

Headless execution is exposed through binaries and daemons that call the shared `react` runtime library. The primary example is `skippr`, which assembles the data-engineer suite and invokes the generic headless runner internally.

Typical usage now looks like:

```bash
cargo run -p skippr -- --log info run
```

If you need custom suites, depend on `react` + `react-core`, implement `react::host::HostComposition`, and call the library entrypoints directly.
