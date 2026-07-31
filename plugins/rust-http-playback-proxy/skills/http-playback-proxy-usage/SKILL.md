---
name: http-playback-proxy-usage
description: Record a web page's HTTP traffic and replay it with the original timing, using the http-playback-proxy CLI, so performance work can be measured against a target that does not change between runs. Use when the user wants repeatable page-speed measurements, needs to compare before and after an optimisation fairly, asks to capture or replay a site's traffic, or is frustrated that measurements of a live site keep moving.
license: MIT
compatibility: Requires the `http-playback-proxy` binary on PATH — run the http-playback-proxy-install skill if it is missing. Recording drives a browser through a MITM proxy and reaches the real site; the client must trust the proxy's certificate for HTTPS.
allowed-tools: Bash(http-playback-proxy:*) Bash(command:*) Bash(ls:*) Read
---

# http-playback-proxy-usage

Turning a live site into a fixed target you can measure twice.

## 1. Confirm the tool

```bash
command -v http-playback-proxy && http-playback-proxy --version
```

Missing binary? Run the `http-playback-proxy-install` skill.

## 2. Why this exists

Measuring a live site gives a different answer every run — CDN state,
third-party latency, your own network. This records one load and replays it
**with the original timing**, so a before/after comparison reflects the change
you made rather than the weather.

Say that to the user when proposing it. If they only need one screenshot or
one number, this is more machinery than the job needs.

## 3. Record before you can play back

```bash
http-playback-proxy recording https://example.com -i ./baseline \
  -x 'google-analytics|doubleclick'
```

- **`-i/--inventory` is the artifact.** Default `./inventory`. Use the same
  path for both subcommands, and keep it — it is what makes next month's run
  comparable.
- **`-x/--exclude`** (repeatable regex) drops analytics and ad beacons.
  Recommend it: they add third-party latency that has nothing to do with the
  site.
- `-e/--extra-url` adds more entry URLs; `-d/--device` selects the profile
  (`mobile` by default).

**Recording reaches the real site.** If the target belongs to someone else,
confirm that is acceptable before running it.

## 4. Play it back

```bash
http-playback-proxy playback -i ./baseline -p 18080
```

Then point the browser or measurement tool at that proxy. Omit `-p` and it
auto-detects from 18080.

Two flags that change what the result means — do not pass them casually:

- **`--full-throttle`** removes timing reproduction. The replay gets fast, and
  the numbers stop being comparable to a real load. Smoke tests only.
- **`--passthrough`** forwards unmatched requests to the network instead of
  returning 404. Convenient, but it reintroduces exactly the nondeterminism
  the tool exists to remove. If you use it, say so alongside the results.

## 5. Read the reference for anything else

```bash
http-playback-proxy llm
http-playback-proxy recording --help
```

## 6. Report

Say which inventory the numbers came from and whether `--full-throttle` or
`--passthrough` was in play. A measurement taken with either is not
comparable to one taken without.

## Failure modes

| Symptom | Fix |
| --- | --- |
| `command not found` | run the `http-playback-proxy-install` skill |
| playback serves nothing | record first, and check `-i` points at the same directory |
| everything 404s | those URLs were never recorded — expected; `--passthrough` if you accept the loss of determinism |
| port already in use | omit `-p` to auto-detect from 18080 |
| replay is unrealistically fast | `--full-throttle` was passed |
| HTTPS fails while recording | the browser must trust the MITM proxy's certificate |
| recording is noisy | exclude beacons with `-x` |
