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

   **Regressions first.** Separately collect every thread whose subject or
   head matches `regression`, `revert`, `bisect`, `KASAN`, `BUG:`, `WARNING:`,
   `Fixes:`, `stable@` — from a human, not a bot. These go into their own
   section regardless of bucket; a regression report is the story even when
   `Mails:` is 0.

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

7. **Summarize** with the language template below. Headings and prose in the
   chosen language; technical identifiers (functions, hashes, subjects,
   maintainer names) verbatim. The **index table comes first** and lists every
   entry that has a card below, one row each, in card order. Titles are
   **plain-language rewrites** (what it does, not the tag soup); the original
   subject goes on the line under the heading.

   English (`en`):

   ```
   # Linux Kernel Mailing List Daily Digest — <YYYY-MM-DD>

   > Window: <start> — <end> UTC · list: <list> · <N> mails / <T> threads

   ## At a glance

   | | Subsystem | What | State | Mails |
   |---|---|---|---|---|
   | 🔴 | mm | <plain-language title> | ⚠️ regression | 14 |
   | 🟡 | net | … | 🔁 v3 | 7 |

   ## 🔴 Today's Highlights
   3–5 sentences on the day's most notable discussions or technical trends.

   ## ⚠️ Regressions & reverts
   (cards as below; write "None reported in this window." if empty)

   ## 📌 <Subsystem>

   ### <plain-language title>
   `<original English subject>`
   <https://lore.kernel.org/<list>/<thread root Message-ID without brackets>/>
   - **Importance**: 🔴 High / 🟡 Medium / 🟢 Low · **State**: <tag>
   - **Topic**: 1–2 sentences on what's being discussed.
   - **Progress**: point of contention, or conclusion.
   - **Key participants**: A, B, C
   - **Deep dive**: `/lkml-summary <head Message-ID> <lang> <list>`
   ```

   Traditional Chinese (`zh`):

   ```
   # Linux Kernel Mailing List 每日摘要 — <YYYY-MM-DD>

   > 涵蓋時間：<起始> — <結束> UTC · 來源：<list> · <N> 封信 / <T> 個討論串

   ## 一眼掃完

   | | 子系統 | 內容 | 狀態 | 信數 |
   |---|---|---|---|---|
   | 🔴 | mm | <白話標題> | ⚠️ regression | 14 |
   | 🟡 | net | … | 🔁 v3 | 7 |

   ## 🔴 今日亮點
   3–5 句話，說明當天最值得關注的討論或技術趨勢。

   ## ⚠️ Regression 與 revert
   （卡片格式同下；沒有就寫「本時段無回報。」）

   ## 📌 <子系統 / List 名稱>

   ### <白話標題>
   `<英文 subject 原文>`
   <https://lore.kernel.org/<list>/<thread root Message-ID without brackets>/>
   - **重要性**：🔴 高 / 🟡 中 / 🟢 低 · **狀態**：<tag>
   - **核心議題**：（1–2 句，說明在討論什麼問題）
   - **進展**：（爭議點、或結論）
   - **主要參與者**：A、B、C
   - **深入閱讀**：`/lkml-summary <head Message-ID> <lang> <list>`
   ```

   Times are printed as the CLI gives them (UTC); don't convert. The link
   uses the `Thread:` root id so it opens the whole discussion on lore; the
   deep-dive line uses the head `Message-ID:` (angle brackets included) so it
   pastes straight into `lkml-summary`. State tags keep their English word
   in both languages.

   Importance dots map to the ranking above: 🔴 high = strong keyword hit
   **and** high mail count or notable maintainer; 🟡 medium = one strong
   signal; 🟢 low = included for breadth or because the user asked.

## Notes

- The CLI walks **every epoch the window touches** and clones older epochs on
  demand, so a wide window is slow (each lkml epoch is a few hundred MB) but
  complete. If the CLI prints a `warning:` that the earliest epoch starts after
  the window, that tail is off lore — tell the user.
