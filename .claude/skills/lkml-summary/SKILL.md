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
2. **Match the thread.** Take the target's base subject — strip a leading `Re:`
   and the `[PATCH vN M/K]` tag down to the series title — and collect every
   record whose subject shares that base (the cover letter `0/N`, the other
   `M/N` patches, and `Re:` review replies). `Replies:` on the root shows how
   hot it is.
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

## Output: a technical article

Write flowing prose, not a bullet dump. Headings and narration in the chosen
language; **all technical identifiers verbatim** (function names, struct
fields, config symbols like `ANON_VMA_LAZY`, commit hashes, subjects, file
paths, maintainer names, numbers/units). Lead with a one-line orientation, then
the four-part arc. Use a diagram **only when it earns its place** — a before/
after data-structure change, a control-flow/lock ordering, a state machine, or a
benchmark table. Prefer a Markdown table for numbers and a fenced ASCII/mermaid
block for structure; skip diagrams for a purely textual discussion.

Template — English (`en`):

```
# <plain-language title> — `<original Subject>`

> Message-ID: `<msgid>` · From: <author> · <date> · list: <list>
> Role: <cover letter | PATCH n/N | review reply | RFC> · Thread: <N> mails

**TL;DR.** 2–3 sentences: what this changes and why it matters.

## The problem
What was broken / slow / missing before this. Ground it in the kernel
mechanism involved (the subsystem, the data structure, the hot path). If the
thread debated *whether* it's a problem, say so.

## The approach
How the patch/series solves it. Walk the key change; name the functions,
flags, and structures touched. Diagram the before→after if structural.

## Results
What the cover letter / replies measured — workload, machine, numbers, deltas.
Put figures in a table. If there are no measurements, say "no benchmarks
posted" rather than inventing any.

## Takeaways
Status (merged / under review / NAK'd / RFC), the main point of contention,
and what to watch next. 2–4 sentences.
```

Template — Traditional Chinese (`zh`):

```
# <白話標題> — `<原始 Subject>`

> Message-ID：`<msgid>` · 作者：<author> · <date> · list：<list>
> 性質：<cover letter | PATCH n/N | review 回覆 | RFC> · 討論串：<N> 封

**一句話總結。** 兩三句：這個改動做了什麼、為什麼重要。

## 原始問題
在這個 patch 之前，哪裡壞了／慢了／少了什麼。扣著牽涉到的核心機制
（子系統、資料結構、hot path）來談。若討論串在爭論「這到底算不算問題」，
也要點出來。

## 解決方法
patch／patch series 怎麼解這個問題。逐步說明關鍵改動，點名被動到的
函式、flag、結構。若是結構性改動，用圖示對照 before→after。

## 實驗結果
cover letter／回覆裡量到的數據——測試負載、機器、數字、差異。
數據用表格呈現。若沒有任何量測，就寫「未附 benchmark」，不要自己編。

## 小結
狀態（已 merge／review 中／被 NAK／RFC）、主要爭議點、後續值得關注的方向。
兩到四句。
```

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
