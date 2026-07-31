//! Validates the distributed Claude Code plugin.
//!
//! go-llm-cli-kit's `skillcheck` is Go-only, so the same checks are
//! reimplemented here: the manifest version must track the crate version, the
//! SKILL.md frontmatter must stay within the Agent Skills standard (the same
//! files are installed into Copilot, Cursor and Gemini CLI via `gh skill`),
//! and the descriptions must still carry the terms a user says when they need
//! this tool.

use std::fs;
use std::path::Path;

const PLUGIN_DIR: &str = "plugins/rust-http-playback-proxy";

/// The Agent Skills standard frontmatter. Claude-only keys such as
/// argument-hint or model are rejected: other agents ignore them, and a
/// distributed skill has to read everywhere.
const ALLOWED_KEYS: &[&str] = &[
    "name",
    "description",
    "license",
    "compatibility",
    "metadata",
    "allowed-tools",
];

/// Terms a user is likely to say when they need this tool. Changing a
/// description without updating this list is exactly the silent regression
/// this test exists to catch.
const KEYWORDS: &[&str] = &["proxy", "record", "install"];

fn frontmatter(body: &str) -> Vec<(String, String)> {
    let mut lines = body.lines();
    assert_eq!(lines.next(), Some("---"), "SKILL.md must open with ---");
    let mut out = Vec::new();
    for line in lines {
        if line == "---" {
            return out;
        }
        if let Some((k, v)) = line.split_once(':') {
            if !k.starts_with(char::is_whitespace) {
                out.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    panic!("SKILL.md frontmatter is not terminated by ---");
}

#[test]
fn plugin_manifest_version_matches_crate() {
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(Path::new(PLUGIN_DIR).join(".claude-plugin/plugin.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        manifest["version"].as_str().unwrap(),
        env!("CARGO_PKG_VERSION"),
        "plugin.json version must match the crate version in Cargo.toml"
    );
    assert_eq!(manifest["name"].as_str().unwrap(), "rust-http-playback-proxy");
}

#[test]
fn skills_follow_the_agent_skills_standard() {
    let skills_dir = Path::new(PLUGIN_DIR).join("skills");
    let mut seen = 0;
    let mut has_install = false;
    let mut all_descriptions = String::new();

    for entry in fs::read_dir(&skills_dir).expect("skills directory") {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let dir_name = dir.file_name().unwrap().to_string_lossy().to_string();
        let body = fs::read_to_string(dir.join("SKILL.md"))
            .unwrap_or_else(|e| panic!("{dir_name}/SKILL.md: {e}"));
        let fm = frontmatter(&body);

        for (k, _) in &fm {
            assert!(
                ALLOWED_KEYS.contains(&k.as_str()),
                "{dir_name}: frontmatter key {k:?} is outside the Agent Skills standard"
            );
        }
        let get = |key: &str| {
            fm.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let name = get("name");
        assert_eq!(name, dir_name, "name must match the directory name");
        assert!(!get("description").is_empty(), "{dir_name}: description is required");
        assert!(
            !get("compatibility").is_empty(),
            "{dir_name}: compatibility should state what the skill needs"
        );

        if name.ends_with("-install") {
            has_install = true;
        }
        all_descriptions.push_str(&get("description").to_lowercase());
        seen += 1;
    }

    assert!(seen >= 2, "expected at least a usage and an install skill, found {seen}");
    assert!(
        has_install,
        "no *-install skill: users whose PATH lacks the CLI have no way forward"
    );
    for kw in KEYWORDS {
        assert!(
            all_descriptions.contains(kw),
            "no skill description mentions {kw:?} — discovery would silently regress"
        );
    }
}
