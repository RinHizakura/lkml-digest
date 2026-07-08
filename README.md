# lkml-digest

A tool that dumps mails from a lore.kernel.org mailing list as
plain text, designed to be piped into an AI summarizer
(e.g. Claude Code) for daily/weekly digests of
"what's hot on this list".

## Build & run

```sh
git submodule update --init   # fetch vendor/lkml-core on first checkout
cargo build --release

./target/release/lkml-digest --list lkml --since 24h
```

Or via the Makefile (this also invokes the Claude Code skill —
see [Skill integration](#skill-integration)):

```sh
make run-digest                      # defaults to LIST=lkml
make run-digest LIST=linux-mm
```

## CLI

```
lkml-digest [OPTIONS]

  -l, --list <LIST>      Mailing list name (default: lkml)
      --since <SINCE>    Time window ending now (default: 24h)
                         Examples: 30m, 24h, 7d
      --range <RANGE>    Explicit range (overrides --since):
                         'today' | 'yesterday' | 'YYYY/MM/DD HH:MM to YYYY/MM/DD HH:MM'
      --limit <LIMIT>    Cap matching mails (0 = no cap, default: 0)
      --format <FORMAT>  'full' (mail bodies, default) or 'compact' (metadata only)
      --select-commit <IDS>
                         Pick mails by commit id (comma-separated, repeatable)
      --select-msgid <IDS>
                         Pick mails by Message-ID (comma-separated, repeatable)
```

## Output format

Both formats start with the same one-line header
(`# lkml-digest list=… epoch=… window=… count=N`); only the per-mail body
differs. `epoch=` lists every epoch the window spanned, newest first
(e.g. `epoch=19,18` when the window reached back into the previous epoch). The format is intentionally LLM-friendly: stable separators, headers
up front, no terminal escape codes. The `window=` range is shown in **UTC**, and
so is the `compact` `Date:` line, so the two line up directly.

### `--format full` (default)

`count` mail blocks separated by `========`, each with full headers and the
decoded body:

```
# lkml-digest list=lkml epoch=20 window=2026/05/22 17:00 to 2026/05/23 17:00 count=347

From: …
Date: …
Subject: …
To: …
Cc: …
Message-ID: …

--

<body, with quoted-printable decoded>

========

From: …
…
```

### `--format compact`

One short record per mail (subject, sender/recipient, time, reply count,
Message-ID, commit) with **no bodies**, separated by a blank line — so a reader
can cheaply scan and pick which mails to fetch in full:

```
# lkml-digest list=lkml epoch=20 window=2026/05/22 17:00 to 2026/05/23 17:00 count=347

Subject: …
From: …
To: …
Date: …
Replies: …
Message-ID: …
Commit: …

Subject: …
```

`Replies:` is the number of mails in the window that reply to this one
transitively (its thread-subtree size minus itself), so a cover letter / thread
root reflects how hot the discussion is.

### Selecting specific mails

Feed the ids from a compact pass back in to fetch just those bodies. Use
`--select-commit` for the `Commit:` ids:

```sh
./target/release/lkml-digest --list lkml --since 7d \
    --select-commit 1bb7c4fb286ce48e679961b4cb6307fe1d0e271c,35d4d256c6f49962d091194b5a1082b2334bdf8c
```

or `--select-msgid` for the `Message-ID:` values:

```sh
./target/release/lkml-digest --list lkml --since 7d \
    --select-msgid '<20260603-foo@example.com>,<20260603-bar@example.com>'
```

## Notes

- The CLI walks **every epoch the window touches**, cloning older epochs on
  demand when `--since` / `--range` reaches before the oldest local mail. Coverage
  is decided by each epoch's earliest *commit* (archival) date, not the mails'
  `Date:` headers, so a few mis-dated mails can't fool it. A wide window therefore
  triggers progressively larger clones — each lkml epoch is a few hundred MB — and
  a slower walk, since every mail body in range is read.
- If even the list's first epoch starts after the window, the uncovered tail is
  simply unavailable (off lore); the CLI prints a `warning:` and returns what it has.
- Network is required on every run (manifest fetch + `git remote update`).
