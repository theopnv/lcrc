//! HTML report renderer.
//!
//! `render_html()` generates a self-contained `latest.html` under the
//! report directory using an askama compile-time template. The write is
//! atomic: temp file written then renamed, so a crash mid-write leaves
//! the previous `latest.html` intact.

use askama::Template;
use std::path::Path;

use crate::cache::cell::Cell;

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
        tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).await?;
    }

    tokio::fs::rename(&tmp, path).await
}

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
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("template render: {e:?}")))?;

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
