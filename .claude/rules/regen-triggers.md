---
paths:
  - "src/cli.rs"
  - "src/llm.rs"
  - "src/llmgen.rs"
  - "llmdocs/0*.md"
  - "plugins/rust-http-playback-proxy/**"
  - "context7.json"
---

# Regen triggers

Before committing:

1. `cargo run --bin http-playback-proxy -- llm --regenerate` and commit the
   result. `--bin` is required: the crate builds two binaries.
2. Do not hand-edit `llmdocs/90-commands.md`.
3. If you changed a skill description, `cargo test --test plugin_skills` —
   it asserts the discovery keywords are still present.
4. Bumping the crate version means bumping
   `plugins/rust-http-playback-proxy/.claude-plugin/plugin.json` too; the test
   and the release workflow both check they agree.
