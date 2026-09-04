//! The deterministic human projection of diagnostics (issue #11).
//!
//! One item per line: status, severity, `[LEK-CODE]`, dotted id, optional
//! `path:start-line:start-column`, then the rendered message. Related
//! locations, causes, and fixes render as fixed-indent children in that
//! order. No ANSI escapes exist in v1.

use super::Diagnostic;

/// Render one diagnostic with its envelope status as human text.
pub fn diagnostic_lines(status: &str, diagnostic: &Diagnostic) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = format!(
        "{status} {} [{}] {}",
        diagnostic.severity.as_str(),
        diagnostic.code(),
        diagnostic.id()
    );
    if let (Some(path), Some(start)) = (diagnostic.path(), diagnostic.start()) {
        line.push_str(&format!(" {path}:{}:{}", start.line, start.column));
    }
    line.push_str(": ");
    line.push_str(diagnostic.message());
    lines.push(line);
    for related in &diagnostic.related_locations {
        let location = &related.location;
        let mut line = format!(
            "  related {} {}",
            related.relation.as_str(),
            location
                .path
                .as_deref()
                .map(|path| path.to_owned())
                .unwrap_or_else(|| "<global>".to_owned())
        );
        if let Some(range) = location.range {
            line.push_str(&format!(":{}:{}", range.start.line, range.start.column));
        }
        lines.push(line);
    }
    for cause in &diagnostic.causes {
        lines.push(format!(
            "  cause [{}] {}: {}",
            cause.code.as_str(),
            cause.id.as_str(),
            cause.message
        ));
    }
    for fix in &diagnostic.fixes {
        lines.push(format!(
            "  fix {} {}: {}",
            fix.applicability.as_str(),
            fix.fix_id.as_str(),
            fix.message
        ));
    }
    lines
}
