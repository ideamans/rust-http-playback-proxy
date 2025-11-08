# Repository Guidelines

## Project Structure & Module Organization
- `src/` hosts the Rust proxy core (`cli`, `recording`, `playback`, `types`, `utils`) plus `main.rs` for CLI wiring.
- `tests/` carries integration cases that drive end-to-end record/playback through Tokio.
- `acceptance/`, `golang/`, and `typescript/` hold cross-language acceptance suites and wrapper packages; keep their READMEs aligned with CLI flags and released binaries.
- `reference/types.ts` defines the shared inventory schema; update it with `types.rs` changes and leave CI-generated `coverage/` or `cobertura.xml` untouched.

## Build, Test, and Development Commands
- `cargo check` fast-fails type issues; run before touching acceptance suites.
- `cargo build --release` outputs `target/release/http-playback-proxy`, the binary consumed by wrappers.
- `cargo run -- recording --inventory ./sessions/foo` (or `-- playback`) is the quickest manual smoke.
- `cargo fmt --all` (or `-- --check`) and `cargo clippy --all-targets -- -D warnings` guard formatting and linting.
- `cargo test`, `cd acceptance/golang && go test -v`, and `cd acceptance/typescript && npm test` mirror the CI matrix.

## Coding Style & Naming Conventions
- Stick to Rust 2024 defaults: 4-space indent, `snake_case` modules/functions, `UpperCamelCase` types, `SCREAMING_SNAKE_CASE` constants.
- Keep module boundaries sharp (`recording` handles capture, `playback` handles timing) and share contracts via `types.rs`.
- Run `cargo fmt` and `cargo clippy` before every commit; never rewrite generated inventory or coverage files.

## Testing Guidelines
- Unit tests sit beside their modules; add integration flows to `tests/integration_test.rs`.
- Async tests should use `#[tokio::test(flavor = "multi_thread")]` when sockets or timers are touched.
- Acceptance runs must point at the freshly built release binary; clean `target/` if behavior looks stale.
- New features should land with at least one playback regression plus updated sample inventories kept small and anonymized.

## Commit & Pull Request Guidelines
- Follow the existing history: short imperative summaries, often scoped in Japanese (e.g., `playback: レイテンシ補正を改善`).
- Explain why timing or proxy semantics changed, reference related issues, and list affected wrappers.
- PRs need reproduction steps, expected vs actual behavior, and logs or screenshots for user-visible changes.
- Request review only after `cargo fmt`, `cargo clippy`, `cargo test`, and the acceptance suites succeed locally.

## Security & Configuration Tips
- Generated CA certs and inventory dumps can expose host data; store them outside git and ignore paths in `.gitignore`.
- Use env vars (`RUST_LOG=debug` or `RUST_LOG=trace`) instead of adding ad-hoc printlns when diagnosing HTTPS.
- Keep `reference/types.ts` aligned with `types.rs` so downstream agents parse playback metadata safely.
