---
name: lkml-summary
description: Explain a single lore.kernel.org mail (given its Message-ID) as a technical article — original problem → solution → experimental results → conclusion, with diagrams where they help. Use when the user passes an LKML / kernel mailing-list Message-ID (msgid) and wants a deep, readable write-up of that one mail or patch thread rather than a daily digest.
---

# lkml-summary — explain one LKML mail as a technical article

Given a **Message-ID**, locate that mail via `lkml-digest`, then turn it into a
self-contained technical article that a reader who *isn't* following the thread
can understand: what problem it solves, how, what the measurements say, and what
to take away.

This is the deep counterpart to `lkml-digest`: that skill scans a whole window
shallowly; this one reads **one** mail (and its thread context) deeply.

## Arguments

`/lkml-summary <msgid> [lang] [list]` — order of the trailing tokens is flexible.

- **msgid** (required) — the `Message-ID`, with or without angle brackets and
  with or without a leading `id:`/`Message-ID:`. All of these are accepted:
  `<ah_VMf0ZJTRsrArV@lucifer>`, `ah_VMf0ZJTRsrArV@lucifer`,
  `https://lore.kernel.org/all/ah_VMf0ZJTRsrArV@lucifer/`.
  If the user pasted a lore URL, extract the msgid segment from it.
- **lang** — `en` → English, `zh` → Traditional Chinese (繁體中文).
  Default: match the language of the user's invocation prompt (zh prompt → zh).
- **list** — mailing list name (default `lkml`). `--select-msgid` is scoped to
  this list, so pass it when the mail lives on a non-`lkml` list (e.g.
  `linux-pm`, `linux-mm`).

## When to use

Trigger on prompts that hand you a specific mail rather than a time window:
- "explain this msgid: …"
- "幫我解釋這封信 <…@…>"
- "write up the patch at https://lore.kernel.org/all/…/"
- "summarize this thread / what does this patch do" + a Message-ID

If the user instead asks for "the last 24h" / "what's hot on <list>", use
**lkml-digest**, not this skill.

## Retrieval: lkml-digest only

`lkml-digest --select-msgid` resolves **from the local mirror, bounded only by
the `--since`/`--range` window** (see its `select_by_msgid` in
`src/main.rs`). It searches every epoch the window spans, and
auto-fetches earlier epochs — retired quarters included, back to the list's
first epoch — whenever the window reaches further back than the local mirror
already holds (`ensure_mirror_covering` in `vendor/lkml-core/src/archive.rs`).
That's the only retrieval path this skill uses — no network fetch of individual
mails. The single bound is the window: a mail is reachable iff its `Date:` falls
within `--since`/`--range`. Older mail just needs a wider window (and, the first
time it crosses into an un-mirrored quarter, a one-off clone of that epoch).

1. **Build once** (no-op when current):
   ```sh
   cargo build --release
   ```

2. **Locate the mail.** Normalize the msgid to include angle brackets, then run
   `--select-msgid`, starting with a modest window and widening only if needed:
   ```sh
   ./target/release/lkml-digest --list <LIST> --since 7d \
       --select-msgid '<MSGID>'
   ```
   - A `count=1` block whose `Message-ID:` matches → you have it; go to
     **Thread context**.
   - `count=0`, or a stderr `warning: message-id … not found in local mirror
     window` → widen the window **one step at a time**: `7d` → `30d` → `90d` →
     `180d` → `365d`. Widening re-walks the whole window and fetches every mail
     body in it, and crossing a quarter boundary triggers a one-off clone of the
     older (large) epoch, so each step is progressively slower on `lkml` — climb
     gradually, don't jump straight to a year.
   - Still not found at a year-wide window, or you suspect a different list: tell
     the user the msgid isn't in the mirror within that window, suggest they
     confirm the **list** name (pass it as the `list` arg) or widen further, and
     stop. Do not fabricate a summary.

## Thread context (do this — results often live elsewhere in the thread)

A single patch mail rarely contains everything. **Benchmarks and "experimental
results" are usually in the `[PATCH 0/N]` cover letter or in a reply**, and the
*motivation* may be upthread. Reconstruct the thread from the local mirror:

1. **Compact scan** the same window to see the neighbours (metadata only, no
   bodies):
   ```sh
   ./target/release/lkml-digest --list <LIST> --since <WINDOW> --format compact
   ```
2. **Match the thread.** Every compact record carries `Thread: <root msgid>`,
   the Message-ID of the thread it hangs off. Take the target's `Thread:` value
   and collect every record with the same one — that is the cover letter
   `0/N`, the other `M/N` patches, and the `Re:` review replies, without
   parsing subjects. `Replies:` on the root shows how hot it is.
3. **Pull those bodies in one call** by their `Commit:` ids:
   ```sh
   ./target/release/lkml-digest --list <LIST> --since <WINDOW> \
       --select-commit <commit1>,<commit2>,…
   ```

Read the pulled thread to find:
- the **cover letter** (`[PATCH 0/N]`) — problem statement + numbers,
- **maintainer review replies** — objections, the real point of contention,
- **v2/v3 deltas** if the subject shows a version bump.

If parts of the thread fall outside the window, summarize what you have
and note that earlier context wasn't in the local mirror — don't invent it.

Keep raw bodies *out* of your final answer. Quote at most a few short lines
(a function name, a key benchmark row, one decisive review sentence) verbatim.

## Reading the mail

Decode/clean as you read:
- `lkml-digest` already decodes quoted-printable / base64 bodies; you read the
  decoded text directly.
- Strip leading `> ` quote levels to separate *this* author's words from quoted
  context, but keep quotes when they show **what is being replied to**.
- Identify the mail's role: cover letter, a numbered patch, a review reply, or a
  standalone RFC — the article framing differs slightly for each.
- Note `From:`, `Date:`, `Subject:` (version + `M/N`), and the diffstat/changed
  files if a patch is inline.

## Output: a technical article as HTML

Write one self-contained HTML file to `out/summary-<msgid>-<lang>.html`
(create `out/` if missing; `<msgid>` = the id without angle brackets, any
character outside `[A-Za-z0-9._-]` replaced by `_`). Flowing prose, not a
bullet dump. Headings and narration in the chosen language; **all technical
identifiers verbatim** (function names, struct fields, config symbols like
`ANON_VMA_LAZY`, commit hashes, subjects, file paths, maintainer names,
numbers/units). HTML-escape `<` `>` `&` in subjects, Message-IDs and code.
Lead with a one-line orientation, then the four-part arc. Use a diagram
**only when it earns its place** — a before/after data-structure change, a
control-flow/lock ordering, a state machine, or a benchmark table. Numbers go
in a `<table>`, structure in a `<pre>` ASCII block; skip diagrams for a purely
textual discussion.

```html
<!doctype html>
<html lang="en">                       <!-- zh: lang="zh-Hant" -->
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title><plain-language title></title>
<style>
body{font:16px/1.6 system-ui,sans-serif;max-width:50rem;margin:2rem auto;padding:0 1rem;color:#222;background:#fff}
table{border-collapse:collapse}th,td{border:1px solid #ccc;padding:.3rem .6rem;text-align:left}
code,pre{background:#f3f3f3}code{padding:.1em .3em}pre{padding:.8rem;overflow-x:auto}
blockquote{color:#555;border-left:3px solid #ccc;margin:0;padding-left:1rem}
@media(prefers-color-scheme:dark){body{background:#111;color:#ddd}code,pre{background:#222}th,td{border-color:#444}}
</style>
</head>
<body>
<h1><plain-language title> — <code><original Subject></code></h1>
<blockquote>Message-ID: <code>&lt;msgid&gt;</code> · From: <author> · <date> · list: <list><br>
Role: <cover letter | PATCH n/N | review reply | RFC> · Thread: <N> mails</blockquote>

<p><b>TL;DR.</b> 2–3 sentences: what this changes and why it matters.</p>

<h2>The problem</h2>
<p>What was broken / slow / missing before this. Ground it in the kernel
mechanism involved (the subsystem, the data structure, the hot path). If the
thread debated <em>whether</em> it's a problem, say so.</p>

<h2>The approach</h2>
<p>How the patch/series solves it. Walk the key change; name the functions,
flags, and structures touched. Diagram the before→after in a <pre> if structural.</p>

<h2>Results</h2>
<p>What the cover letter / replies measured — workload, machine, numbers,
deltas, in a <table>. If there are no measurements, say "no benchmarks
posted" rather than inventing any.</p>

<h2>Takeaways</h2>
<p>Status (merged / under review / NAK'd / RFC), the main point of contention,
and what to watch next. 2–4 sentences.</p>
</body>
</html>
```

Traditional Chinese (`zh`): same skeleton with `lang="zh-Hant"` and these
labels:

| en | zh |
|---|---|
| Message-ID: · From: · list: | Message-ID： · 作者： · list： |
| Role: · Thread: N mails | 性質： · 討論串：N 封 |
| TL;DR. | 一句話總結。 |
| The problem | 原始問題 |
| The approach | 解決方法 |
| Results · "no benchmarks posted" | 實驗結果 · 「未附 benchmark」 |
| Takeaways | 小結 |

Finish by replying with the file path and the TL;DR — not the HTML itself.

## Notes & failure modes

- **Not found in the mirror**: with a wide enough window the search reaches any
  epoch back to the list's first, so a miss usually means the `Date:` is outside
  the window you tried, or the mail is on a different list. Widen the window
  and/or confirm the `list` arg; if it still won't surface, report that plainly
  and stop — don't guess a different mail.
- **Wrong list**: `--select-msgid` is scoped to `--list`. If the user knows the
  mail is on `linux-pm`/`linux-mm`/etc., pass that as the `list` arg.
- **A bare reply with no substance** ("Reviewed-by: …", "+1"): say so plainly
  and summarize the *parent* it's acking, using the compact-scan thread context.
- **Huge Cc lists** (as kernel mails have): never reproduce them; name only the
  author and the maintainers who actually replied.
- **No numbers in the thread**: keep the **Results** section but state that no
  benchmarks were posted — never fabricate measurements or speedups.
