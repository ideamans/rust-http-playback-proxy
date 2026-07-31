---
name: http-playback-proxy-install
description: Make the http-playback-proxy command available, installing it only if it is missing. Use when another skill reports that `http-playback-proxy` is not on PATH, or when the user asks to install, update or upgrade the ideamans HTTP recording and playback proxy. Prefers an already-installed binary, then the latest GitHub release, then a build from source with cargo.
license: MIT
compatibility: Requires curl and tar to install from a release, or a Rust toolchain for the source fallback. Standalone — does not need http-playback-proxy to be present already. Installs from the public repository github.com/ideamans/rust-http-playback-proxy, so no GitHub authentication is needed.
allowed-tools: Bash(curl:*) Bash(wget:*) Bash(tar:*) Bash(unzip:*) Bash(cargo:*) Bash(uname:*) Bash(command:*) Bash(which:*) Bash(mkdir:*) Bash(mv:*) Bash(cp:*) Bash(rm:*) Bash(chmod:*) Bash(ls:*) Bash(test:*) Bash(echo:*) Read
---

# http-playback-proxy-install

Make the `http-playback-proxy` command usable, doing the least work that
achieves it.

## Route 1 — an existing installation on PATH

```bash
command -v http-playback-proxy && http-playback-proxy --version
```

If that resolves, **use it and stop here.** Do not check for a newer release —
it costs an API call and the user did not ask for an upgrade.

Two checks before trusting the hit:

- **It is the right tool.** `http-playback-proxy llm | head -1` must read
  `# http-playback-proxy — reference for AI agents`. If something else owns
  the name, say so and use an explicit path rather than shadowing theirs.
- **It is recent enough.** If `llm` is not a known subcommand, the binary
  predates the embedded reference — continue to route 2 to upgrade it.

## Route 2 — the latest GitHub release

The repository is public, so no authentication is needed.

```bash
VERSION=$(curl -fsSL https://api.github.com/repos/ideamans/rust-http-playback-proxy/releases/latest \
  | grep '"tag_name"' | head -1 | cut -d'"' -f4)
```

**This project builds with Cargo, not goreleaser, so the asset names do not
follow the usual pattern.** List them and match one to this machine rather
than guessing:

```bash
curl -fsSL "https://api.github.com/repos/ideamans/rust-http-playback-proxy/releases/tags/${VERSION}" \
  | grep '"name"' | grep -E 'tar.gz|zip'
```

Rust release assets are usually named by target triple — `x86_64-apple-darwin`,
`aarch64-apple-darwin`, `x86_64-unknown-linux-gnu` and so on. Pick the one
matching `uname -s` / `uname -m`, download it, and unpack.

### Install onto PATH

```bash
mkdir -p ~/.local/bin && mv ./http-playback-proxy ~/.local/bin/ \
  && chmod +x ~/.local/bin/http-playback-proxy
```

Prefer the first writable directory already on PATH — `~/.local/bin`, then
`/usr/local/bin`. Two things not to do on your own initiative:

- If nothing on PATH is writable, leave the binary in `/tmp`, print the exact
  `sudo mv` command and let the user run it. Do not run `sudo` yourself.
- If `~/.local/bin` is not on PATH, give the user the line for their shell
  profile. Do not edit the profile for them.

## Route 3 — build from source

Needs a Rust toolchain (`rustup`). Note the crate builds **two** binaries, so
the target has to be named:

```bash
git clone https://github.com/ideamans/rust-http-playback-proxy
cd rust-http-playback-proxy
cargo build --release --bin http-playback-proxy
mv target/release/http-playback-proxy ~/.local/bin/
```

If `cargo` is missing, say so and let the user decide whether to install a
Rust toolchain — do not install one on their behalf.

## Verify

```bash
command -v http-playback-proxy && http-playback-proxy --version \
  && http-playback-proxy llm | head -1
```

Report the version and the path. Nothing further is needed to run it, but
recording HTTPS requires the client browser to trust the proxy's certificate —
mention that if the user is about to record.
