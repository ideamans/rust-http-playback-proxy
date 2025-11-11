# CODEX Review – SIGNAL.md Alignment

## Findings

1. **Windows recording stop is now destructive**  
   - `typescript/src/proxy.ts:71-110` always sends `'SIGTERM'`, but on Windows Node.js turns that into `TerminateProcess`, so the proxy never receives the `CTRL_C`/`CTRL_BREAK` events handled by `src/recording/signal_handler.rs`. Recording sessions started via the TS wrapper will abort without saving inventories. Emit `'SIGBREAK'` (or `'SIGINT'`) on win32 so the Rust handler can flush batches.

2. **Go wrappers/tests no longer compile**  
   - `RecordingOptions.ControlPort` and all dereferences remain in `acceptance/golang/main_test.go:90-114` and `golang/example/shutdown_test.go:15-48`, but the struct field was removed in `golang/proxy.go:40-127`. Running `go test ./...` now fails with “unknown field 'ControlPort'”. Either retain the field (ignored) for compatibility or update every caller before merging.

3. **Go `Reload()` signature mismatch**  
   - `golang/proxy.go:299-335` now returns only `error`, while call sites such as `acceptance/golang/main_test.go:375-386` expect `(string, error)` and log the returned status. This change breaks the acceptance suite and any downstream code.

4. **Rust e2e suites still depend on `--control-port`**  
   - `e2e/performance/src/main.rs:218-288` and `e2e/content/src/main.rs:416-486` always pass `recording --control-port …`, and both binaries still POST to `/_shutdown` (`performance`:613-626, `content`:1180-1196). With the CLI flag removed, these tools now exit immediately, so the documented CI pipeline cannot run.

5. **Docs and TypeScript types advertise removed behavior**  
   - `README.md:76-98`, `README_ja.md:76-98`, and `typescript/src/types.ts:50-64` still claim recording supports `--control-port` and that playback uses `/ _shutdown`, and they list the default port as `8080` (actual default 18080 via `get_port_or_default`). Users following the README will hit rejected flags/endpoints, and TS consumers still see `controlPort` as a supported recording option.

## Suggested Follow-ups

1. Fix wrapper signal semantics (TS + Go) so Windows users still get graceful shutdown/reload.  
2. Update Go API/tests/examples together, or keep deprecated fields temporarily to avoid breaking dependents.  
3. Rewrite the e2e suites to send signals (SIGTERM/SIGINT) instead of HTTP shutdowns, and add Windows-specific fallbacks if needed.  
4. Align public docs (`README*.md`, `CLAUDE.md`, `SIGNAL.md`, package READMEs) and TypeScript types with the new SIGNAL design, including the 18080 default and the “Windows reload requires control port” rule.  
5. Rerun `cargo test`, `go test ./...`, `npm test`, and the `e2e/*` binaries once the above is addressed to ensure CI still matches AGENTS.md expectations.
