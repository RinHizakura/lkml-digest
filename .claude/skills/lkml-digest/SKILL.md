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

1. **`--format compact`** prints one short metadata record per mail (subject,
   sender/recipient, time, reply count, Message-ID, commit) — no bodies. Scan
   this cheaply to decide what's worth reading.
2. **`--select-commit <commit>,…`** (or **`--select-msgid <id>,…`**) then
   re-fetches *only* the chosen mails in full, by the `Commit:` ids (read from
   the local mirror) or `Message-ID:` values (fetched from lore) the compact pass
   handed you. This keeps the expensive full-body read scoped to the handful of
   threads you'll actually summarize.

## Steps

1. **Build.** Always run `cargo build --release` from the repo
   root first, so the binary tracks the latest source (it's a no-op rebuild
   when already current).

2. **List & window.** Default `lkml` / `--since 24h`. "this week" → `7d`,
   "today" → `--range today`. Override on user request.

3. **Phase 1 — compact scan** (metadata only, no bodies):

   ```sh
   ./target/release/lkml-digest --list <LIST> --since <WINDOW> --format compact \
       --exclude-from syzbot,lkp@intel.com
   ```

   Output is a `# … count=N` header, then blank-line-separated records of
   `Subject / From / To / Date / Replies / Message-ID / Thread / Commit`.
   `Replies` = transitive in-window reply count for that subtree. `Thread` =
   the root Message-ID of the thread this mail belongs to (equals its own
   `Message-ID` when the mail is itself the root). `--exclude-from` drops bot
   senders at the source; leave it off when a bot report *is* the story.

4. **Rank & bucket.** Group records by `Thread:` — every mail of a series or
   discussion shares one root id, so don't parse `Subject:` for `Re:` /
   `[PATCH vN M/K]` to rebuild threads. A `Thread:` whose root isn't in the
   window means the discussion started earlier; treat the oldest in-window
   record as its head. Score by subject keywords, reply volume (**>5** = hot),
   and notable maintainers in `From:` (Torvalds, Greg KH, akpm, Peter Zijlstra,
   tglx, Kicinski, Rafael, …). Drop bot traffic (syzbot, test robot) unless it's
   the story. Then spread picks across these buckets for breadth — don't let one
   hot subsystem dominate:

   - **core** — VFS, locking, generic kernel
   - **mm** — memory management
   - **scheduler** — sched core, load balancing, sched/pm interaction
   - **pm / power** — pm, cpufreq, cpuidle, thermal, OPP, suspend/resume
   - **pci** — pci, pcie, hotplug, ASPM
   - **usb** — usb, xhci, dwc3, typec
   - **net** — net(-next), mptcp, dsa, bpf
   - **storage / fs** — nvme, scsi, block, ext4/btrfs/xfs, nfs
   - **other** — rust, tracing, kvm, …

   **3–5** mails per populated bucket. If a bucket has no hot thread, still pick
   one **general, non-platform-specific** mail (a `pci:` core change over a board
   DT patch; a `usb:` core fix over SoC phy glue). Skip a bucket only if truly
   empty — note the omission.

5. **Phase 2 — fetch picks in full.** Per thread, pick the cover letter
   (`0/N`) or root plus the replies with `Replies > 0` or from a maintainer;
   skip numbered `n/N` patch bodies unless the discussion is about one. Pull
   the chosen `Commit:` ids in one call, **same window**, with `--no-diff` so
   patch mails stop at their first `diff --git` line:

   ```sh
   ./target/release/lkml-digest --list <LIST> --since <WINDOW> --no-diff \
       --select-commit <commit1>,<commit2>,…
   ```

   (Use `--select-msgid <id1>,<id2>,…` instead when picking by `Message-ID:`.)

   Prints `========`-separated blocks (headers, blank line, `--`, decoded body;
   `[diff omitted]` where hunks were cut).

6. **Summarize** by bucket with the language template below. Headings and prose
   in the chosen language; technical identifiers (functions, hashes, subjects,
   maintainer names) verbatim.

   English (`en`):

   ```
   # Linux Kernel Mailing List Daily Digest — <YYYY-MM-DD>

   > Window: <start> — <end> UTC · list: <list> · <N> mails

   ## 🔴 Today's Highlights
   3–5 sentences on the day's most notable discussions or technical trends.

   ## <Subsystem>

   ### <original English subject>
   <https://lore.kernel.org/<list>/<thread root Message-ID without brackets>/>
   - **Importance**: 🔴 High / 🟡 Medium / 🟢 Low
   - **Topic**: 1–2 sentences on what's being discussed.
   - **Progress**: patch state, point of contention, or conclusion.
   - **Key participants**: A, B, C
   ```

   Traditional Chinese (`zh`):

   ```
   # Linux Kernel Mailing List 每日摘要 — <YYYY-MM-DD>

   > 涵蓋時間：<起始> — <結束> UTC · 來源：<list> · <N> 封信

   ## 🔴 今日亮點
   3–5 句話，說明當天最值得關注的討論或技術趨勢。

   ## <子系統 / List 名稱>

   ### <英文 subject 原文>
   <https://lore.kernel.org/<list>/<thread root Message-ID without brackets>/>
   - **重要性**：🔴 高 / 🟡 中 / 🟢 低
   - **核心議題**：（1–2 句，說明在討論什麼問題）
   - **進展**：（patch 狀態、爭議點、或結論）
   - **主要參與者**：A、B、C
   ```

   Times are printed as the CLI gives them (UTC); don't convert. The link
   uses the `Thread:` root id so it opens the whole discussion on lore.

   Importance dots map to the ranking above: 🔴 high = strong keyword hit
   **and** high reply count or notable maintainer; 🟡 medium = one strong
   signal; 🟢 low = included for breadth or because the user asked.

## Notes

- The CLI walks **every epoch the window touches** and clones older epochs on
  demand, so a wide window is slow (each lkml epoch is a few hundred MB) but
  complete. If the CLI prints a `warning:` that the earliest epoch starts after
  the window, that tail is off lore — tell the user.
