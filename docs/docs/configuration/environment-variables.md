# Environment Variables

The generic `react` runtime uses host-provided configuration. Product hosts own domain-specific environment variables for connectors, warehouses, and billing.

Common runtime-level variables include:

| Variable | Description |
|---|---|
| `RUST_LOG` | Standard tracing filter, for example `react=debug,warn` |
| `REACT_HEADLESS` | Enables host-defined non-interactive prompt handling in headless runs |
| `REACT_PLAIN_PROGRESS` | Prints plain progress output for non-TTY environments |
| `REACT_LOG_DIR` | Enables rotating runtime logs in the provided directory |

Downstream hosts may pass additional suite or provider variables through their own configuration layer.
