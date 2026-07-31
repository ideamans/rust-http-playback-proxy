# Command catalog

Generated from the clap definition by `cargo run --bin http-playback-proxy -- llm --regenerate`.
Do not edit by hand — edit the definitions in `src/cli.rs` instead.

## `http-playback-proxy playback`

Playback recorded HTTP traffic

| flag | default | description |
| --- | --- | --- |
| `--full-throttle` | `false` | Disable timing control (TTFB and transfer speed) for fastest playback |
| `-i`, `--inventory` | `./inventory` | Inventory directory |
| `--passthrough` | `false` | Forward unmatched requests to real servers instead of returning 404 |
| `-p`, `--port` | — | Port to use for the proxy server (default: auto-detect from 18080) |

## `http-playback-proxy recording`

Record HTTP traffic

```
http-playback-proxy recording [entry_url]
```

| flag | default | description |
| --- | --- | --- |
| `-d`, `--device` | `mobile` | Device type |
| `-x`, `--exclude` | — | Regex patterns to exclude URLs from recording (can be specified multiple times) |
| `-e`, `--extra-url` | — | Additional entry URLs (can be specified multiple times) |
| `-i`, `--inventory` | `./inventory` | Inventory directory |
| `-p`, `--port` | — | Port to use for the proxy server (default: auto-detect from 18080) |
