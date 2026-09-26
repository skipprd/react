---
name: rust-react-crate
description: >-
  Implements or refactors Rust crates in the Skippr ReAct workspace with
  design-by-construction, kernel purity, DRY, and hard cutovers. Use when
  editing src/*, Cargo.toml, or writing Rust for react-core, runtime,
  transport, view, suites, or adapters.
---

# Rust work in Skippr ReAct

## Before coding

1. Identify the single target shape. Extend an existing pattern only when it **is** that shape; if it is a parallel path, delete it (skill: **hard-cutover**) rather than wrapping both.
2. Confirm behavior against `specs/` (tiger-style, style, testing, layout, decisions). If specs fork, amend them to one design in the same change.
3. Kernel vs host: `react-core` is pure given injected world. New I/O is a trait implementor in adapters/host, not ambient OS in the kernel (skill **`tiger-style`**).

## Implementation

- Make illegal states unrepresentable (enums, newtypes, typed errors).
- Prefer flat functions; extract traits only for multiple implementors.
- Hard cutover: delete old modules in the same change.
- Durable handlers: idempotency, commit boundaries, conditional writes.
- No impl env knobs; fixed safety ceilings in code; auto-tune working setpoints.
- No `unwrap`/`expect` on untrusted input (LLM JSON, WS frames, UTF-8 slices).

## Verify

```bash
cargo fmt -p <crate>
cargo check -p <crate>
cargo test -p <crate>
# when touching features:
cargo check -p <crate> --all-features
```

Add simulation / property / fuzz coverage per `specs/testing.md` (skill **`tdd-react`**).

Do not add `-D warnings` to clippy unless the user asks.
