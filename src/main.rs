// SPDX-License-Identifier: GPL-2.0

use std::collections::HashSet;
use std::io::Write;
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{Duration, Utc};
use clap::{Parser, ValueEnum};

use lkml_core::archive;
use lkml_core::filter::{DateFilter, DateRange, Filter, MsgidFilter};
use lkml_core::mail::{self, Mail};
use lkml_core::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Format {
    /// Full mail blocks: headers plus decoded body, separated by `========`.
    Full,
    /// One compact metadata record per mail (subject, sender/recipient, time,
    /// reply count, Message-ID, commit). Bodies are omitted so a reader can
    /// cheaply scan and pick which mails to fetch in full.
    Compact,
}

#[derive(Parser, Debug)]
#[command(
    name = "lkml-digest",
    version,
    about = "Dump filtered mails from a local lore.kernel.org mirror as plain text."
)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "lkml",
        help = "Mailing list name (e.g. lkml, linux-pm)."
    )]
    list: String,

    #[arg(
        long,
        default_value = "24h",
        help = "Time window ending now, like 30m, 24h, 7d. Ignored if --range is set."
    )]
    since: String,

    #[arg(
        long,
        help = "Explicit range: 'today', 'yesterday', or 'YYYY/MM/DD HH:MM to YYYY/MM/DD HH:MM'."
    )]
    range: Option<String>,

    #[arg(long, default_value_t = 0, help = "Cap matching mails (0 = no cap).")]
    limit: usize,

    #[arg(
        long,
        value_enum,
        default_value_t = Format::Full,
        help = "Output format: 'full' mail bodies, or 'compact' metadata for filtering."
    )]
    format: Format,

    #[arg(
        long = "select-commit",
        value_delimiter = ',',
        help = "Pick mails by commit id (comma-separated, repeatable)"
    )]
    select_commit: Vec<String>,

    #[arg(
        long = "select-msgid",
        value_delimiter = ',',
        help = "Pick mails by Message-ID (comma-separated, repeatable)"
    )]
    select_msgid: Vec<String>,
}

/// Parse `30m`, `24h`, `7d` into a chrono Duration.
fn parse_since(s: &str) -> Result<Duration> {
    let (n_str, unit) = s.split_at(s.len().saturating_sub(1));
    let n: i64 = n_str
        .parse()
        .with_context(|| format!("invalid duration '{s}' (expected like 24h)"))?;
    if n <= 0 {
        bail!("duration must be positive: '{s}'");
    }
    match unit {
        "m" => Ok(Duration::minutes(n)),
        "h" => Ok(Duration::hours(n)),
        "d" => Ok(Duration::days(n)),
        _ => bail!("duration unit must be m/h/d (e.g. 24h)"),
    }
}

fn resolve_filter(args: &Args) -> Result<DateFilter> {
    let mut filter = DateFilter::new();
    if let Some(text) = &args.range {
        filter.set(text)?;
    } else {
        let end = Utc::now();
        let start = end - parse_since(&args.since)?;
        filter.date_range = Some(DateRange { start, end });
    }
    Ok(filter)
}

/// Drop later mails that repeat an already-seen commit id. Commit ids are
/// unique per mail, so this collapses the same commit picked more than once.
fn dedup_mails(mails: &mut Vec<Mail>) {
    let mut seen: HashSet<String> = HashSet::new();
    mails.retain(|m| m.commit.is_empty() || seen.insert(m.commit.clone()));
}

/// Render the searched epochs newest-first for the header line, e.g. `19,18`.
fn fmt_epochs(epochs: &[u32]) -> String {
    let mut e: Vec<u32> = epochs.to_vec();
    e.sort_unstable_by(|a, b| b.cmp(a));
    e.iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn header_line(args: &Args, epochs: &[u32], window: &str, count: usize) -> String {
    format!(
        "# lkml-digest list={} epoch={} window={} count={}",
        args.list,
        fmt_epochs(epochs),
        window,
        count
    )
}

/// How many mails one git process reads. Reading a mail costs a git process far
/// more than it costs to read the blob, so mails are read in batches; chunked
/// rather than all at once so a wide window's raw text is not all held together.
const READ_CHUNK: usize = 256;

/// Fetch a mail by commit id, trying each mirrored epoch in turn. Commit ids
/// are scoped to one epoch's repo and the CLI only carries the bare id, so a
/// `--select-commit` may live in any epoch the window now spans.
fn fetch_commit_any(list: &str, epochs: &[u32], commit: &str) -> Result<Mail> {
    let want = [commit.to_string()];
    let mut last_err = None;
    for &epoch in epochs {
        // An epoch that simply does not hold this commit reads as no mails, not
        // as an error; only a mirror that will not be read at all is an error.
        match mail::fetch(list, epoch, &want) {
            Ok(mails) => {
                if let Some(mail) = mails.into_iter().next() {
                    return Ok(mail);
                }
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err
        .unwrap_or_else(|| anyhow!("commit {commit} is not in any mirrored epoch of the window")))
}

/// Every mail of the window across `epochs`, read in batches. Both ways of
/// picking mails — the whole window, or the ids selected out of it — walk the
/// same commits and differ only in what they keep, so they share the read.
fn read_window(list: &str, epochs: &[u32], range: &DateRange) -> Result<Vec<Mail>> {
    let mut mails = Vec::new();
    for &epoch in epochs {
        let commits = listall_commits(list, epoch, range)?;
        for chunk in commits.chunks(READ_CHUNK) {
            let read = mail::fetch(list, epoch, chunk)?;
            // A mail that will not read is dropped from the batch rather than
            // failing it; say how many, since the digest is then incomplete.
            if read.len() < chunk.len() {
                eprintln!(
                    "warning: {} mail(s) in epoch {epoch} could not be read",
                    chunk.len() - read.len()
                );
            }
            mails.extend(read);
        }
    }
    Ok(mails)
}

/// The window rendered in UTC, matching the per-mail `Date:` lines so a reader
/// can compare them directly (the mails' own offsets vary).
fn window_utc(range: &DateRange) -> String {
    format!(
        "{} to {} UTC",
        range.start.format("%Y/%m/%d %H:%M"),
        range.end.format("%Y/%m/%d %H:%M"),
    )
}

fn write_mails(args: &Args, mails: Vec<Mail>, epochs: &[u32], window: &str) -> Result<()> {
    let replies = thread::reply_counts(&mails);
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", header_line(args, epochs, window, mails.len()))?;
    writeln!(out)?;
    match args.format {
        Format::Full => {
            for (i, m) in mails.iter().enumerate() {
                if i > 0 {
                    writeln!(out, "\n========\n")?;
                }
                out.write_all(m.render_full().as_bytes())?;
            }
        }
        Format::Compact => {
            for (i, m) in mails.iter().enumerate() {
                if i > 0 {
                    writeln!(out)?;
                }
                writeln!(out, "Subject: {}", m.subject)?;
                writeln!(out, "From: {}", m.from)?;
                if !m.to.is_empty() {
                    writeln!(out, "To: {}", m.to)?;
                }
                match m.date {
                    Some(d) => writeln!(
                        out,
                        "Date: {} UTC",
                        d.with_timezone(&Utc).format("%Y/%m/%d %H:%M")
                    )?,
                    None => writeln!(out, "Date: -")?,
                }
                writeln!(out, "Replies: {}", replies[i])?;
                writeln!(out, "Message-ID: {}", m.message_id)?;
                writeln!(out, "Commit: {}", m.commit)?;
            }
        }
    }
    Ok(())
}

/// Trim and drop blank tokens from a comma-separated `--select-*` list.
fn clean_selects(raw: &[String]) -> Vec<String> {
    raw.iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// List the window: commits whose date falls in `range`. The start is padded a
/// little so mails whose committer/header dates disagree aren't lost; the date
/// filter still enforces the exact window per mail.
fn listall_commits(list: &str, epoch: u32, range: &DateRange) -> Result<Vec<String>> {
    let walk_since = range.start - Duration::hours(1);
    archive::list_commits_since(list, epoch, walk_since)
}

/// Walk the window once and keep the mails whose Message-ID matches one of the
/// requested ids — resolved entirely from the local mirror (no network). Ids
/// that match nothing in the window are reported as warnings.
fn select_by_msgid(
    list: &str,
    epochs: &[u32],
    range: &DateRange,
    date: &DateFilter,
    msgids: &[String],
) -> Result<Vec<Mail>> {
    if msgids.is_empty() {
        return Ok(Vec::new());
    }
    let filters: Vec<MsgidFilter> = msgids.iter().map(|m| MsgidFilter::new(m)).collect();
    let mails: Vec<Mail> = read_window(list, epochs, range)?
        .into_iter()
        .filter(|m| date.matches(m) && filters.iter().any(|f| f.matches(m)))
        .collect();
    for (sel, filter) in msgids.iter().zip(&filters) {
        if !mails.iter().any(|m| filter.matches(m)) {
            eprintln!("warning: message-id {sel}: not found in local mirror window");
        }
    }
    Ok(mails)
}

fn run() -> Result<()> {
    let args = Args::parse();

    // Resolve the window first (no network) so we know how far back to mirror.
    let filter = resolve_filter(&args)?;
    let range = filter
        .date_range
        .clone()
        .ok_or_else(|| anyhow!("no date range resolved"))?;
    let window = window_utc(&range);

    eprintln!("Updating mirror for '{}'…", args.list);
    let epochs = archive::ensure_epoch_by_time(&args.list, range.start)?;

    // If even the earliest mirrored epoch begins after the window start, the
    // tail of the window predates anything lore still carries — say so once
    // rather than silently returning a truncated result.
    if let Some(&oldest_epoch) = epochs.last() {
        if let Ok(Some(started)) = archive::epoch_start_date(&args.list, oldest_epoch) {
            if started > range.start {
                eprintln!(
                    "warning: window starts {} but the earliest mirrored epoch ({}) \
                     only goes back to {}; older mail is off lore (epoch 0 reached)",
                    range.start.format("%Y/%m/%d %H:%M"),
                    oldest_epoch,
                    started.format("%Y/%m/%d %H:%M"),
                );
            }
        }
    }

    let commit_selects = clean_selects(&args.select_commit);
    let msgid_selects = clean_selects(&args.select_msgid);
    let any_selected = !commit_selects.is_empty() || !msgid_selects.is_empty();

    let mut mails: Vec<Mail> = if !any_selected {
        // No selection: list the whole window across every spanned epoch and
        // keep the in-window mails.
        read_window(&args.list, &epochs, &range)?
            .into_iter()
            .filter(|m| filter.matches(m))
            .collect()
    } else {
        // Selection: build a mail vector from each source, concatenate, dedup.
        // Commit ids resolve directly; Message-IDs are matched while walking the
        // window (both come from the local mirror — no network).
        let mut from_commits: Vec<Mail> = Vec::new();
        for commit in &commit_selects {
            match fetch_commit_any(&args.list, &epochs, commit) {
                Ok(mail) => {
                    if filter.matches(&mail) {
                        from_commits.push(mail);
                    }
                }
                Err(e) => eprintln!("warning: commit {commit}: {e:#}"),
            }
        }
        let from_msgids = select_by_msgid(&args.list, &epochs, &range, &filter, &msgid_selects)?;
        let mut mails: Vec<Mail> = from_commits.into_iter().chain(from_msgids).collect();
        dedup_mails(&mut mails);
        mails
    };
    mails.sort_by_key(|m| m.date);
    if args.limit != 0 && mails.len() > args.limit {
        mails.truncate(args.limit);
    }

    write_mails(&args, mails, &epochs, &window)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
