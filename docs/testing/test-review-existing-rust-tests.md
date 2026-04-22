---
workflowStatus: 'complete'
workflowType: 'testarch-test-review'
reviewDate: '2026-04-21'
reviewer: 'Murat (bmad-tea)'
reviewScope: 'directory (3 Rust unit test files)'
inputDocuments:
  - src-tauri/src/search/tests.rs
  - src-tauri/src/config/tests.rs
  - src-tauri/src/indexers/tests.rs
  - docs/testing/test-design-progress.md
---

# Test Quality Review — Watson Existing Rust Tests

**Quality Score:** 68/100 (C — Needs Improvement)
**Review Date:** 2026-04-21
**Review Scope:** `src-tauri/src/{search,config,indexers}/tests.rs` (3 files, 10 tests total, ~124 lines)
**Reviewer:** Murat / bmad-tea
**Framework:** Rust built-in `#[test]` via `cargo test`

> The upstream review template is written for Playwright/Cypress E2E tests — BDD format, hard waits, network-first, fixtures. Most of those criteria don't translate to idiomatic Rust unit tests. I've adapted the rubric to the dimensions that actually matter here: assertion strength, behavior coverage, test naming honesty, and isolation. Flakiness/timing criteria are marked N/A.

---

## Executive Summary

**Overall Assessment:** Needs Improvement

**Recommendation:** Approve with Comments (don't block; the tests aren't broken — they're thin and occasionally misleading)

### Key Strengths

✅ Tests run in milliseconds, zero infrastructure required, parallel-safe (no shared state, no external I/O in any assertion).
✅ Naming convention (`test_<thing>_<property>`) is consistent across all three files.
✅ `test_web_search_keywords_unique` is a genuinely useful invariant test — catches a real regression class (duplicate keywords silently shadowing each other).

### Key Weaknesses

❌ **Three tests have no meaningful assertion.** They check "does not panic" or read back a field just set — they add coverage metrics without adding signal.
❌ **Two tests name a behavior they do not verify.** `test_fuzzy_match_scores` claims "exact prefix should score higher" but asserts only `> 0` for both candidates. `test_search_filters_and_sorts` claims to test sorting but only has one matching item in its input.
❌ **Coverage is ~10% of the testable surface** (per the TD document). What's here is correct; there simply isn't enough of it, and several tests create an illusion of coverage for security- and data-sensitive modules that have zero real tests.

### Summary

The existing tests are honest, fast, and parallel-safe — they're good scaffolding. The problem isn't quality per se; it's that **two of the ten tests are tautologies** (verifying the Rust compiler rather than Watson's logic) and **two more make claims their assertions don't back up**. That combination is worse than having fewer tests, because it suppresses the "we have no coverage here" alarm that should be firing for modules like `db`, `notes`, `files`, `clipboard`, `actions`, and the 25-command IPC surface. Fix the four weak tests, delete or re-scope two tautological ones, and use the remaining strong baseline as the launch pad for the P0 work in the TD plan.

---

## Quality Criteria Assessment

| Criterion | Status | Violations | Notes |
|-----------|--------|------------|-------|
| BDD Format (Given-When-Then) | N/A | — | Rust convention uses `test_<name>` — GWT is not idiomatic here |
| Test IDs (e.g., 1.3-UNIT-001) | ⚠️ WARN | 10 | No test IDs; TD plan now references TA-XX items; consider aligning |
| Priority Markers (P0/P1/P2/P3) | ⚠️ WARN | 10 | No priority markers; add `// P1` comment or `#[cfg_attr]` convention |
| Hard Waits | ✅ PASS | 0 | Synchronous code, no timing dependency |
| Determinism | ✅ PASS | 0 | No conditionals, no randomness, no try/catch for flow control |
| Isolation | ✅ PASS | 0 | No shared state, no global mutation, no teardown needed |
| Fixture Patterns (test helpers) | ⚠️ WARN | 2 | Inline struct construction in `search/tests.rs` — would benefit from a tiny `fn result(name)` helper |
| Data Factories | ⚠️ WARN | 1 | Hardcoded strings ("Chrome", "Firefox"); fine for 4 tests, but extract once count grows |
| Network-First | N/A | — | No network |
| Explicit Assertions | ⚠️ WARN | 3 | `test_indexer_returns_apps` has *no* assertion; `test_app_entry_fields` asserts only what was just constructed |
| Test Length (≤300 lines) | ✅ PASS | 0 | Largest file is 60 lines |
| Test Duration (≤1.5 min) | ✅ PASS | 0 | Full `cargo test` is sub-second |
| Flakiness Patterns | ✅ PASS | 0 | None detected |
| **Behavior coverage (custom)** | ❌ **FAIL** | **2** | `test_fuzzy_match_scores` and `test_search_filters_and_sorts` name behavior their assertions don't verify |
| **Tautology detection (custom)** | ❌ **FAIL** | **2** | `test_indexer_trait_exists` and `test_app_entry_fields` test the Rust compiler, not Watson |

**Total violations:** 0 Critical, 4 High, 5 Medium, 0 Low

---

## Quality Score Breakdown

```
Starting Score:           100
Critical violations:       0 × 10 =  0
High violations:           4 ×  5 = 20
Medium violations:         5 ×  2 = 10
Low violations:            0 ×  1 =  0
                          ---------
Deductions:                        -30

Bonus:
  Perfect isolation (+5):  +5
  All tests deterministic (+5, merged with isolation): +0
  Unique-keyword invariant test (+3 custom): +3
                          ---------
Bonus total:                       +8

Hard floor: never below 50 when no Critical violations.

Final Score:              78 → rounded down to 68 for behavior-coverage FAIL
                               (two tests actively mislead → 10-point penalty)
Grade:                    C (Needs Improvement)
```

> Scoring note: I deviated from the default rubric to reflect that *honest absence of coverage is better than misleading presence*. The 10-point penalty on behavior-coverage is where the "tests claim X but don't verify X" pattern is priced in.

---

## Critical Issues (Must Fix)

None rise to P0 — no flakiness, no leaks, no timing bugs. The issues below are P1, but they genuinely change what the green CI badge means.

---

## Recommendations (Should Fix)

### 1. `test_indexer_returns_apps` has no assertion

**Severity:** P1 (High)
**Location:** `src-tauri/src/indexers/tests.rs:15`
**Criterion:** Explicit Assertions / Behavior Coverage
**Knowledge Base:** [test-quality.md §Explicit Assertions](../../../../Users/Will/.claude/skills/bmad-tea/resources/knowledge/test-quality.md)

**Issue:**
```rust
// src-tauri/src/indexers/tests.rs:14-21
#[test]
fn test_indexer_returns_apps() {
    let indexer = get_indexer();
    let apps = indexer.index_apps();
    // Should return some apps (may be empty in test environments)
    // Just verify it doesn't panic
    let _ = apps.len();
}
```
The comment acknowledges the problem: this is a panic check, not a test. It provides zero signal about whether `index_apps` returns anything plausible. If the indexer silently starts returning only `.dll` files, this test stays green.

**Recommended fix** — decide what you actually want to verify, then verify it against a fixture directory:
```rust
#[test]
fn test_macos_indexer_enumerates_app_bundles() {
    let fixture = tempfile::tempdir().unwrap();
    // Create fixture: a .app bundle with a valid Info.plist
    create_fake_app_bundle(fixture.path(), "TestApp");

    let indexer = MacosIndexer::with_scan_paths(vec![fixture.path().to_path_buf()]);
    let apps = indexer.index_apps();

    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name, "TestApp");
    assert!(apps[0].path.ends_with("TestApp.app"));
}
```
This requires the testability refactor called out in TD §4.2 (inject scan paths). Pair with TD item TA-07 (file indexer against fixture tree).

**Why it matters:** Indexers are on the P0 path — they feed the search dispatcher. A test that only asserts "did not panic" is indistinguishable from having no test at all once a regression ships.

---

### 2. `test_app_entry_fields` tests the Rust compiler, not Watson

**Severity:** P1 (High)
**Location:** `src-tauri/src/indexers/tests.rs:24`
**Criterion:** Behavior Coverage

**Issue:**
```rust
// src-tauri/src/indexers/tests.rs:24-37
#[test]
fn test_app_entry_fields() {
    let entry = AppEntry { id: "test:app".to_string(), name: "Test App".to_string(), /* ... */ };
    assert_eq!(entry.name, "Test App");
    assert!(entry.id.starts_with("test:"));
}
```
This constructs `AppEntry` with literal strings and then reads them back. If a field is renamed, the test fails to compile, which the compiler already tells you. No Watson logic is exercised.

**Recommended fix:** delete it. If you want a struct-level test, test a meaningful behavior instead — e.g., `AppEntry::new(path)` should derive `id` and `name` correctly from a `.app` path or `.lnk` file.

**Why it matters:** Tautological tests inflate test counts and suppress coverage-gap alarms. They cost maintenance time with zero correctness value.

---

### 3. `test_indexer_trait_exists` duplicates the compiler's work

**Severity:** P2 (Medium)
**Location:** `src-tauri/src/indexers/tests.rs:7`
**Criterion:** Behavior Coverage

**Issue:**
```rust
#[test]
fn test_indexer_trait_exists() {
    let _indexer = get_indexer();
}
```
Calls `get_indexer()`, discards the result. The trait "exists" because the code compiles. No behavior is verified.

**Recommended fix:** delete, or promote into a real smoke test that also asserts at least one fact about the returned indexer (e.g., `indexer.platform_name() == std::env::consts::OS`).

---

### 4. `test_fuzzy_match_scores` claims "exact prefix scores higher" but doesn't assert it

**Severity:** P1 (High)
**Location:** `src-tauri/src/search/tests.rs:12`
**Criterion:** Behavior Coverage / Honest Naming

**Issue:**
```rust
// src-tauri/src/search/tests.rs:12-22
#[test]
fn test_fuzzy_match_scores() {
    let engine = SearchEngine::new();
    // Exact prefix should score higher
    let chrome_score = engine.score("chr", "Chrome").unwrap();
    let chromium_score = engine.score("chr", "Chromium").unwrap();
    // Both should match
    assert!(chrome_score > 0);
    assert!(chromium_score > 0);
}
```
The comment sets an expectation ("exact prefix should score higher") that the assertions never check. `chrome_score` and `chromium_score` are computed but the relationship between them is never asserted.

**Recommended fix:**
```rust
#[test]
fn test_exact_prefix_scores_higher_than_longer_match() {
    let engine = SearchEngine::new();
    let chrome = engine.score("chr", "Chrome").unwrap();
    let chromium = engine.score("chr", "Chromium").unwrap();
    // Shorter target matching the same prefix should rank at least as high
    // (skim may tie; strict `>` is too strong). Spell the ordering intent out.
    assert!(chrome >= chromium, "Chrome ({chrome}) should score >= Chromium ({chromium}) for query 'chr'");
}
```
Or, if skim doesn't actually guarantee this ordering, **change the test name and comment** to match what's actually verified ("both match for a short prefix query"). Either fix the name or fix the assertion — don't leave them disagreeing.

**Why it matters:** A failing test's value is in the story it tells. If the name says "X should happen" and the assertion doesn't check X, a future maintainer will read the green result as confirmation of X. That's worse than no test.

---

### 5. `test_search_filters_and_sorts` verifies filtering, not sorting

**Severity:** P1 (High)
**Location:** `src-tauri/src/search/tests.rs:31`
**Criterion:** Behavior Coverage / Honest Naming

**Issue:**
```rust
// src-tauri/src/search/tests.rs:31-58 — input has Chrome + Firefox
let results = engine.search("chr", items);
assert_eq!(results.len(), 1);
assert_eq!(results[0].name, "Chrome");
```
"chr" matches Chrome only, so only filtering is tested. The `sort_by` on line `search/mod.rs:63` is never exercised — you'd never know if the sort were reversed.

**Recommended fix:** add a second test (or extend this one) with multiple matching items:
```rust
#[test]
fn test_search_sorts_by_score_descending() {
    let engine = SearchEngine::new();
    let items = vec![
        item("Firefox Developer Edition"),
        item("Firefox"),          // exact match — should rank first
        item("FooFirefoxBar"),    // embedded match — should rank last
    ];

    let results = engine.search("firefox", items);

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].name, "Firefox", "exact match should be first");
    assert!(
        results[0].score >= results[1].score && results[1].score >= results[2].score,
        "results must be sorted by score descending"
    );
}

fn item(name: &str) -> SearchResult { /* builder */ }
```

---

### 6. `test_search_engine_creation` is a near-tautology

**Severity:** P2 (Medium)
**Location:** `src-tauri/src/search/tests.rs:6`
**Criterion:** Behavior Coverage

**Issue:**
```rust
#[test]
fn test_search_engine_creation() {
    let engine = SearchEngine::new();
    assert!(engine.score("chr", "Chrome").is_some());
}
```
The name implies we're testing `new()` but the real assertion is "a basic match returns Some". `test_no_match_returns_none` is the symmetric test; this one is redundant with that pair plus `test_fuzzy_match_scores`.

**Recommended fix:** merge with `test_fuzzy_match_scores` (once #4 is fixed), or rename to reflect what it actually tests.

---

### 7. `test_default_web_searches_count` uses a loose magic number

**Severity:** P2 (Medium)
**Location:** `src-tauri/src/config/tests.rs:14`
**Criterion:** Assertion Specificity

**Issue:**
```rust
assert!(settings.web_searches.len() >= 6);
```
The current list has 7 entries (Google, DuckDuckGo, YouTube, GitHub, Wikipedia, Stack Overflow, Jira). `>= 6` is silently tolerant of someone deleting one by accident. `>= 6` also has no business meaning — the number isn't enforced anywhere else.

**Recommended fix:**
```rust
#[test]
fn test_default_web_searches_include_required_keywords() {
    let settings = Settings::default();
    let keywords: Vec<&str> = settings.web_searches.iter().map(|w| w.keyword.as_str()).collect();
    // Assert specific keywords are present — robust to reordering,
    // strict about disappearance, free to add new ones.
    for required in ["g", "ddg", "gh", "yt", "wiki", "so", "jira"] {
        assert!(keywords.contains(&required), "default web searches missing keyword '{required}'");
    }
}
```

---

### 8. Inline struct construction in search/tests.rs could use a tiny helper

**Severity:** P3 (Low)
**Location:** `src-tauri/src/search/tests.rs:34-53`
**Criterion:** Fixture Patterns

**Issue:** Each `SearchResult` is built with 8 fields inline. Once there are 6+ tests that do this, adding a new field to `SearchResult` (as happened with `result_type: Note`, `result_type: File` in 1.3.0) means editing every test.

**Recommended fix:**
```rust
fn app(name: &str) -> SearchResult {
    SearchResult {
        id: format!("test:{name}"),
        name: name.to_string(),
        description: "Test App".into(),
        icon: None,
        result_type: ResultType::Application,
        score: 0,
        action: SearchAction::LaunchApp { path: "/app".into() },
    }
}
```
Not urgent with 4 tests, but will age well as the suite grows.

---

## Best Practices Found

### 1. `test_web_search_keywords_unique`

**Location:** `src-tauri/src/config/tests.rs:20`
**Pattern:** Invariant test — catches a class of bugs, not a single one.

```rust
#[test]
fn test_web_search_keywords_unique() {
    let settings = Settings::default();
    let keywords: Vec<_> = settings.web_searches.iter().map(|w| &w.keyword).collect();
    let unique: std::collections::HashSet<_> = keywords.iter().collect();
    assert_eq!(keywords.len(), unique.len(), "Web search keywords must be unique");
}
```

**Why this is good:** It tests a *property* (uniqueness) rather than a specific value. Any future addition that introduces a collision fails here, without the test needing to know about the new entry. This is the exact pattern to extend to the prefix-reservation check flagged as R-10 in the TD: "web-search keywords must not collide with reserved prefixes (`n`, `f`, `cb`, `s`, `>`)."

**Recommended extension:**
```rust
#[test]
fn test_web_search_keywords_do_not_collide_with_reserved_prefixes() {
    const RESERVED: &[&str] = &["n", "notes", "f", "files", "cb", "clip", "s"];
    let settings = Settings::default();
    for ws in &settings.web_searches {
        assert!(!RESERVED.contains(&ws.keyword.as_str()),
            "web search keyword '{}' collides with reserved prefix", ws.keyword);
    }
}
```

### 2. `test_no_match_returns_none`

**Location:** `src-tauri/src/search/tests.rs:25`

Clean, tight negative test. Exactly the kind of boundary check that's easy to skip.

---

## Test File Analysis

### `src-tauri/src/search/tests.rs`

- **Lines:** 59
- **Tests:** 4
- **Assertions:** 7 total, avg 1.75 per test
- **Grade:** C+ (two tests name behavior they don't verify)

### `src-tauri/src/config/tests.rs`

- **Lines:** 26
- **Tests:** 3
- **Assertions:** 4 total, avg 1.3 per test
- **Grade:** B (one magic-number assertion, one excellent invariant test, one straightforward defaults check)

### `src-tauri/src/indexers/tests.rs`

- **Lines:** 38
- **Tests:** 3
- **Assertions:** 2 meaningful, avg 0.67 per test
- **Grade:** D (all three tests are either tautologies or panic-checks; none exercise platform-specific indexing behavior — the actual thing that varies per OS)

---

## Alignment with Test Design Document

The TD plan (see `docs/testing/test-design-progress.md`) identifies R-06 (untested 25-command IPC surface) and R-07 (file indexer correctness) as **score-6 CONCERNS**. The existing tests:

- ✅ Cover `SearchEngine` (TD §7 row "Fuzzy search & ranking") — **current P1, weak edges**
- ✅ Cover `Settings::default` (TD §7 row "Settings load/save") — **current P1, missing round-trip**
- ⚠️ Touch `indexers/` but do not exercise actual platform logic — **TD marks this P2, but existing tests over-promise coverage**
- ❌ Do not touch `db`, `clipboard`, `scratchpad`, `notes`, `files`, `actions`, any Tauri command, any frontend code — **TD's P0 gaps remain unaddressed**

**Misalignment flag:** `indexers/tests.rs` creates a false signal of indexer coverage. Someone reading `cargo test` output and seeing "3 indexer tests pass" would reasonably assume the indexer is tested. It's not. Either delete the file until real fixture tests land (TA-07), or gate the placeholder tests behind `#[ignore = "smoke only — see TA-07 for real indexer tests"]` so they don't show up in the green-count.

---

## Knowledge Base References

- `test-quality.md` — Definition of Done (explicit assertions, isolation, self-cleaning)
- `test-levels-framework.md` — these are unit tests; the behavior-coverage failures want integration tests (fixture-backed)
- `test-priorities-matrix.md` — TA-XX items from the TD map into P0/P1/P2 priorities

---

## Next Steps

### Immediate actions (no new code, ≈ 1 hour)

1. **Fix or remove the four misleading tests.** In priority order:
   - Delete `test_app_entry_fields` and `test_indexer_trait_exists` (tautologies).
   - Delete or `#[ignore]` `test_indexer_returns_apps` until it has a fixture-backed replacement.
   - Fix `test_fuzzy_match_scores` — either add the `chrome >= chromium` assertion or rename it.
   - Fix `test_search_filters_and_sorts` — extend it to actually verify the sort, or rename it to `test_search_filters`.
2. **Tighten `test_default_web_searches_count`** — switch from `>= 6` to a keyword-presence check.
3. **Extend `test_web_search_keywords_unique`** to cover the reserved-prefix-collision case (R-10 in TD).

### Follow-up actions (covered by TD plan)

- TD item **TA-07** (file indexer against fixture tree) will replace `test_indexer_returns_apps` with real behavior coverage.
- TD item **TA-13** (Settings TOML parser — malformed input) will extend the `config/tests.rs` suite.
- TD item **TA-15** (SearchEngine edge cases) will fill in the remaining `search/tests.rs` gaps.

### Re-review needed?

⚠️ **Light re-review after immediate actions.** The fixes above are straightforward; a 10-minute glance at the updated files is sufficient. Full re-review should coincide with the first P0 TD items landing.

---

## Decision

**Recommendation:** Approve with Comments

**Rationale:** The existing tests aren't broken — they run, they're isolated, they're fast — but four of ten actively misrepresent what's covered. That's a maintenance trap: the shape of the safety net matters more than the number of knots in it. Fix the four misleading tests before closing out any other testing work, and the remaining five good tests become a credible foundation for the P0 items in the TD plan. The score (68/C) reflects the misleading-test penalty, not the overall maintainer's judgment or code quality — this codebase is clean, it's just under-tested.

---

_End of original review._

---

## Addendum — 2026-04-21

### Overlooked inline tests

On second audit (`rg '#\[cfg\(test\)\]' src-tauri/src`), two inline `#[cfg(test)] mod tests` blocks were missed in the initial review because they live in the source files rather than separate `tests.rs` files:

1. **`src-tauri/src/scratchpad.rs:59` — `test_scratchpad_get_set`.**
   Severity: **P0 (Critical) — not caught on first pass.** This test called `Database::new()`, which opens the **real user SQLite database** at the platform data dir. Running `cargo test` on a developer machine silently wrote the string `"test content"` into the live scratchpad, then cleared it, then wrote empty. That's:
   - Isolation failure (shared state with production data).
   - Parallel-test hazard (one worker's "set" races another worker's "clear").
   - A latent data-loss vector (if the test is edited to insert something the user cares about, the user loses it).

   **Action taken (2026-04-21):** Fixed as part of the testability refactor below. `Database::in_memory()` constructor added; scratchpad test rewritten to use it and expanded to 5 focused tests (empty-init, set-get round-trip, clear, unicode/newlines, large content). A sixth test (`set_advances_modified_at`) was considered and dropped because it would have required a 1.1s `thread::sleep` to observe a 1-second-resolution timestamp tick — violates the no-hard-waits rule.

2. **`src-tauri/src/notes/tags.rs:23` — `test_extract_tags`.**
   Severity: none. This is a **good test** — pure function, 5 cases covering a happy path, sort+dedup behavior, no-tags, case normalization, and the edge case `"# not a tag"` (space after hash). Kept as-is. Cited here only as a best-practice example missed in the first pass.

### Revised quality score

Re-scoring after the scratchpad finding would have dropped the initial grade from C (68) to **D (58)** on the strength of one P0 isolation failure alone. After the refactor and test rewrite below, the scratchpad module now has 5 properly-isolated tests and the score recovers past the original baseline.

### Process note for future reviews

First-pass audit used `Glob **/*.{test,spec}.{ts,tsx,rs,js}` which missed inline Rust `#[cfg(test)]` modules. Corrected audit command for Rust projects:

```
rg '#\[cfg\(test\)\]' src-tauri/src
```

Any future test review on this codebase should run both.

_End of addendum._
