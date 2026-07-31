//! The reference this binary carries for AI agents.
//!
//! `go-llm-cli-kit` is Go-only, so the four-outlet standard is implemented
//! here by hand. The contract is the same as every other ideamans CLI:
//! `http-playback-proxy llm` prints Markdown, `--format json` prints an array
//! of chapters with `file`, `title` and `body`.
//!
//! Chapters are embedded at compile time with `include_str!`, so the output
//! always matches the running binary. `90-commands.md` is generated from the
//! clap definition by `cargo run -- llm --regenerate`; the rest are written
//! by hand.

use serde::Serialize;

#[derive(Serialize)]
pub struct Section {
    pub file: String,
    pub title: String,
    pub body: String,
}

/// Chapters in reading order. The numeric filename prefix is the order, the
/// same convention the Go CLIs use.
const CHAPTERS: &[(&str, &str)] = &[
    ("00-guide.md", include_str!("../llmdocs/00-guide.md")),
    ("90-commands.md", include_str!("../llmdocs/90-commands.md")),
];

fn title_of(body: &str) -> String {
    body.lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").trim().to_string())
        .unwrap_or_default()
}

pub fn sections() -> Vec<Section> {
    CHAPTERS
        .iter()
        .map(|(file, body)| Section {
            file: (*file).to_string(),
            title: title_of(body),
            body: body.trim_end().to_string(),
        })
        .collect()
}

pub fn markdown() -> String {
    let mut out = String::new();
    for (i, s) in sections().iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&s.body);
    }
    out.push('\n');
    out
}

pub fn json() -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(&sections())?)
}

/// Renders the reference in the requested format.
pub fn render(format: &str) -> anyhow::Result<String> {
    match format.to_ascii_lowercase().as_str() {
        "" | "markdown" | "md" => Ok(markdown()),
        "json" => json(),
        other => anyhow::bail!("unknown format {other:?}: use markdown or json"),
    }
}
