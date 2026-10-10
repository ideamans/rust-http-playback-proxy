# http-playback-proxy — reference for AI agents

`http-playback-proxy` records the HTTP traffic of a page load into a local
inventory, then replays it — **reproducing the original timing**, not just the
bytes. That is the point: it turns a live site into a fixed, repeatable target
so performance work can be measured against something that does not move.

It is a MITM proxy driving a browser, so recording reaches the real site.
Nothing prompts; it reads flags only. This reference is embedded in the
binary, so `http-playback-proxy llm` always describes the exact version you
are running.

## Ground rules

1. **Record first, then play back.** `playback` serves whatever is in the
   inventory directory; with no recording it has nothing to serve.
2. **The inventory directory is the artifact.** Both subcommands default to
   `./inventory`. Point `-i` at the same place for both, and keep it — it is
   what makes a later run comparable to an earlier one.
3. **Recording hits the real site.** Requests come from wherever you run it.
   On someone else's property, confirm that is acceptable first.
4. **Playback reproduces timing by default.** TTFB and transfer speed are
   replayed, which is what makes the measurement meaningful. `--full-throttle`
   removes that, and the result is no longer comparable to a real load — use
   it only when you want the fastest possible replay for a smoke test.
5. **Unmatched requests 404 by default.** That is deliberate: a silent
   fall-through to the network would make the replay non-deterministic.
   `--passthrough` forwards them instead, at the cost of that guarantee.
6. **An unedited resource is replayed byte for byte.** The recorder keeps the
   body exactly as it crossed the wire under `originals/`, with the SHA-256 of
   the editable file under `contents/`. While that file, `contentEncoding`,
   `contentCharset` and `minify` are unchanged, playback serves the recorded
   bytes and headers. Change any of them and the body is regenerated from the
   editable file: re-minified with a real parser if `minify` is true,
   re-encoded to `contentCharset`, compressed with `contentEncoding`, with the
   `Content-Encoding` header rewritten to match.
7. **Transfer time follows the bytes.** In an inventory with
   `"formatVersion": 2`, a body takes `wire bytes × 8 / mbps`, where `mbps` was
   measured from the recorded wire bytes and the download time in microseconds.
   An unedited body therefore takes exactly the recorded time; a smaller file
   arrives sooner; a higher `mbps` models a faster link. Inventories without
   `formatVersion` (recorded before 0.11) replay the recorded `durationMs`
   whatever the size — record again to get size-aware timing.
8. **HTML is stored as recorded.** Minified JavaScript and CSS are stored
   reformatted (swc, lightningcss) for editing and minified again with the same
   parsers; HTML is never reformatted, because re-serialising a document changes
   whitespace between inline elements, character references and `<pre>`.

## Commands

| Task | Command |
| --- | --- |
| Capture a page's traffic | `http-playback-proxy recording <url>` |
| Serve it back | `http-playback-proxy playback` |

Both take `-p/--port` (auto-detected from 18080 upwards when omitted) and
`-i/--inventory` (default `./inventory`).

### recording

```bash
http-playback-proxy recording https://example.com -i ./inventory -d mobile
```

- `-d/--device` — `mobile` (default) or the other device profiles
- `-e/--extra-url` — additional entry URLs, repeatable
- `-x/--exclude` — regex patterns for URLs to leave out, repeatable. Use this
  for analytics and ad beacons that add noise and third-party latency

### playback

```bash
http-playback-proxy playback -i ./inventory -p 18080
```

Then point a browser or a measurement tool at that proxy.

There is a third subcommand, `signal`, which is a hidden internal helper for
Windows process control. It is not part of the workflow.

## Typical use

```bash
# 1. Freeze the site as it is today
http-playback-proxy recording https://example.com -i ./baseline -x 'google-analytics|doubleclick'

# 2. Serve it, and measure against the proxy rather than the live site
http-playback-proxy playback -i ./baseline -p 18080
```

The value is that step 2 gives the same answer tomorrow. Measuring the live
site does not.

## Failure modes

| Symptom | Cause | Fix |
| --- | --- | --- |
| playback serves nothing | the inventory is empty or `-i` points elsewhere | record first; use the same `-i` for both |
| everything 404s during playback | requests do not match what was recorded | expected for URLs that were never recorded — `--passthrough` if you accept the loss of determinism |
| the port is already in use | another instance, or the port is taken | omit `-p` to auto-detect from 18080 |
| the replay is much faster than reality | `--full-throttle` was passed | drop it; timing reproduction is the point |
| shrinking a file does not shorten its transfer | the inventory has no `formatVersion` (recorded before 0.11) | record again |
| a regenerated body is larger than the recorded one | compression levels differ from the origin's | expected for edited resources; unedited ones are served as recorded |
| the recording is noisy or slow | third-party beacons | `-x` with a regex for them |
| HTTPS pages fail to record | the browser does not trust the proxy CA | this is a MITM proxy; the client has to accept its certificate |

## What this CLI will not do

- It does not measure. It provides a stable target; use Lighthouse, `loadshow`
  or a browser against the proxy for numbers.
- It does not crawl. One entry URL plus whatever that page loads, extended by
  `-e`.
- It does not edit the recorded content.
