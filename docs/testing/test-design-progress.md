---
workflowStatus: 'in-progress'
totalSteps: 5
stepsCompleted:
  - step-01-detect-mode
  - step-02-load-context
  - step-03-risk-and-testability
  - step-04-coverage-plan
lastStep: 'step-04-coverage-plan'
nextStep: './step-05-generate-output.md'
lastSaved: '2026-04-21'
mode: 'system-level'
scope: 'Watson v1.3.0 — cross-platform productivity launcher'
inputDocuments:
  - docs/plans/2025-12-29-watson-launcher-design.md
  - docs/plans/2025-12-31-notes-and-file-search-design.md
  - docs/plans/2025-12-29-watson-implementation.md
  - docs/plans/2025-12-31-notes-file-search-implementation.md
  - README.md
  - CHANGELOG.md
  - src-tauri/src/lib.rs + module tree
knowledgeFragments:
  - risk-governance
  - probability-impact
  - test-levels-framework
  - test-priorities-matrix
---

# Watson — System-Level Test Design

**Author:** Murat (bmad-tea)
**Date:** 2026-04-21
**Scope:** Watson v1.3.0, system-level (entire application)
**Target readers:** Will (maintainer), future contributors

---

## 1. Executive Summary

Watson is a Tauri 2.x desktop launcher (Rust backend + React 19 frontend + SQLite) shipping on Windows/macOS/Linux with auto-updates. Current state:

- **~10 Rust unit tests** covering fuzzy match, settings defaults, indexer smoke
- **Zero frontend tests**
- **Zero integration or E2E tests**
- **Zero tests for the 25 Tauri IPC commands** that form the contract between UI and backend
- **Zero tests for the update pipeline** despite 3 shipped update-related defects and one signing-key leak in the CHANGELOG

**Overall gate verdict:** CONCERNS — no critical blockers in code, but coverage gaps across SEC, DATA, and OPS categories are large enough that a regression could ship undetected. Current green CI tells you the code compiles, nothing more.

**Top three recommendations (in order):**

1. **Stand up a Rust integration test tier** for the Tauri IPC surface (notes CRUD, file indexer lifecycle, config round-trips). This is the single highest-ROI investment — the IPC contract is where the app's state actually mutates.
2. **Add Vitest + React Testing Library** for the frontend. The prefix-routing logic (`n `, `f `, `cb `, `>`, `g `, etc.) has moved from design doc into code without tests at either end. Covering it at the component/store level is fast and catches 80% of UX regressions.
3. **Formalize the release-pipeline smoke test.** The update bug tail in CHANGELOG (1.2.0–1.2.6 is almost entirely update-pipeline fixes) is a flashing light. A scheduled smoke test against the latest release — install, launch, update to a newer pre-release, verify signature — pays for itself after one near-miss.

---

## 2. Testable Architecture Map

```
┌───────────────────────────────────────────────────────────────┐
│                  React Frontend (src/)                        │
│   App.tsx • SearchBar • ResultsList • ResultItem              │
│   NoteEditor • Scratchpad • stores/app.ts (Zustand)           │
└──────────────────────────┬────────────────────────────────────┘
                           │ Tauri invoke() — 25 commands
┌──────────────────────────▼────────────────────────────────────┐
│                  Rust Backend (src-tauri/src/)                │
│  ┌────────────┐ ┌─────────────┐ ┌─────────────┐ ┌───────────┐ │
│  │ indexers/  │ │ search/     │ │ actions/    │ │ config/   │ │
│  │ macos,     │ │ SkimMatcher │ │ launch_app, │ │ settings, │ │
│  │ windows    │ │ + dispatcher│ │ exec_cmd,   │ │ TOML I/O  │ │
│  └────────────┘ └─────────────┘ │ open_url    │ └───────────┘ │
│  ┌────────────┐ ┌─────────────┐ └─────────────┘ ┌───────────┐ │
│  │ clipboard/ │ │ scratchpad/ │ ┌─────────────┐ │ db/       │ │
│  │ arboard +  │ │ SQLite      │ │ notes/      │ │ rusqlite, │ │
│  │ history    │ │ single-row  │ │ file+DB     │ │ schema    │ │
│  └────────────┘ └─────────────┘ └─────────────┘ └───────────┘ │
│  ┌────────────────────────────┐                               │
│  │ files/indexer — walkdir    │ + Tauri plugins:              │
│  └────────────────────────────┘   global-shortcut, updater,   │
│                                   shell, process              │
└───────────────────────────────────────────────────────────────┘
```

**Trust boundaries (where test effort concentrates):**
- User → Frontend (keyboard input, hotkey activation)
- Frontend → Backend (25 Tauri IPC commands, all untested)
- Backend → OS (file system, shell commands, clipboard, registry)
- Backend → Remote (GitHub Releases for updater)

---

## 3. Risk Register (System-Level)

Scoring scale: **Probability (1–3)** × **Impact (1–3)** = **Score (1–9)**.
Action thresholds: 1–3 DOCUMENT · 4–5 MONITOR · 6–8 MITIGATE · 9 BLOCK.
Categories: TECH · SEC · PERF · DATA · BUS · OPS.

### 3.1 Critical & high-risk items (score ≥ 6)

| ID | Cat | Title | P | I | Score | Action | Rationale |
|----|-----|-------|---|---|-------|--------|-----------|
| R-01 | OPS | Auto-update pipeline regression ships broken binary | 3 | 3 | **9** | BLOCK | Three update-pipeline bugs shipped in 30 days (1.2.0 → 1.2.6). Signing key was leaked once and regenerated. Untested update = shipping a bricked app to real users in minutes. |
| R-02 | DATA | SQLite schema migration corrupts user DB across version upgrade | 2 | 3 | **6** | MITIGATE | `db/schema.rs` exists, no migration framework seen. 1.3.0 added `notes` + `files` tables; future schema changes with no migration test = silent data loss. |
| R-03 | SEC | `launch_app` on Windows uses `cmd /C start "" <path>` with path from indexer | 2 | 3 | **6** | MITIGATE | Untrusted input unlikely (paths come from Start Menu/registry), but any attacker-controlled `.lnk` with shell metacharacters in its target could hit shell interpretation. Low probability, high impact. |
| R-04 | SEC | Web search `{instance}` placeholder not URL-encoded in template substitution | 3 | 2 | **6** | MITIGATE | `lib.rs:225-231` — `ws.instance` is interpolated into URL without encoding. User-editable via settings UI. Malicious config imported/synced could redirect search to attacker domain or inject JS via `javascript:` URL. |
| R-05 | DATA | Notes stored on disk + DB — index drift on crash/partial write | 2 | 3 | **6** | MITIGATE | Notes file-on-disk + SQLite index pattern. No test for crash recovery, half-written file, or stale index after external edit. Users' data. |
| R-06 | TECH | IPC command surface entirely untested (25 commands) | 3 | 2 | **6** | MITIGATE | Any refactor to `AppState`, search dispatcher, or a manager can break the frontend in ways cargo test will not see. This is the largest test-debt single item. |
| R-07 | PERF | File indexer on large home dirs blocks UI / runs unbounded | 3 | 2 | **6** | MITIGATE | `FileIndexer` walks `~/Documents`, `~/Downloads`, `~/Desktop` with `max_depth=5`. No test for: symlink loops, bind-mounts, network drives, very large trees. `reindex_files` is a synchronous Tauri command → UI hang on slow disks. |

### 3.2 Medium-risk items (score 4–5)

| ID | Cat | Title | P | I | Score | Action |
|----|-----|-------|---|---|-------|--------|
| R-08 | SEC | Clipboard history retains sensitive data in memory (passwords, tokens) | 3 | 2 | 6 → treat as **MITIGATE**, move up | (reclassified; see §3.3) |
| R-09 | TECH | SearchEngine.search filters by `name` only — `items` with zero score are dropped before ranking | 2 | 2 | 4 | MONITOR |
| R-10 | BUS | Prefix collisions between web-search keywords and reserved prefixes (`n`, `f`, `cb`, `s`) | 2 | 2 | 4 | MONITOR |
| R-11 | OPS | Global-shortcut registration silent-fails on platforms where Alt+Space is already bound | 2 | 2 | 4 | MONITOR |
| R-12 | PERF | `search()` iterates all apps + all web_searches on every keystroke | 2 | 2 | 4 | MONITOR |
| R-13 | DATA | Scratchpad is a single row; no test for the `set` → `get` round trip or concurrent write | 2 | 2 | 4 | MONITOR |
| R-14 | SEC | Settings TOML on disk has no schema/version field; malformed file = startup crash or silent reset | 2 | 2 | 4 | MONITOR |

### 3.3 Low-risk / documentation-only (score 1–3)

| ID | Cat | Title | P | I | Score | Action |
|----|-----|-------|---|---|-------|--------|
| R-15 | TECH | Zustand store has no tests, but the store is thin (pass-through) | 1 | 2 | 2 | DOCUMENT |
| R-16 | BUS | Theme token validation (CustomTheme hex color format) | 2 | 1 | 2 | DOCUMENT |
| R-17 | TECH | `resize_window` uses fixed width 600; hardcoded | 1 | 1 | 1 | DOCUMENT |
| R-18 | BUS | Icon cache is best-effort; stale icons after app upgrade | 2 | 1 | 2 | DOCUMENT |

### Gate decision (today, pre-mitigation)

- **Blockers (score=9):** 1 — R-01 (auto-update pipeline)
- **Concerns (score 6–8):** 6 — R-02 through R-07
- **Decision:** **FAIL → CONCERNS after mitigation plan accepted**

R-01 is classified BLOCK because the consequence is shipping to users' machines silently. It comes off BLOCK once a pipeline smoke test exists (see §5, TA-01).

---

## 4. Testability Assessment

What makes the code hard to test today, in order of drag coefficient:

1. **Tauri `State<AppState>` injection.** All command bodies take `State<AppState>` — testing them in isolation requires building an `AppState` with real `Database`, real `ScratchpadManager`, real `NotesManager`. Fix: extract a thin "service" layer per module (pure functions over `&Database` or over a `NotesStore` trait); commands become two-line adapters. Test the services, not the commands.
2. **No dependency inversion on platform-specific code.** `indexers::get_indexer()` returns the platform indexer unconditionally. Fix: accept an `&dyn AppIndexer` in the search path and let tests pass a `MockIndexer`.
3. **File-system and SQLite paths are implicit.** `Database::new()` and `NotesManager::new()` pick paths from `directories`/`ProjectDirs`. Fix: accept an injectable root in constructors; tests pass a `tempfile::TempDir`.
4. **Clipboard monitor is a background thread started in `run()`.** Fix: move the monitor loop behind a `start_monitoring(shutdown: impl Future)` signature to make it deterministic in tests.
5. **Frontend has no test runner.** Fix: add Vitest + React Testing Library; convert `src/stores/app.ts` and the prefix-routing helpers in `App.tsx` to testable units (pull router logic into `src/lib/parseQuery.ts`).
6. **Update pipeline only exercisable in CI against real GitHub Releases.** Fix: end-to-end test that uses a sandboxed updater endpoint (Tauri updater supports custom endpoints).

None of these are blockers — they're refactors that pay back within the first sprint of test writing.

---

## 5. Coverage Plan (Test Level × Priority)

Mapping risks → tests. Each row names a test at the lowest level that can realistically cover the risk.

### 5.1 P0 — Must-test (blocks release)

| TID | Risk | Level | Description | Tool |
|-----|------|-------|-------------|------|
| TA-01 | R-01 | CI (release smoke) | **MVP landed** (`.github/workflows/release-smoke.yml`): validate `latest.json` schema, probe every artifact URL, verify minisign signatures against configured pubkey, launch-smoke the installer on win/mac/linux matrix, auto-file an issue on failure. Triggers on `release.published` and `workflow_dispatch`. | GitHub Actions; `minisign -V`; NSIS silent install; Xvfb for linux AppImage. |
| TA-01b | R-01 | E2E (updater round-trip) | **Deferred follow-up.** Install *previous* release, point at staged pre-release endpoint, run tauri-driver to trigger updater, verify signed update applies, assert relaunch under new version. Also consider moving smoke inline in `release.yml` so a failing smoke keeps the release drafted. | Tauri WebDriver + WebDriverIO/Playwright; staged updater endpoint. |
| TA-02 | R-02 | Integration (Rust) | For each SQLite schema version `N`, seed DB with `N`-shaped data, upgrade to `N+1`, verify all tables intact, verify row counts, verify no data loss. | `cargo test` + `tempfile`; tests live under `src-tauri/tests/db_migrations.rs`. |
| TA-03 | R-04 | Unit (Rust) | Web-search URL construction: `{instance}` and `{query}` — input sanitization, rejection of `javascript:` scheme, encoding of reserved characters. | `cargo test` in `config/tests.rs` or new `search/url_builder_tests.rs`. |
| TA-04 | R-05 | Integration (Rust) | Notes lifecycle: create → crash-simulate (drop manager mid-write) → reopen → index consistency. External-edit-on-disk detection. | `cargo test` with `tempfile`; use `Drop` + explicit `sync` calls to simulate crash. |
| TA-05 | R-06 | Integration (Rust) | Round-trip test for each Tauri command that mutates state — use `tauri::test::mock_app()` where available, otherwise test the underlying service functions after the refactor in §4.1. | `cargo test`; tests under `src-tauri/tests/ipc_commands.rs`. |

### 5.2 P1 — Should-test (core functionality)

| TID | Risk | Level | Description | Tool |
|-----|------|-------|-------------|------|
| TA-06 | R-03 | Unit (Rust) | `launch_app` path handling: paths with spaces, Unicode, embedded `"`/`&`/`\|`/`%`. Assert arguments passed to `Command::new` are the exact path, never a shell-interpreted string. | `cargo test`; use `Command`-arg capture helper. |
| TA-07 | R-07 | Integration (Rust) | File indexer against a fixture tree: symlink loop, 10k-file directory, path with Unicode, excluded-pattern respected, `max_depth` honored. Budget: indexing 1k files < 500ms. | `cargo test` with `tempfile`. |
| TA-08 | R-07 | Integration (Rust) | `reindex_files` is non-blocking from the UI perspective — split synchronous IPC from the actual walk (moves to a background task); test the task cancellation path. | `cargo test` after refactor. |
| TA-09 | R-06/R-10 | Unit (**Rust**, relocated from frontend) | Classifier tests — every prefix (`n `, `notes `, `f `, `files `, `cb `, `clip `, web-search keywords). Conflict case: user-defined web search with keyword `n` must not shadow notes. _Scope correction 2026-04-21: the actual dispatch lives in `lib.rs::search` (Rust), not the frontend. Extracted to `src-tauri/src/search/dispatch.rs` with pure `classify_prefix_route()` + `match_web_search()` functions. Frontend keyboard shortcuts in `SearchBar.tsx` remain untested — defer to TA-10 when Vitest setup is warranted._ | `cargo test`. |
| TA-10 | R-06 | Component (frontend) | `SearchBar` + `ResultsList`: keyboard navigation (Up/Down/Tab/Shift-Tab/Enter/Esc), result type → action mapping, empty-query behavior. | Vitest + React Testing Library. |
| TA-11 | R-11 | Integration (Rust) | Global-shortcut registration failure path — mock `GlobalShortcutExt` and assert graceful fallback to tray-only activation with a surfaced error, not silent drop. | `cargo test`. |
| TA-12 | R-13 | Unit (Rust) | Scratchpad: `set` → `get` round-trip, empty string, large content (>10KB), Unicode, concurrent writes serialize correctly. | `cargo test`. |
| TA-13 | R-14 | Unit (Rust) | Settings TOML parser: malformed file, missing section, unknown key, version mismatch — must fall back to defaults, never crash. Log surfaces the parse error. | `cargo test`. |

### 5.3 P2 — Nice-to-test

| TID | Risk | Level | Description |
|-----|------|-------|-------------|
| TA-14 | R-08 | Unit (Rust) | Clipboard: configurable passthrough ignore-list (skip entries matching a regex — e.g., password managers' format), cleared on explicit clear. |
| TA-15 | R-09 | Unit (Rust) | SearchEngine: empty `items`, items whose `name` is empty, description-matched items (currently ignored), score ties. |
| TA-16 | R-12 | Perf (Rust criterion) | Benchmark `search()` at 500/1k/5k indexed apps; assert p99 < 20ms on dev hardware. |
| TA-17 | R-16 | Component (frontend) | Custom theme color validation — rejects non-hex / non-CSS-color strings in the Settings UI. |

### 5.4 P3 — Defer / manual

- R-15 (Zustand store) — covered transitively by TA-10.
- R-17 (hardcoded width 600) — visual only; catch via future visual regression if added.
- R-18 (stale icon cache) — manual smoke before each release.

---

## 6. Test Infrastructure Recommendations

To execute the plan above, add:

1. **Rust test tooling:**
   - `tempfile` (already common) — dev-dep for isolated DB/file tests.
   - `serial_test` — for tests that touch shared state (clipboard, global shortcut).
   - `insta` — for snapshotting Tauri command response JSON.
   - `criterion` — only when TA-16 lands.
2. **Frontend test tooling:**
   - `vitest`, `@testing-library/react`, `@testing-library/user-event`, `jsdom` — dev-deps.
   - `@tauri-apps/api/mocks` — mock `invoke` in tests.
3. **E2E / release smoke:**
   - `tauri-driver` + WebDriverIO or Playwright with Tauri adapter.
   - Separate GitHub Actions workflow — `release-smoke.yml` — triggers on release-tag push, installs from the actual artifact, performs updater round-trip.
4. **CI policy additions:**
   - Require `cargo test` + `vitest run` green on every PR (blocking).
   - Release-smoke job is blocking on release tags; advisory on main-branch pushes.

---

## 7. Traceability Matrix (Coverage Targets)

| Feature | Source file(s) | Current cov. | Target cov. | P | Gap |
|---------|---------------|--------------|-------------|---|-----|
| Fuzzy search & ranking | `search/mod.rs` | 4 unit tests | Unit >90%, fuzz input | P1 | edge cases + perf (TA-15, TA-16) |
| Settings load/save | `config/settings.rs`, `config/mod.rs` | 3 tests (defaults only) | Unit >90% | P1 | TOML round-trip, migration (TA-13, TA-03) |
| App indexer (Win/macOS) | `indexers/*.rs` | 3 smoke tests | Integration on fixtures | P2 | fixture-based indexing per platform |
| System commands | `actions/system.rs` | **0** | Unit >90% (path of each cmd ID) | P1 | TA-06 adjacent |
| `launch_app` / `open_url` | `actions/mod.rs` | **0** | Unit + Command-arg capture | P0 | TA-06 |
| Clipboard history | `clipboard.rs` | **0** | Unit + integration (monitor loop) | P1 | TA-14 |
| Scratchpad | `scratchpad.rs` | **0** | Unit round-trip | P1 | TA-12 |
| Notes CRUD + search | `notes/mod.rs`, `notes/storage.rs`, `notes/tags.rs` | **0** | Integration (tempdir+DB) | P0 | TA-04 |
| File search indexer | `files/indexer.rs`, `files/mod.rs` | **0** | Integration on fixture tree | P0 | TA-07, TA-08 |
| Database / schema | `db/mod.rs`, `db/schema.rs` | **0** | Integration + migration | P0 | TA-02 |
| Tauri IPC commands (25) | `lib.rs` | **0** | Integration per command | P0 | TA-05 |
| Prefix routing (`n `, `f `, `cb `, `>`, web) | `lib.rs::search` + `App.tsx` | **0** | Unit (both sides) | P0 | TA-09 |
| Frontend components | `src/components/*` | **0** | Component tests | P1 | TA-10 |
| Global shortcut | `lib.rs::setup` | **0** | Integration with mock plugin | P1 | TA-11 |
| Auto-updater | Tauri plugin | **0** | E2E on release tags | P0 | TA-01 |

**Aggregate today:** ~10 tests, <10% of the testable surface.
**Aggregate after P0+P1 execution:** projected ~75 tests, 60–70% of testable surface, full coverage of release-blocking paths.

---

## 8. Execution Sequence (recommended)

If I were sequencing the work for a solo maintainer (one sprint = one week):

**Sprint 1 — Unblock P0 foundations (≈ 3 days of focused time)**
- Refactor: inject `tempfile` root into `Database::new`, `NotesManager::new`, `FileSearchManager::new`. This unlocks 80% of the integration tests with no runtime behavior change.
- Add `cargo test` integration tier (`src-tauri/tests/*`) — seed TA-02, TA-04, TA-05 scaffolds.
- Stand up Vitest + one smoke test on `SearchBar`.

**Sprint 2 — Coverage of P0 risks**
- TA-01 release-smoke workflow (the highest-pain item, do it early while updater pain is still in recent memory).
- TA-03, TA-04, TA-05, TA-09, TA-10.

**Sprint 3 — P1 polish**
- TA-06, TA-07, TA-08, TA-11, TA-12, TA-13.

**Sprint 4+ — P2 as time permits**

---

## 9. Gate Recommendation

**Current state: FAIL** (because R-01 auto-update risk has score 9 and no mitigation in place).

**Mitigation path to CONCERNS:** accept TA-01 (release-smoke) as the R-01 mitigation plan with Will as owner, target completion before next minor release.

**Path to PASS:** complete P0 test set (TA-01 through TA-05) and confirm all pass in CI for one full release cycle.

This is a standard single-maintainer OSS posture: the code is clean, the shipped features work, and the risk is concentrated in "no one will notice when something breaks" rather than "something is currently broken." A modest test investment moves the whole project from CONCERNS to PASS quickly.

---

## 10. Next step

Proceed to **RV (Review Tests)** — Murat will now review the existing test files (`search/tests.rs`, `config/tests.rs`, `indexers/tests.rs`) against the best-practices checklist and this TD plan, and call out what's shallow, what's redundant, and what's missing.

_End of TD document._
