---
name: lkml-digest
description: Summarize the last 24 hours (or a chosen window) of a lore.kernel.org mailing list. Use when the user asks for an LKML / kernel mailing-list digest, daily summary, top topics/subsystems, or "what happened on <list> recently". Wraps the local `lkml-digest` CLI which reads from the lkml-tools cache.
---

# lkml-digest — LKML daily digest

This skill turns the locally cached lore.kernel.org mirror into a summary
by piping the `lkml-digest` CLI output into the model.

## Arguments

The user may pass a language token as the first argument:

- `en` → write the final summary in English (default).
- `zh` → write the final summary in Traditional Chinese (繁體中文).

Additional free-form tokens after the language are treated as overrides for
the list and/or window, e.g.:

- `/lkml-digest zh` — Chinese summary of the last 24h of `lkml`.
- `/lkml-digest en linux-pm 7d` — English summary of the last week of linux-pm.
- `/lkml-digest zh linux-mm today` — Chinese, today only, linux-mm.

If the first token isn't `en` or `zh`, fall back to: language = match the
language of the user's invocation prompt; treat all tokens as list/window
hints.

## When to use

Trigger on prompts like:
- "summarize the last 24h of lkml"
- "what's hot on linux-pm this week"
- "give me a digest of <list>"
- "top kernel subsystems in the last <N> hours"

## Strategy: filter cheap, then read deep

Don't pour every full mail body into context. Use the CLI's two-phase design:

1. **`--format threads`** prints one short record per *thread* (head mail's
   metadata, in-window mail count, participants, series revision, and every
   member's commit id) — no bodies. Scan this cheaply to decide what's worth
   reading. (`--format compact` is the per-mail variant, several times larger;
   use it only when you need one specific mail's `Replies:` or `Series:`.)
2. **`--select-commit <commit>,…`** (or **`--select-msgid <id>,…`**) then
   re-fetches *only* the chosen mails in full, by the `Commits:` ids the thread
   pass handed you. This keeps the expensive full-body read scoped to the
   handful of threads you'll actually summarize.

## Steps

1. **Build.** Always run `cargo build --release` from the repo
   root first, so the binary tracks the latest source (it's a no-op rebuild
   when already current).

2. **List & window.** Default `lkml` / `--since 24h`. "this week" → `7d`,
   "today" → `--range today`. Override on user request.

3. **Phase 1 — thread scan** (metadata only, no bodies):

   ```sh
   ./target/release/lkml-digest --list <LIST> --since <WINDOW> --format threads \
       --exclude-from syzbot,lkp@intel.com
   ```

   Output is a `# … count=N` header (N = mails, not threads), then
   blank-line-separated records:

   ```
   Subject: <head mail's subject>
   From: <head mail's sender>
   Date: <head>          Latest: <newest in-window mail>
   Mails: <in-window count>
   Participants: <distinct authors, first-seen order>
   Series: v3 12 patches     (only for patch threads; highest revision seen)
   Message-ID: <head>
   Thread: <root Message-ID>
   Commits: <c1>,<c2>,…      (every in-window member, oldest first)
   ```

   The head is the thread's oldest in-window mail. A `Subject:` starting with
   `Re:` while `Message-ID` ≠ `Thread` means the discussion started before the
   window; summarize what's in the window and say so. `--exclude-from` drops
   bot senders at the source. **Do not exclude `pr-tracker-bot`** — its
   "Pull request merged" mails are the merge signal for step 6.

4. **Rank & bucket.** Score each thread by subject keywords, `Mails:` volume
   (**>5** = hot), and notable maintainers in `Participants:` (Torvalds, Greg
   KH, akpm, Peter Zijlstra, tglx, Kicinski, Rafael, …). Drop bot traffic
   (syzbot, test robot) unless it's the story. Then spread picks across these
   buckets for breadth — don't let one hot subsystem dominate:

   - **core** — VFS, locking, generic kernel
   - **mm** — memory management
   - **scheduler** — sched core, load balancing, sched/pm interaction
   - **pm / power** — pm, cpufreq, cpuidle, thermal, OPP, suspend/resume
   - **pci** — pci, pcie, hotplug, ASPM
   - **usb** — usb, xhci, dwc3, typec
   - **net** — net(-next), mptcp, dsa, bpf
   - **storage / fs** — nvme, scsi, block, ext4/btrfs/xfs, nfs
   - **other** — rust, tracing, kvm, …

   **3–5** threads per populated bucket. If a bucket has no hot thread, still
   pick one **general, non-platform-specific** mail (a `pci:` core change over
   a board DT patch; a `usb:` core fix over SoC phy glue). Skip a bucket only if
   truly empty — note the omission.

   Threads whose subject or head matches `regression`, `revert`, `bisect`,
   `KASAN`, `BUG:`, `WARNING:`, `Fixes:`, `stable@` — from a human, not a
   bot — always make the cut for their bucket, even when `Mails:` is 0.

5. **Phase 2 — fetch picks in full.** Per thread, take the `Commits:` list and
   keep the head (cover letter `0/N` or root) plus the replies you want — skip
   numbered `n/N` patch bodies unless the discussion is about one. If you need
   to tell members apart first, `--format compact` and match on `Thread:`.
   Pull the chosen commits in one call, **same window**, with `--no-diff` so
   patch mails stop at their first `diff --git` line:

   ```sh
   ./target/release/lkml-digest --list <LIST> --since <WINDOW> --no-diff \
       --select-commit <commit1>,<commit2>,…
   ```

   (Use `--select-msgid <id1>,<id2>,…` instead when picking by `Message-ID:`.)

   Prints `========`-separated blocks (headers, blank line, `--`, decoded body;
   `[diff omitted]` where hunks were cut).

6. **State tag.** Give every entry exactly one tag from this fixed set — no
   free-form status:

   | tag | when |
   |---|---|
   | 🆕 new | first posting (v1 / RFC / bare report) in the window |
   | 🔁 vN | a re-spin; N from `Series:` |
   | ✅ merged | **only** if the window holds a `pr-tracker-bot` "merged" mail for it, or a maintainer reply saying "Applied"/"queued"/"pulled" |
   | ❌ NAK | a maintainer explicitly rejected it |
   | ⚠️ regression | breakage / bisect / revert request |
   | ↩️ revert | a revert was posted or applied |
   | 💤 silent | patch thread with no reply in the window |
   | 💬 discussing | replies exist, none of the above applies |

   Never infer "merged" from the absence of objections. If you didn't see the
   merge signal in the window, it's 💬 or 💤.

7. **Summarize** into one self-contained HTML file and write it to
   `out/digest-<list>-<YYYY-MM-DD>-<lang>.html` (create `out/` if missing).
   Headings and prose in the chosen language; technical identifiers
   (functions, hashes, subjects, maintainer names) verbatim. Layout:

   - **One tab per bucket**, switched by the sticky `<nav>` at the top. Tabs
     are `<section id="sN">` inside `<main>`, shown via `:target` (pure CSS,
     no JS), one per bucket in the order from step 4. Skip a tab only if its
     bucket is truly empty.
   - **One `<details>` per thread.** The always-visible `<summary>` carries
     the plain-language title, then a `<small>` line: importance dot ·
     subsystem · state tag · mail count · **one or two sentences** that give
     the gist (what it is, where it stands). The full card (original subject,
     lore link, bullets) sits inside, collapsed by default.
   - **Progress is a nested bullet list**, not prose: 2–6 bullets, one fact
     each, bold lead word (`<b>Fix</b>:`, `<b>Reviewer</b>:`, `<b>Numbers</b>:`,
     `<b>State</b>:`), under ~20 words per bullet. Skim-first.
   - Titles are **plain-language rewrites** (what it does, not the tag
     soup); the original subject goes on the first line inside. HTML-escape
     `<` `>` `&` in subjects and Message-IDs (`&lt;id@host&gt;`).

   ```html
   <!doctype html>
   <html lang="en">                       <!-- zh: lang="zh-Hant" -->
   <head>
   <meta charset="utf-8">
   <meta name="viewport" content="width=device-width, initial-scale=1">
   <title>LKML Daily Digest — <YYYY-MM-DD></title>
   <style>
   body{font:16px/1.6 system-ui,sans-serif;max-width:60rem;margin:2rem auto;padding:0 1rem;color:#222;background:#fff}
   code{background:#f3f3f3;padding:.1em .3em}blockquote{color:#555;border-left:3px solid #ccc;margin:0;padding-left:1rem}
   nav{display:flex;flex-wrap:wrap;gap:.4rem;position:sticky;top:0;background:#fff;padding:.5rem 0;border-bottom:1px solid #ddd;margin-bottom:1rem}
   nav a{text-decoration:none;color:inherit;border:1px solid #bbb;border-radius:.5rem;padding:.25rem .8rem;background:#f6f6f6}nav a:hover{background:#e6e6e6}
   /* one selector per tab, s0..sN; the last line keeps the first tab lit when there is no hash */
   body:has(#s0:target) nav a[href="#s0"],body:has(#s1:target) nav a[href="#s1"],
   body:not(:has(main>section:target)) nav a[href="#s0"]{background:#222;color:#fff;border-color:#222}
   main>section{display:none}main>section:target{display:block}main:not(:has(section:target))>section:first-child{display:block}
   details{border-top:1px solid #ddd;padding:.6rem 0}summary{cursor:pointer;list-style:none}summary::before{content:"▸ ";opacity:.6}details[open]>summary::before{content:"▾ "}
   summary small{color:#555;font-weight:400}
   @media(prefers-color-scheme:dark){body{background:#111;color:#ddd}code{background:#222}nav{background:#111;border-color:#444}nav a{background:#1c1c1c;border-color:#555}nav a:hover{background:#2a2a2a}details{border-color:#444}summary small{color:#aaa}
   body:has(#s0:target) nav a[href="#s0"],body:has(#s1:target) nav a[href="#s1"],body:not(:has(main>section:target)) nav a[href="#s0"]{background:#eee;color:#111;border-color:#eee}}
   </style>
   </head>
   <body>
   <h1>Linux Kernel Mailing List Daily Digest — <YYYY-MM-DD></h1>
   <blockquote>Window: <start> — <end> UTC · list: <list> · <N> mails / <T> threads</blockquote>

   <nav><a href="#s0">core</a><a href="#s1">mm</a> …</nav>
   <main>

   <section id="s0">
   <h2><Subsystem></h2>

   <details id="t1">
   <summary><b><plain-language title></b><br><small>🔴 <subsystem> · <tag> · <N> mails — one or two sentences: what it is and where it stands.</small></summary>
   <p><code><original English subject></code><br>
   <a href="https://lore.kernel.org/<list>/<thread root Message-ID without brackets>/">https://lore.kernel.org/<list>/<root id>/</a></p>
   <ul>
   <li><b>Importance</b>: 🔴 High / 🟡 Medium / 🟢 Low · <b>State</b>: <tag></li>
   <li><b>Topic</b>: 1–2 sentences on what's being discussed.</li>
   <li><b>Progress</b>: <ul>
     <li><b>Lead word</b>: one fact per bullet, short, scannable</li>
     <li><b>Reviewer</b>: what they said / asked for</li>
     <li><b>State</b>: what happens next</li>
   </ul></li>
   <li><b>Key participants</b>: A, B, C</li>
   <li><b>Deep dive</b>: <code>/lkml-summary &lt;head Message-ID&gt; <lang> <list></code></li>
   </ul>
   </details>
   </section>

   </main>
   </body>
   </html>
   ```

   Traditional Chinese (`zh`): same skeleton with `lang="zh-Hant"` and these
   labels (state tags keep their English word):

   | en | zh |
   |---|---|
   | Linux Kernel Mailing List Daily Digest — | Linux Kernel Mailing List 每日摘要 — |
   | Window: … · list: … · N mails / T threads | 涵蓋時間：… · 來源：… · N 封信 / T 個討論串 |
   | N mails (summary line) | N 封 |
   | Importance: High / Medium / Low · State | 重要性：高 / 中 / 低 · 狀態 |
   | Topic · Progress · Key participants · Deep dive | 核心議題 · 進展 · 主要參與者 · 深入閱讀 |

   Times are printed as the CLI gives them (UTC); don't convert. The link
   uses the `Thread:` root id so it opens the whole discussion on lore; the
   deep-dive line uses the head `Message-ID:` (angle brackets included, escaped)
   so it pastes straight into `lkml-summary`.

   Importance dots map to the ranking above: 🔴 high = strong keyword hit
   **and** high mail count or notable maintainer; 🟡 medium = one strong
   signal; 🟢 low = included for breadth or because the user asked.

8. **Reply** with the file path and one line per tab (`tab — N cards`) —
   not the HTML itself.

## Notes

- The CLI walks **every epoch the window touches** and clones older epochs on
  demand, so a wide window is slow (each lkml epoch is a few hundred MB) but
  complete. If the CLI prints a `warning:` that the earliest epoch starts after
  the window, that tail is off lore — tell the user.
