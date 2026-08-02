# Lightpanda versus Obscura crawler context

## Purpose

This document records the renderer evaluation for the Rust crawler.
It preserves the results, decisions, commands, and known errors across sessions.

The crawler uses a browser only when raw HTTP extraction is not useful.
It does not use a browser for every page.

## Decision

Use the official, unmodified Lightpanda binary as the first JavaScript renderer.
Start with one subprocess for each render.
Limit renderer concurrency to two subprocesses.

Do not add Obscura as a second renderer fallback.
Two renderers add operational complexity and increase the possible failure states.

Reconsider this decision if Lightpanda fails on representative production pages.
Also reconsider it if AGPL-3.0 compliance becomes unacceptable.

## Evaluation environment

Installed binaries:

```text
Lightpanda 1.0.0-nightly.8372+5bbec625
Obscura 0.1.9
```

Both installed binaries were approximately 60 MiB.
The tests used the official, unmodified binaries from `/usr/bin`.

The evaluation fetched only the homepage of each client site.
It did not crawl links or other pages.

The evaluation measured these values:

- Process exit status
- Rendered HTML size
- Page title
- First H1
- Approximate visible-text length
- Link count
- Wall time
- Process-tree CPU time
- Peak process-tree RSS

The RSS sampler read the process tree every 10 milliseconds.
Network conditions and remote server load can change the absolute timing values.
Treat the timing and resource values as directional measurements.

Temporary source repositories were deleted after the documentation review.

## Client-site results

The resource values below come from one corrected cold run.
The visible-text calculation approximated the Rust extractor.
It excluded script, style, noscript, template, and SVG content.

| Site | Raw HTTP text | Lightpanda | Obscura |
|---|---:|---|---|
| `pmsquare.com` | 13,538 | Pass, 2.06s, 49 MiB, 0.23 CPU-s | Pass, 7.29s, 51 MiB, 0.18 CPU-s |
| `revketer.ai` | 5,470 | Pass, 3.07s, 64 MiB, 0.56 CPU-s | Timeout, 10.73s, 70 MiB, 7.87 CPU-s |
| `lumjha.ai` | 7,028 | Pass, 0.61s, 27 MiB, 0.09 CPU-s | Pass, 11.99s, 81 MiB, 8.00 CPU-s |
| `location3.com` | 2,628 | Pass, 4.13s, 77 MiB, 1.02 CPU-s | Timeout, 14.96s, 66 MiB, 10.23 CPU-s |

### Content findings

`pmsquare.com` produced the same useful content through raw HTTP and both browsers.

`revketer.ai` produced 5,470 visible characters through raw HTTP.
Lightpanda produced 7,090 visible characters and preserved the title, H1, and links.
Obscura timed out without an HTML document.

`lumjha.ai` produced 7,028 visible characters through raw HTTP and Lightpanda.
Obscura produced 7,642 characters but consumed much more CPU time.
The raw page was already complete enough for the crawler.

`location3.com` produced the same useful title, H1, text, and links through raw HTTP and Lightpanda.
Obscura timed out without an HTML document.

Lightpanda logged a worker-script error on `revketer.ai` and `location3.com`:

```text
Cannot read properties of undefined (reading 'width')
```

This error did not prevent main-document extraction.
Do not reject a rendered page only because stderr contains a nonfatal script error.
Use the process result, HTTP result, document content, and extraction quality instead.

## JavaScript-page results

| Page | Raw HTTP content | Lightpanda content | Obscura content |
|---|---:|---:|---:|
| Quotes to Scrape JS | 91 chars | 1,512 chars | 1,512 chars |
| React Shopping Cart | 30 chars | 1,380 chars | 30 chars |
| Campfire Commerce | 304 chars with loading placeholders | 2,970 chars | 2,970 chars |
| Playwright TodoMVC | 155 chars | 161 chars | 161 chars |
| `gsap.com` | 3,334 useful chars | Cloudflare block page | Process hang |

Obscura did not render the React Shopping Cart content.
The failure occurred with all tested modes:

- `domcontentloaded`
- `load`
- `networkidle0`
- Stealth enabled
- Stealth disabled

Lightpanda rendered the React Shopping Cart content with its default `done` wait mode.

Both browsers rendered Quotes to Scrape and Campfire Commerce.
Both browsers also handled the TodoMVC page.

Lightpanda received a Cloudflare block page from `gsap.com`.
Obscura did not return before the external process deadline.
The raw `gsap.com` response already contained useful content, so the detector must not render it.

## Current detector behavior

The current Go-compatible detector gives these raw-page decisions:

| Page | Score | Render decision |
|---|---:|---|
| `pmsquare.com` | 0 | No render |
| `revketer.ai` | 1 | No render |
| `lumjha.ai` | 1 | No render |
| `location3.com` | 1 | No render |
| Quotes to Scrape JS | 5 | Render |
| React Shopping Cart | 6 | Render |
| Campfire Commerce | 0 | No render |
| Playwright TodoMVC | 3 | No render |

The four client pages do not need browser rendering.
A renderer failure on those pages will not affect normal crawling.

### Detector gap

Campfire Commerce contains useful static text, links, and explicit `Loading...` placeholders.
Its rendered page contains much more product content.
The current detector does not select it.

Add a focused fixture before changing the detector.
A narrow placeholder signal can inspect important fields for values such as `Loading...`.
Do not trigger rendering for every page that contains the word `loading`.

## Resource comparison

| Property | Lightpanda | Obscura |
|---|---:|---:|
| Installed binary size | 60 MiB | 60 MiB |
| Idle CDP server RSS | 17 MiB | 11 MiB |
| Working-process peak range | 24–77 MiB | 42–81 MiB |
| Implementation | Zig | Rust |
| JavaScript engine | V8 | V8 through `deno_core` |
| HTML parser | `html5ever` | `html5ever` |
| HTTP implementation | libcurl | Reqwest or `wreq` in stealth builds |
| License | AGPL-3.0 | Apache-2.0 |
| Current release status | Beta nightly | 0.1.x release |
| Built-in stealth | No equivalent | Yes |
| Private-network blocking | Optional flag | Enabled by default |
| Pixel rendering | No | No |

Lightpanda usually used less memory on pages that completed.
It used much less CPU on the tested client pages.
Obscura used less memory while its CDP server was idle.

Obscura uses one shared, single-threaded V8 isolate in each server process.
CPU-heavy JavaScript serializes through a lock.
Obscura scales server execution with multiple worker processes.

Lightpanda documents incomplete CORS support and incomplete Web API coverage.
It identifies the project as beta software.
Production integration must treat all renderer failures as nonfatal capability loss.

## Upstream benchmark claims

Lightpanda publishes a benchmark against Headless Chrome.
The benchmark crawls 933 pages from a Lightpanda demonstration site.
The published single-process result used 27.2 MiB and completed in 51.68 seconds.

That benchmark is not a Lightpanda-versus-Obscura comparison.
The demonstration site was made to work with Lightpanda.
Do not use the upstream numbers as proof of production compatibility.

Obscura advertises approximately 30 MiB per process and an 85-millisecond median page load.
The reviewed Obscura README did not provide a direct benchmark against Lightpanda.
The local tests did not reproduce those page-load values on the client sites.

## License context

Lightpanda uses AGPL-3.0.
AGPL-3.0 permits free and commercial use.
The license does not make crawled HTML, customer data, or browser output AGPL-licensed.

The planned use has low license risk because:

- Revserp uses the official, unmodified binary.
- Lightpanda remains a separate program.
- Revserp communicates through a subprocess or CDP.
- Revserp is already open source.
- The CDP server stays private.

Keep the Lightpanda license and copyright notices.
Record the exact binary version and source revision.
If a distributed container includes Lightpanda, provide its corresponding source or source archive.

Do not link Lightpanda source code directly into the Rust worker without a new license review.
Do not modify the Lightpanda binary without recording and publishing the applicable source changes.

Obscura uses Apache-2.0.
Apache-2.0 permits private modifications and has fewer source-disclosure obligations.
Its license is simpler, but its tested rendering results were worse.

## Recommended architecture

Use this flow:

```text
Raw HTTP fetch
    -> parse and extract once
    -> run the JavaScript detector
    -> start Lightpanda only when required
    -> parse the rendered HTML once
    -> compare raw and rendered extraction quality
    -> retain only the better extraction
    -> discard both raw HTML bodies
```

A renderer failure must not fail the page or crawl.
Return the raw extraction when Lightpanda fails, times out, returns a challenge, or produces worse content.

Do not render non-2xx responses.
Do not render non-HTML responses.
Do not render a `304 Not Modified` response after conditional crawling exists.

## Recommended subprocess command

Start with a subprocess for each selected page.
This model gives strong process isolation and simple hard-timeout behavior.
Lightpanda starts quickly enough for a fallback renderer with bounded concurrency.

Recommended command shape:

```bash
LIGHTPANDA_DISABLE_TELEMETRY=true \
LIGHTPANDA_DISABLE_CORE_DUMP=1 \
lightpanda fetch "$URL" \
  --json \
  --dump html \
  --wait-until done \
  --terminate-ms 6000 \
  --http-connect-timeout 5000 \
  --http-timeout 5000 \
  --http-max-response-size 16777216 \
  --block-private-networks \
  --log-level error
```

Apply a seven-second parent-process deadline.
Kill the complete process tree when this deadline expires.
The Lightpanda deadline must expire before the parent deadline.

Apply a 16 MiB cap to captured process output.
Stop the process when stdout or stderr exceeds the configured combined cap.
The JSON wrapper can make stdout larger than the rendered HTML.

Use a shared concurrency limit of two render subprocesses.
Do not use `tokio::spawn`, channels, or a second generic task pool only for rendering.
Use the smallest bounded primitive that fits the existing crawler.

### Why `--json` is useful

JSON output includes the HTTP status, final URL, response headers, and rendered content.
The process can exit successfully after it receives a block page.
The HTTP result is necessary to reject these responses.

Parse these values from the JSON result:

- Final URL
- HTTP status
- Response headers
- Rendered HTML content

Reject non-2xx HTTP results.
Reject a final URL that violates crawler scope or SSRF policy.
Do not trust the requested URL as the final URL.

### Wait-mode findings

Use `--wait-until done` as the first generic mode.
It produced the best overall content in the tests.

Do not use a one-second fixed wait:

```text
--wait-ms 1000
```

This mode returned only `<!DOCTYPE html>` for slower pages.
The process exited successfully even though the document was empty.

Do not use `domcontentloaded` as the only Lightpanda completion signal.
It returned Campfire Commerce before its asynchronous product requests completed in one test.

`networkidle` worked on the tested JavaScript demonstrations.
It can wait too long on pages with analytics, chat widgets, or persistent connections.
Use it only after new production evidence shows that `done` is insufficient.

Do not add `--disable-workers` by default.
Some pages use workers to calculate state or update the main page.
Nonfatal worker errors are acceptable when the main extraction is useful.

Do not add `--disable-subframes` without a content-quality comparison.
An iframe can contain useful content, but most iframe content is not a page fact.
This flag needs a separate resource and quality evaluation.

Do not enable external stylesheet fetching by default.
The crawler extracts semantic HTML and does not need pixel layout.

## Result acceptance rules

A zero process exit code does not mean that rendering succeeded.
A nonzero process exit code does not mean that all rendered output is unusable.
Lightpanda can return useful main-document HTML after a nonfatal worker-script error.
It can also return an empty document or a browser-block page with exit code zero.

Capture stdout and stderr separately.
Treat stderr, the exit status, and deadline termination as diagnostics.
Always attempt to decode bounded stdout, even when Lightpanda exits with a nonzero code.
Never mix stderr into the JSON or HTML buffer.

Accept a complete candidate when its status, URL, content, and quality checks pass.
The raw extraction remains available when no complete candidate passes these checks.

Reject the rendered result when any condition is true:

1. The combined output exceeds the configured cap.
2. Stdout does not contain one complete, valid JSON result.
3. The HTTP status is not 2xx.
4. The final URL is unsafe or out of scope.
5. The content is empty or contains only a document shell.
6. The response is a known challenge or block page.
7. Rendered extraction quality is not greater than raw extraction quality.

Useful challenge signals include:

- Title contains `Attention Required`
- H1 contains `Sorry, you have been blocked`
- Page asks for a CAPTCHA
- Page asks the user to enable cookies before access
- HTTP status is 401, 403, 429, or 503

Do not use challenge text alone when a clear non-2xx status exists.
Use the HTTP status first.

## Extraction quality comparison

Keep Go parity for the first implementation.
The current quality score uses these signals:

- Add 2 points for a nonempty title.
- Add 1 point for a nonempty meta description.
- Add 2 points for a nonempty H1.
- Add 3 points when visible text has at least 200 characters.
- Otherwise, add 1 point for nonempty visible text.
- Add 2 points when the page has more than three links.
- Otherwise, add 1 point when the page has at least one link.

Use the rendered extraction only when its score is greater than the raw score.
Do not replace equal-quality raw extraction.

Parse the rendered HTML once with the same shared `scraper::Html` extraction path.
Do not create a second set of renderer-specific extractors.

## SSRF requirements

Lightpanda performs its own DNS resolution and network requests.
The existing Reqwest resolver cannot protect Lightpanda requests.

Use all of these controls:

1. Validate the requested URL before process start.
2. Pass `--block-private-networks` to Lightpanda.
3. Parse and validate the final URL from JSON output.
4. Reject unsafe or cross-scope final URLs.
5. Keep the parent process deadline and output cap.

The Lightpanda flag blocks private addresses after DNS resolution.
It is the browser-level protection against DNS results that point to private networks.

Keep the CDP server on loopback or an internal Unix-network boundary.
Never expose an unauthenticated CDP endpoint to the public internet.

## CDP server option

Do not start with CDP.
Use it only after subprocess benchmarks show that process startup limits throughput.

A possible private server command is:

```bash
LIGHTPANDA_DISABLE_TELEMETRY=true \
LIGHTPANDA_DISABLE_CORE_DUMP=1 \
lightpanda serve \
  --host 127.0.0.1 \
  --port 9222 \
  --block-private-networks \
  --http-connect-timeout 5000 \
  --http-timeout 5000 \
  --http-max-response-size 16777216 \
  --watchdog-ms 6000 \
  --log-level error
```

Discover the WebSocket endpoint from the server metadata instead of assuming a fixed path.
Apply a client deadline to every CDP operation.
Restart the server when its health check fails.
Keep two logical render slots even if the server accepts more CDP connections.

The measured idle RSS was approximately 17 MiB for Lightpanda.
Obscura used approximately 11 MiB while idle.
This idle difference does not offset Obscura's compatibility failures on the tested pages.

## Operational logging

Record one structured event when the detector selects rendering.
Include the requested URL and detector reasons.

Record one result event with these fields:

- Renderer name and version
- Requested URL
- Final URL
- HTTP status
- Wall time
- Output size
- Raw quality score
- Rendered quality score
- Applied, discarded, failed, or timed-out result

Do not write full HTML or customer content to logs.
Do not treat a discarded rendered page as a crawl error.

## Initial implementation and test plan

1. Port the JavaScript detector and its Go fixtures.
2. Add the focused loading-placeholder fixture.
3. Define a small renderer interface.
4. Implement the Lightpanda subprocess adapter.
5. Add the process deadline and output cap.
6. Parse Lightpanda JSON output.
7. Apply status, final-URL, scope, and SSRF checks.
8. Parse rendered HTML through the existing extraction path.
9. Compare extraction quality.
10. Add bounded renderer concurrency.

Required tests:

- Detector threshold and reason parity
- No renderer call for useful raw HTML
- Quotes-style JavaScript content improvement
- React app-shell content improvement
- Loading-placeholder detection
- Empty document with exit code zero
- Non-2xx JSON result
- Cloudflare-style challenge page
- Malformed JSON
- Output cap
- Lightpanda deadline followed by parent kill
- Unsafe final URL
- Cross-scope final URL
- Nonfatal script error with useful HTML
- Equal-quality rendered page discarded
- Better rendered page selected
- Renderer absence remains nonfatal
- Renderer concurrency never exceeds two

## Source references

- Lightpanda repository: <https://github.com/lightpanda-io/browser>
- Lightpanda benchmark details: <https://github.com/lightpanda-io/demo/blob/main/BENCHMARKS.md>
- Obscura repository: <https://github.com/h4ckf0r0day/obscura>
- Existing Go renderer: `revserp-backend/internal/crawler/renderer.go`
- Existing Go detector: `revserp-backend/internal/crawler/render_detector.go`
- Existing Go renderer integration: `revserp-backend/internal/crawler/worker.go`

## Final summary

Lightpanda gave the best content coverage on the tested fallback pages.
It also used less CPU and usually less memory during successful client-page renders.

Obscura has a simpler Apache-2.0 license and lower idle server memory.
It failed or timed out on more tested pages, including the important React app-shell case.

Use raw HTTP first, Lightpanda only when selected, and raw extraction after every renderer failure.
Use a subprocess, `done` wait mode, strict deadlines, private-network blocking, bounded output, and quality-based replacement.
