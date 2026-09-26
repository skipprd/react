# Locked decisions (Skippr ReAct)

Normative locks for this repository. Amend in the same change as the code when a lock still describes a discarded design. Do not implement both.

| Id | Lock |
|----|------|
| **R1** | **Tiger Style (ReAct dialect)** is the engineering style for kernel and host platform code. Normative: [tiger-style.md](./tiger-style.md). |
| **R2** | **Design by construction.** Illegal states are unrepresentable. Prefer types, enums, newtypes, and trait bounds over runtime checks. Always favour compile errors over runtime convention. |
| **R3** | **One design.** One authority, type, CLI, journal, and planner per job. Delete the old way in the same change. No shims, dual-write, feature flags, or “keep for now.” |
| **R4** | **Kernel purity.** `react-core` (agent loop, session, budget, transcript, tool protocol) has **no external side effects**. Clock, entropy, storage, LLM, and net are injected traits. Production adapters and the deterministic simulator run the **same owner code**. |
| **R5** | **Verification stack.** Every kernel/host change is incomplete until the matching tests exist: **unit**, **functional**, **deterministic simulation**, **property**, and **fuzz**. Normative: [testing.md](./testing.md). Sleep, wall-clock races, and live network are not correctness. |
| **R6** | Product hosts (`sde`, other binaries) own domain suites. This repo owns the suite-neutral runtime. Do not pull Skippr-specific warehouse/dbt suites back into `react`. Smoke this repo with in-repo example suites (`src/suites/kb`). Larger tests use `sde/crates/skipprd-react-suite-data-engineer`. |
| **R7** | **Cloudflare R2** is the only remote object store (threads, artifacts, Lance vectors). Auth is Skippr Cloud `OBJECTS_*` R2 S3 keys. **MUST NOT** AWS S3/STS/IAM as the storage backend. Normative: [storage.md](./storage.md). |
