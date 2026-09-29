//! Issue #53 canonicalizer: validates and rewrites the browser-E2E
//! scenario documents of the planning screen into canonical bytes.
//! Run from the repository root:
//!
//!     cargo run -p lekalo-core --example canonicalize-screen-scenarios -- .
//!
//! For every `*.scenario.json` under the routes model home's
//! `lekalo/scenarios/`, parses through the closed Scenario IR frontend,
//! re-renders the canonical bytes, and rewrites the file (plus one
//! trailing newline). A document that fails to parse or is not
//! already canonically ordered is refused, never silently rewritten.

use lekalo_core::scenario::ScenarioIr;

fn main() {
    let root = std::env::args().nth(1).expect("repo root");
    let dir = format!("{root}/tests/fixtures/php-laravel/routes/model/lekalo/scenarios");
    for entry in std::fs::read_dir(&dir).expect("scenario dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::read(&path).expect("read");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let ir = ScenarioIr::from_value(&value).unwrap_or_else(|set| {
            panic!(
                "{name}: {}",
                serde_json::to_string(&set).unwrap_or_default()
            )
        });
        let canonical = ir.canonical_bytes().expect("canonical bytes");
        let on_disk = String::from_utf8(bytes).expect("utf8");
        let on_disk_trimmed = on_disk.trim_end_matches('\n').to_string();
        if on_disk_trimmed != canonical {
            std::fs::write(&path, format!("{canonical}\n")).expect("rewrite");
            println!("{name}: canonicalized");
        } else {
            println!("{name}: already canonical");
        }
    }
}
