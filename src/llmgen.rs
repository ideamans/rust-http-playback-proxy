//! Generates `llmdocs/90-commands.md` from the clap definition.
//!
//! The Go CLIs get this from go-llm-cli-kit's catalog package. There is no
//! Rust equivalent, so this walks clap's own `Command` model. Output is
//! sorted so the committed file is deterministic and a diff only ever shows a
//! real change.
//!
//! Run with `cargo run --bin http-playback-proxy -- llm --regenerate`, and commit the result — the
//! chapters are embedded with `include_str!`, so the file has to exist at
//! build time. CI regenerates and fails on a dirty tree.

use clap::{Command, CommandFactory};
use std::fmt::Write as _;

use crate::cli::Cli;

const OUT_PATH: &str = "llmdocs/90-commands.md";

pub fn regenerate() -> anyhow::Result<()> {
    let cmd = Cli::command();
    let mut out = String::new();
    out.push_str("# Command catalog\n\n");
    out.push_str("Generated from the clap definition by `cargo run --bin http-playback-proxy -- llm --regenerate`.\n");
    out.push_str("Do not edit by hand — edit the definitions in `src/cli.rs` instead.\n");

    let mut subs: Vec<&Command> = cmd.get_subcommands().collect();
    subs.sort_by_key(|c| c.get_name().to_string());
    for sub in subs {
        write_command(&mut out, "http-playback-proxy", sub)?;
    }

    std::fs::write(OUT_PATH, &out)?;
    eprintln!("wrote {OUT_PATH} ({} bytes)", out.len());
    Ok(())
}

fn write_command(out: &mut String, prefix: &str, cmd: &Command) -> anyhow::Result<()> {
    // `llm` documents itself in 00-guide.md; listing it here wastes context.
    if cmd.is_hide_set() || cmd.get_name() == "llm" {
        return Ok(());
    }
    let full = format!("{prefix} {}", cmd.get_name());
    write!(out, "\n## `{full}`\n\n")?;
    if let Some(about) = cmd.get_about() {
        writeln!(out, "{about}")?;
    }

    let positionals: Vec<String> = cmd
        .get_positionals()
        .map(|a| {
            let n = a.get_id().as_str();
            if a.is_required_set() {
                format!("<{n}>")
            } else {
                format!("[{n}]")
            }
        })
        .collect();
    if !positionals.is_empty() {
        write!(out, "\n```\n{full} {}\n```\n", positionals.join(" "))?;
    }

    let mut opts: Vec<_> = cmd.get_opts().filter(|a| !a.is_hide_set()).collect();
    opts.sort_by_key(|a| a.get_id().as_str().to_string());
    if !opts.is_empty() {
        out.push_str("\n| flag | default | description |\n| --- | --- | --- |\n");
        for a in opts {
            let mut names = Vec::new();
            if let Some(s) = a.get_short() {
                names.push(format!("`-{s}`"));
            }
            if let Some(l) = a.get_long() {
                names.push(format!("`--{l}`"));
            }
            let default = a
                .get_default_values()
                .first()
                .map(|d| format!("`{}`", d.to_string_lossy()))
                .unwrap_or_else(|| "—".to_string());
            let help = a
                .get_help()
                .map(|h| h.to_string().replace('|', "\\|"))
                .unwrap_or_default();
            let required = if a.is_required_set() {
                " **(required)**"
            } else {
                ""
            };
            writeln!(
                out,
                "| {} | {default} | {help}{required} |",
                names.join(", ")
            )?;
        }
    }

    let mut subs: Vec<&Command> = cmd.get_subcommands().collect();
    subs.sort_by_key(|c| c.get_name().to_string());
    for sub in subs {
        write_command(out, &full, sub)?;
    }
    Ok(())
}
