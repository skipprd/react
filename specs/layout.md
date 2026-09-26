# Repository layout (Skippr ReAct)

Normative directory layout. Product hosts (`sde`, others) live in sibling repos.

```
react/
├── AGENTS.md                 # Agent contract (principles + checklist)
├── Cargo.toml                # Workspace root
├── rust-toolchain.toml
├── specs/                    # Normative MUST docs (not under docs/)
│   ├── decisions.md
│   ├── tiger-style.md
│   ├── style.md
│   ├── testing.md
│   ├── storage.md
│   └── layout.md
├── docs/                     # Public product docs (VitePress → react.skippr.io)
├── docs-internal/            # Maintainer notes / audits (not normative)
├── infra/
│   └── cloudflare/r2/        # Ops Terraform: ReAct R2 buckets (R7)
├── src/
│   ├── core/                 # react-core — kernel (pure given injected world)
│   ├── runtime/              # react — host/bootstrap/headless/server helpers
│   ├── transport/            # react-transport — WS/protocol
│   ├── http-protocol/
│   ├── view/                 # react-view
│   ├── suites/               # in-repo example suites (kb, suite_debugger, …) — smoke here; DE is in sde
│   └── modules/              # storage / vector adapters (real I/O behind traits)
├── tests/                    # workspace functional + sim (see testing.md)
├── fuzz/                     # cargo-fuzz targets
├── scripts/
└── .cursor/                  # rules + skills
    ├── rules/
    └── skills/
```

## Ownership

| Crate | Layer | I/O |
|-------|-------|-----|
| `react-core` | Kernel | Injected only |
| `react` | Host | Real, through kernel traits |
| `react-transport` / `react-view` / `react-http-protocol` | Protocol / presentation | Parse at edge; kernel types inward |
| `react-module-*` | Adapters | Real storage/vector |
| `react-suite-*` (this repo) | Example suites (`kb`, …) | Depend only on generic React contracts. **Smoke this repo against these.** |
| `react-suite-data-engineer` | Product DE suite | Sibling `sde/crates/skipprd-react-suite-data-engineer`. **Larger tests run from `sde`.** |

**MUST NOT** put warehouse/dbt/Skippr-domain suites here. **MUST NOT** smoke ReAct by copying DE into this workspace. **MUST NOT** put normative MUST language only in `docs-internal/` — that belongs in `specs/`.
