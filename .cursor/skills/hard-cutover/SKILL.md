---
name: hard-cutover
description: >-
  Performs bold delete-and-replace refactors with no migration or legacy shims.
  Use when renaming APIs, collapsing duplicate paths, removing old modules, or when
  the user asks for a hard cutover / no backwards compatibility in Skippr ReAct.
---

# Hard cutover (Skippr ReAct)

Skippr ReAct prefers one clean design over compatibility layers. Converging on that design is the priority — including amending locked decisions when they keep a fork alive.

## Rules

1. **One way** — if old and new both exist, delete the old in the same change.
2. **No shims** — MUST NOT add `legacy_`, `v1`/`v2` dual handlers, feature flags for “use old path,” or dual-write “for migration.”
3. **Update the graph** — callers, tests, and `specs/` / `docs/` move together.
4. **Rename in place** — prefer rename + fix compile errors over adapters that translate forever.
5. **Specs** — if behavior or names change, edit `specs/`. Amend `decisions.md` when locking a new choice **or** when existing lock text describes a second design. “Locked” is the surviving design, not a freeze on removing forks.

## Workflow

1. Identify the single target shape (types, module, API).
2. Move/rename implementation to that shape.
3. Fix all compile breaks (`cargo check -p …`).
4. Delete obsolete files and dead `cfg` / comments that describe the old world.
5. Align tests (including sim/property) so they encode the surviving path only.

## Anti-patterns

- “Keep the old function for now”
- `#[deprecated]` without removal in the same PR
- Parallel crates that do the same job
- Runtime toggles between implementations
- Leave `decisions.md` naming the discarded path so the lock “still matches”
- A test-only second kernel next to production
