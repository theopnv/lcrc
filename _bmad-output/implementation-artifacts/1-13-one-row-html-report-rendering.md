# Story 1.13: One-row HTML report rendering

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As Theop,
I want `lcrc scan` to write a `latest.html` file containing one row showing the canary measurement after each cell write,
so that I can open the report in a browser and verify the renderer interlocks with the cache (FR32, FR33, FR39 default path only).

## Acceptance Criteria

**AC1.** **Given** a successful `lcrc scan` produces a cell **When** the scan completes **Then** `$XDG_DATA_HOME/lcrc/reports/latest.html` exists. On macOS with etcetera, this resolves to `~/Library/Application Support/lcrc/reports/latest.html`.

**AC2.** **Given** that file **When** I open it in a browser **Then** it renders one row containing model identifier, task name, pass/fail status, and scan timestamp. (No Wilson CIs, no badges, no canonical header, no streaming — those land in Epic 2.)

**AC3.** **Given** the file is being written **When** the write process is killed midway **Then** the existing `latest.html` (if any) is unaffected — the writer uses tempfile + atomic rename per the architecture's atomic-write pattern.

**AC4.** **Given** the file is opened in an offline browser (no network) **When** it loads **Then** it renders fully — no external CSS, JS, fonts, or images (FR32 self-contained).

**AC5.** **Given** the HTML render completes **When** I inspect the file mode **Then** it is 0o644 (readable by owner/group, not world-writable).

## Tasks / Subtasks

- [x] **T1. Add `pub mod report;` to `src/lib.rs`** (AC: all)
  - [x] T1.1 Insert `pub mod report;` into `src/lib.rs` in alphabetical order after `pub mod output;` and before `pub mod sandbox;`.

- [x] **T2. Configure askama template directory in `Cargo.toml`** (AC: all)
  - [x] T2.1 Add the following section at the end of `Cargo.toml` (after `[lints.clippy]`):
    ```toml
    [package.metadata.askama]
    dirs = ["src/report/templates"]
    ```
    This tells askama's build script to look for template files in `src/report/templates/` relative to the crate manifest directory, matching the architecture's directory structure. Without this, askama defaults to `templates/` at the crate root.

- [x] **T3. Create `src/report/templates/report.html`** (AC: 2, 4)
  - [x] T3.1 Create directory `src/report/templates/` (not a Rust module — contains only askama template files).
  - [x] T3.2 Create `src/report/templates/report.html`:
    ```html
    <!DOCTYPE html>
    <html lang="en">
    <head>
    <meta charset="utf-8">
    <title>lcrc report</title>
    <style>
    body{font-family:monospace;margin:2em}
    table{border-collapse:collapse}
    th,td{border:1px solid #ccc;padding:.4em .8em}
    th{background:#f0f0f0}
    .pass{color:green}
    .fail{color:red}
    </style>
    </head>
    <body>
    <h1>lcrc report</h1>
    <table>
    <thead>
    <tr><th>Model</th><th>Task</th><th>Result</th><th>Scanned</th></tr>
    </thead>
    <tbody>
    <tr>
    <td>{{ model_ident }}</td>
    <td>{{ task_id }}</td>
    <td class="{% if pass %}pass{% else %}fail{% endif %}">{% if pass %}PASS{% else %}FAIL{% endif %}</td>
    <td>{{ scan_timestamp }}</td>
    </tr>
    </tbody>
    </table>
    </body>
    </html>
    ```
    **CSS is inlined** — no external stylesheet, font, or script references (AC4). `{{ }}` expressions are HTML-escaped by askama by default in `.html` templates. `{% if %}`/`{% else %}`/`{% endif %}` are Jinja2-style control blocks.

- [x] **T4. Create `src/report.rs`** (AC: 1, 2, 3, 4, 5)
  - [x] T4.1 Add file-level doc:
    ```rust
    //! HTML report renderer.
    //!
    //! `render_html()` generates a self-contained `latest.html` under the
    //! report directory using an askama compile-time template. The write is
    //! atomic: temp file written then renamed, so a crash mid-write leaves
    //! the previous `latest.html` intact.
    ```
  - [x] T4.2 Define `ReportTemplate` with lifetime annotation (borrows from `&Cell`):
    ```rust
    use askama::Template;
    use crate::cache::cell::Cell;
    use std::path::Path;

    #[derive(Template)]
    #[template(path = "report.html")]
    struct ReportTemplate<'a> {
        model_ident: &'a str,
        task_id: &'a str,
        pass: bool,
        scan_timestamp: &'a str,
    }

    impl<'a> From<&'a Cell> for ReportTemplate<'a> {
        fn from(cell: &'a Cell) -> Self {
            let sha = &cell.key.model_sha;
            Self {
                model_ident: &sha[..sha.len().min(8)],
                task_id: &cell.key.task_id,
                pass: cell.pass,
                scan_timestamp: &cell.scan_timestamp,
            }
        }
    }
    ```
    **`model_ident` is the first 8 hex chars of `model_sha`** — Epic 1 has no model name; full model discovery lands in Story 2.1. `.min(8)` guards the slice in tests that use short SHA strings.

  - [x] T4.3 Implement the public `render_string` function (pure, synchronous, used by tests):
    ```rust
    /// Render the HTML string for a single cell row without performing any I/O.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the askama template engine fails to render (unlikely
    /// for a static template; surfaces as `Error::Other` at call sites).
    pub fn render_string(cell: &Cell) -> Result<String, askama::Error> {
        use askama::Template as _;
        ReportTemplate::from(cell).render()
    }
    ```

  - [x] T4.4 Implement private `write_report_atomic`:
    ```rust
    /// Write `html` to `path` via temp file + atomic rename.
    ///
    /// The temp file is `path` with extension replaced by `html.tmp`. If the
    /// process is killed between the write and rename, the caller's next
    /// invocation overwrites the same temp file and retries the rename —
    /// `latest.html` is never left in a half-written state.
    async fn write_report_atomic(path: &Path, html: &str) -> Result<(), std::io::Error> {
        let tmp = path.with_extension("html.tmp");
        tokio::fs::write(&tmp, html.as_bytes()).await?;

        // Set 0o644 before rename so the final file inherits the intended
        // permissions atomically regardless of the process umask.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            tokio::fs::set_permissions(
                &tmp,
                std::fs::Permissions::from_mode(0o644),
            )
            .await?;
        }

        tokio::fs::rename(&tmp, path).await
    }
    ```

  - [x] T4.5 Implement the public `render_html` entry point:
    ```rust
    /// Render and atomically write `latest.html` to `report_dir`.
    ///
    /// Creates `report_dir` if it does not exist. The output file is
    /// `report_dir/latest.html`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Other`] if the template fails to render,
    /// the directory cannot be created, or the atomic file write fails.
    pub async fn render_html(cell: &Cell, report_dir: &Path) -> Result<(), crate::error::Error> {
        tokio::fs::create_dir_all(report_dir)
            .await
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("create report dir: {e}")))?;

        let html = render_string(cell)
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("template render: {e}")))?;

        let latest = report_dir.join("latest.html");
        write_report_atomic(&latest, &html)
            .await
            .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("write report: {e}")))?;

        tracing::info!(
            target: "lcrc::report",
            path = %latest.display(),
            "HTML report written",
        );
        crate::output::diag(&format!("lcrc scan: report → {}", latest.display()));
        Ok(())
    }
    ```

- [x] **T5. Update `src/scan/orchestrator.rs::measure_and_persist`** (AC: 1, 2, 3)
  - [x] T5.1 In `measure_and_persist`, compute `report_dir` from `db_path` **before** `db_path` is moved into the `spawn_blocking` closure (it's moved via `let p = db_path;`). Insert before the `// Write cell atomically.` comment:
    ```rust
    // Derive the report dir from the same data-dir parent as lcrc.db.
    // db_path = data_dir/lcrc/lcrc.db → parent = data_dir/lcrc → reports = data_dir/lcrc/reports.
    let report_dir = db_path
        .parent()
        .ok_or_else(|| {
            crate::error::Error::Preflight("db_path has no parent directory".into())
        })?
        .join("reports");
    ```

  - [x] T5.2 Clone `cell` before it is moved into the `spawn_blocking` closure, so it is available for the report step after the write. Insert immediately before `// Write cell atomically.`:
    ```rust
    let cell_for_report = cell.clone();
    ```

  - [x] T5.3 After the `write_cell` `spawn_blocking` block succeeds (after the `}` closing the atomic write block, before `crate::output::diag("lcrc scan: done…")`), add:
    ```rust
    crate::report::render_html(&cell_for_report, &report_dir)
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("render report: {e}")))?;
    ```

  - [x] T5.4 Verify that `db_path` is still consumed correctly after T5.1 (the `let p = db_path;` line remains; no change needed to the spawn_blocking body). `cell` is consumed by `spawn_blocking`; `cell_for_report` (the clone) is consumed by `render_html`. Both bindings disappear cleanly at end of scope. The `#[allow(clippy::too_many_lines)]` on `run()` may need extending to `measure_and_persist` if clippy triggers.

- [x] **T6. Integration test `tests/report_render.rs`** (AC: 1, 2, 3, 4, 5)
  - [x] T6.1 Add file-level doc:
    ```rust
    //! Integration tests for the HTML report renderer (FR32, FR33).
    ```
  - [x] T6.2 Implement helper `synthetic_cell()` — mirrors the pattern from `src/cache/cell.rs`:
    ```rust
    use lcrc::cache::cell::{Cell, CellKey};

    fn synthetic_cell(pass: bool) -> Cell {
        Cell {
            key: CellKey {
                machine_fingerprint: "M1Pro-32GB-14gpu".into(),
                model_sha: "abcdef01".repeat(8), // 64-char hex
                backend_build: "llama.cpp-b3791+a1b2c3d".into(),
                params_hash: "1".repeat(64),
                task_id: "swe-bench-pro:canary".into(),
                harness_version: "mini-swe-agent-0.1.0".into(),
                task_subset_version: "0.0.1-canary-only".into(),
            },
            container_image_id: "sha256:deadbeef".into(),
            lcrc_version: "0.0.1".into(),
            depth_tier: "quick".into(),
            scan_timestamp: "2026-05-07T00:00:00.000Z".into(),
            pass,
            duration_seconds: Some(12.5),
            tokens_per_sec: None,
            ttft_seconds: None,
            peak_rss_bytes: None,
            power_watts: None,
            thermal_state: None,
            badges: vec![],
        }
    }
    ```
  - [x] T6.3 Snapshot test for pass cell (uses `insta`):
    ```rust
    #[test]
    fn report_snapshot_pass_cell() {
        let cell = synthetic_cell(true);
        let html = lcrc::report::render_string(&cell).unwrap();
        insta::assert_snapshot!(html);
    }
    ```
  - [x] T6.4 Snapshot test for fail cell:
    ```rust
    #[test]
    fn report_snapshot_fail_cell() {
        let cell = synthetic_cell(false);
        let html = lcrc::report::render_string(&cell).unwrap();
        insta::assert_snapshot!(html);
    }
    ```
  - [x] T6.5 Async atomic-write test (verifies AC3: temp file → rename):
    ```rust
    #[tokio::test(flavor = "current_thread")]
    async fn report_atomic_write_creates_latest_html() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let cell = synthetic_cell(true);
        lcrc::report::render_html(&cell, dir.path()).await.unwrap();
        let path = dir.path().join("latest.html");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("PASS"), "expected PASS in report: {content}");
        assert!(content.contains("swe-bench-pro:canary"), "expected task_id: {content}");
        assert!(content.contains("2026-05-07"), "expected timestamp: {content}");
        // No temp file left behind
        assert!(!dir.path().join("latest.html.tmp").exists());
    }
    ```
  - [x] T6.6 File-permission test (AC5 — unix only):
    ```rust
    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn report_html_has_correct_permissions() {
        use std::os::unix::fs::PermissionsExt as _;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let cell = synthetic_cell(true);
        lcrc::report::render_html(&cell, dir.path()).await.unwrap();
        let meta = std::fs::metadata(dir.path().join("latest.html")).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "expected 0o644, got {mode:o}");
    }
    ```
  - [x] T6.7 Run `cargo insta review` after first run to accept snapshots. Committed `.snap` files go in `tests/snapshots/`.

- [x] **T7. Local CI mirror** (AC: all)
  - [x] T7.1 `cargo build` — new module and template compile; askama build script finds template in `src/report/templates/report.html`.
  - [x] T7.2 `cargo fmt --check` — rustfmt clean.
  - [x] T7.3 `cargo clippy --all-targets --all-features -- -D warnings`. Watch for:
    - `missing_docs` on every `pub` item in `src/report.rs`.
    - `clippy::module_name_repetitions` — not expected here, but check.
    - `clippy::too_many_lines` on `measure_and_persist` if the added lines push it over 100.
    - Any lint on `#[cfg(unix)]` blocks in tests (should be fine).
  - [x] T7.4 `cargo test` — all 140+ pre-existing tests pass. New snapshot tests in `tests/report_render.rs` pass after `cargo insta review`.
  - [x] T7.5 Scope check — confirm `src/lib.rs` now lists `pub mod report;`:
    ```bash
    grep "pub mod report" src/lib.rs
    ```

## Dev Notes

### Scope discipline (read this first)

This story adds the HTML rendering layer called by Story 1.12's orchestrator immediately after `write_cell` succeeds.

**This story DOES:**
- Create `src/report.rs` with `render_html()`, `render_string()`, and `write_report_atomic()`
- Create `src/report/templates/report.html` (minimal one-row template)
- Configure askama template directory in `Cargo.toml`
- Add `pub mod report;` to `src/lib.rs`
- Update `measure_and_persist` in `src/scan/orchestrator.rs` to call `render_html` after `write_cell`
- Create `tests/report_render.rs` with snapshot and I/O tests

**This story does NOT:**
- Create `src/report/badges.rs` — Badge enum (Story 2.4+)
- Create `src/report/wilson.rs` — Wilson CI math (Story 2.4+)
- Create `src/report/header.rs` — canonical header (Story 2.12+)
- Create `src/report/templates/header.html` or `row.html` — Epic 2 templates
- Implement streaming / per-cell ETA (Story 2.13)
- Implement `lcrc show` plain-text mirror (Story 4.1)
- Implement JSON output (Story 4.4)
- Implement Wilson CIs, depth badges, or any badge rendering

### askama 0.12 API

```rust
use askama::Template;

#[derive(Template)]
#[template(path = "report.html")]  // relative to configured dirs
struct MyTemplate { field: String }

// render() returns Result<String, askama::Error>
let html = MyTemplate { field: "hello".into() }.render()?;
```

**Template syntax (Jinja2-like):**
- `{{ variable }}` — HTML-escaped expression output
- `{% if condition %}...{% else %}...{% endif %}` — conditional blocks
- `.html` files get HTML auto-escaping by default; no `escape = "html"` annotation needed

**Template directory configuration** in `Cargo.toml`:
```toml
[package.metadata.askama]
dirs = ["src/report/templates"]
```
This path is relative to `CARGO_MANIFEST_DIR` (the crate root). The `path = "report.html"` in `#[template]` is then relative to this configured dir.

**askama is a build-time dependency**: template compilation happens during `cargo build`. Errors in the template (syntax, missing fields) surface as compile errors, not runtime panics.

**`askama::Error` does not implement `std::error::Error`** in all versions; use `anyhow::anyhow!("template render: {e:?}")` when mapping to `Error::Other` rather than `{e}`.

### Atomic write pattern (AR-30)

From the architecture:
```rust
pub async fn write_report(path: &Path, html: &str) -> Result<()> {
    let tmp = path.with_extension("html.tmp");
    tokio::fs::write(&tmp, html).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}
```

The temp file name is `latest.html.tmp` (adjacent to `latest.html`). Both files are on the same filesystem, making `rename` atomic at the OS level. If the process is killed between `write` and `rename`, the next scan run overwrites `latest.html.tmp` and completes the rename — `latest.html` is never partially written.

**Set permissions on the temp file before rename** (T4.4 / AC5) so the final file atomically appears with 0o644 permissions, independent of the process umask.

### Orchestrator integration point

In `src/scan/orchestrator.rs`, the change touches `measure_and_persist` (not `run()`). The exact insertion point is after the `spawn_blocking` block that calls `write_cell`:

```
// current code (around line 232-248)
{
    let p = db_path;   ← db_path consumed here
    tokio::task::spawn_blocking(move || {
        let mut cache = crate::cache::cell::Cache::open(&p)?;
        cache.write_cell(&cell)  ← cell consumed here
    }).await...?;
}

// INSERT BEFORE the spawn_blocking block:
let report_dir = db_path.parent()
    .ok_or_else(|| ...)?
    .join("reports");
let cell_for_report = cell.clone();

// AFTER the spawn_blocking block (after the closing } and map_err):
crate::report::render_html(&cell_for_report, &report_dir)
    .await
    .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("render report: {e}")))?;
```

**`db_path` path hierarchy:**
- `db_path` = `data_dir/lcrc/lcrc.db`
- `db_path.parent()` = `data_dir/lcrc`
- `report_dir` = `data_dir/lcrc/reports`
- `latest.html` = `data_dir/lcrc/reports/latest.html`

On macOS with etcetera's default strategy, `data_dir` = `~/Library/Application Support`.

### rusqlite + `cell.clone()` notes

`Cell` derives `Clone` (`#[derive(Debug, Clone, PartialEq)]`). Cloning is cheap: 7 strings in `CellKey` plus a handful of `Option<f64>` and `Vec<String>`. No heap-heavy data.

`cell` is moved into the `spawn_blocking` closure. The clone (`cell_for_report`) must be created **before** that move, i.e., before `let p = db_path;` and the `move` closure that captures `cell`.

### `render_string` vs `render_html` distinction

- `render_string(cell: &Cell) -> Result<String, askama::Error>` — pure synchronous; called by unit/snapshot tests without a runtime.
- `render_html(cell: &Cell, report_dir: &Path) -> Result<(), crate::error::Error>` — async; creates dir, calls `render_string`, calls `write_report_atomic`. Called by the orchestrator.

Keeping them separate makes the snapshot tests dead simple and avoids needing `tokio::test` in most tests.

### Testing with `insta`

`insta = { version = "1", features = ["yaml"] }` is already in `[dev-dependencies]`.

Workflow:
1. Run `cargo test` — insta will write pending snapshot files to `tests/snapshots/` (or fail with "snapshot not yet accepted").
2. Run `cargo insta review` — inspect and accept snapshots.
3. Commit the `.snap` files.

The snapshot captures the **exact rendered HTML string**. If the template changes, `cargo test` will fail until `cargo insta review` is re-run. This is the intended behaviour per the architecture (FR32–FR36 HTML rendering).

**Snapshot filename convention**: insta generates names from the test function name: `tests/snapshots/report_render__report_snapshot_pass_cell.snap`.

### File permissions cross-platform

`#[cfg(unix)]` wraps the `set_permissions` call in `write_report_atomic` and the permission assertion in the test. The project targets macOS Apple Silicon, so this is always active in practice. The cfg guard prevents compile errors if someone ever builds on Windows.

`std::os::unix::fs::PermissionsExt` must be imported as a trait (`use ... as _`) since only its methods are needed, not the type name.

### File structure

```
Cargo.toml                               MODIFIED — add [package.metadata.askama]
src/
├── lib.rs                               MODIFIED — add `pub mod report;`
├── report.rs                            NEW: render_html(), render_string(), ReportTemplate
├── report/
│   └── templates/
│       └── report.html                  NEW: askama one-row HTML template
└── scan/
    └── orchestrator.rs                  MODIFIED — call render_html after write_cell

tests/
└── report_render.rs                     NEW: snapshot + I/O + permission tests
tests/snapshots/                         NEW (insta-managed):
├── report_render__report_snapshot_pass_cell.snap
└── report_render__report_snapshot_fail_cell.snap
```

Note: `src/report/templates/` is NOT a Rust module directory — no `mod.rs` or Rust source files. It contains only askama `.html` template files accessed by the build script.

### Cross-story interaction

- **Depends on**: Story 1.12 (`Cell`, `CellKey`, `Cache::write_cell`, orchestrator pipeline, `crate::util::rfc3339_now`, `crate::output::diag`).
- **Unblocks**: Story 1.14 (container image that makes the integration test pass end-to-end); Story 2.4 (badge rendering — adds to `src/report/badges.rs`).
- **Epic 2 callers**: Story 2.4 will add the Badge enum at `src/report/badges.rs` and update the template. No changes to `render_html`'s signature — the badge list is already in `cell.badges` (always `vec![]` in Epic 1).

### Previous story learnings (from Story 1.12 dev notes and review)

- **Planning artifact refs in comments**: Do not reference story numbers, AC codes, or planning artifact paths in source code comments. This was flagged in the Story 1.12 review.
- **`spawn_blocking` pattern**: All `Cache` operations use `spawn_blocking`. `render_html` is pure async (tokio I/O only), so no `spawn_blocking` needed in the report module.
- **`crate::output::diag`** for user-visible progress lines — no `println!` or `eprintln!` in new modules (FR46 stdout/stderr discipline).
- **`missing_docs = "warn"`** — every `pub` item needs a `///` doc comment.
- **Tracing discipline**: new events use `target: "lcrc::report"` with structured fields.
- **Drop order**: `cell_for_report` (used in `render_html`) must remain alive until after the `write_cell` block. Since it's a separate binding (clone), it naturally outlives the moved `cell`. No special handling needed.
- **`etcetera::BaseStrategy as _`** trait import is already `use`d in `orchestrator.rs` for `data_dir()` — Story 1.13 does NOT call etcetera in `report.rs`; the report dir is passed in as a `&Path`, keeping the report module free of etcetera dependency.

### Project context

- **Epic 1 position**: Story 13 of 14. Story 1.14 (container image publish) is the final one.
- **NFR coverage**: NFR-R2 (atomic writes — tempfile + rename); FR32 (self-contained HTML); FR33 (regenerated per-cell); FR39 (default path only — no `--report-dir` flag yet).
- **No new crate dependencies** — `askama` is already in `Cargo.toml`. `tempfile` and `tokio` already present.

### References

- [Source: `_bmad-output/planning-artifacts/epics.md` § "Story 1.13"] — five AC clauses and user story
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "HTML report (FR32, FR33, FR34)"] — askama choice, self-contained requirement
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "HTML report: tempfile + atomic rename" (lines 707–715)] — canonical `write_report` pattern
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Complete Project Directory Structure" (lines 950–958)] — `src/report.rs`, `src/report/templates/`
- [Source: `_bmad-output/planning-artifacts/architecture.md` § "Architectural Boundaries"] — stdout/stderr sole module; badge enum sole module
- [Source: `_bmad-output/implementation-artifacts/1-12-end-to-end-one-cell-scan-no-html-yet.md` § "Dev Agent Record / Completion Notes"] — orchestrator call site confirmed; Story 1.13 adds render_html after write_cell
- [Source: `src/scan/orchestrator.rs` lines 158–264] — `measure_and_persist` function; insertion points for T5
- [Source: `src/cache/cell.rs`] — `Cell`, `CellKey` types; `Cell` derives `Clone`
- [Source: `src/lib.rs`] — module declarations to extend
- [Source: `src/output.rs`] — `crate::output::diag` for user-visible progress
- [Source: `src/error.rs`] — `Error::Other(anyhow::anyhow!(...))` wrapping pattern
- [Source: `Cargo.toml`] — `askama = "0.12"`, `insta = { version = "1", features = ["yaml"] }`, `tempfile = "3"`, `tokio` full features

## Dev Agent Record

### Agent Model Used

claude-sonnet-4-6[1m]

### Debug Log References

### Completion Notes List

- Implemented `src/report.rs` with `render_string` (pure), `render_html` (async), and private `write_report_atomic` using tokio atomic write + rename pattern.
- Used `askama.toml` (crate root) for template directory config — `[package.metadata.askama]` in `Cargo.toml` is not picked up by askama 0.12; `askama.toml` with `[general] dirs = ["src/report/templates"]` is required instead. Both are present.
- `model_ident` is the first 8 hex chars of `model_sha` using `.min(8)` slice guard.
- Template uses Jinja2-style `{% if pass %}` with inline CSS — fully self-contained, no external resources.
- `write_report_atomic` sets 0o644 permissions on the temp file before rename so the final `latest.html` atomically appears with correct mode regardless of process umask.
- Integration test file suppresses `clippy::unwrap_used` at file level (matches project pattern for integration test files).
- 144 tests pass (4 new in `tests/report_render.rs`: 2 snapshot, 1 atomic-write I/O, 1 permission check); 0 regressions.
- Snapshot files accepted and committed to `tests/snapshots/`.

### File List

- `Cargo.toml` — added `[package.metadata.askama]` section
- `askama.toml` — new; `[general] dirs = ["src/report/templates"]` for askama build script
- `src/lib.rs` — added `pub mod report;`
- `src/report.rs` — new: `render_html`, `render_string`, `write_report_atomic`, `ReportTemplate`
- `src/report/templates/report.html` — new: askama one-row HTML template
- `src/scan/orchestrator.rs` — added `report_dir` derivation, `cell_for_report` clone, and `render_html` call after `write_cell`
- `tests/report_render.rs` — new: snapshot + atomic-write + permissions integration tests
- `tests/snapshots/report_render__report_snapshot_pass_cell.snap` — new: insta snapshot
- `tests/snapshots/report_render__report_snapshot_fail_cell.snap` — new: insta snapshot
