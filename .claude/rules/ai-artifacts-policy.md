# AI artifact policy

`http-playback-proxy llm` prints a reference assembled from `llmdocs/`,
embedded at compile time with `include_str!`.

## Never hand-edit

| File | Produced by |
| --- | --- |
| `llmdocs/90-commands.md` | `cargo run --bin http-playback-proxy -- llm --regenerate` (walks the clap model) |

## Source of truth

| To change… | Edit |
| --- | --- |
| ground rules, the recording/playback contract, failure modes | `llmdocs/00-guide.md` |
| a command or flag description | the clap definitions in `src/cli.rs` |
| what the distributed skills tell an agent | `plugins/rust-http-playback-proxy/skills/*/SKILL.md` |
| pitfalls surfaced through context7 | `context7.json` `rules` |

## This is the only Rust CLI in the family

`go-llm-cli-kit` is Go-only, so three pieces are reimplemented here and have
no upstream to inherit fixes from:

- `src/llm.rs` — the renderer and the `llm` output contract
- `src/llmgen.rs` — the clap equivalent of the kit's cobra catalog generator
- `tests/plugin_skills.rs` — the skillcheck equivalent

If the standard changes, these three have to be updated by hand. Keep the
output contract identical to the Go CLIs: Markdown by default, and
`--format json` yielding an array of `{file, title, body}`.

Because chapters are embedded with `include_str!`, `llmdocs/90-commands.md`
must exist at build time — that is why it is committed rather than gitignored,
and why CI regenerates it and fails on a dirty tree.
