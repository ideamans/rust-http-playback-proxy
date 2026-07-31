---
name: regen-ai
description: Regenerate the embedded command catalog for http-playback-proxy and verify it still builds and passes the plugin checks.
---

# regen-ai

```bash
cargo run --bin http-playback-proxy -- llm --regenerate
git diff --stat -- llmdocs
cargo test --test plugin_skills
cargo run --bin http-playback-proxy -- llm | head -5
```

`--bin` is required — the crate builds two binaries. Report what changed in
`llmdocs/`, and whether the tests pass.
