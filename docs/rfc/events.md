<!--
# SPDX-FileCopyrightText: 2026 aerynOS Developers
# SPDX-License-Identifier: MPL-2.0
-->

# RFC: Events instead of shell output in the moss library

## Summary

This proposal outlines how the moss library can stop printing and instead report
**typed tasks** to an `Observer` that the caller provides.

The CLI (and later boulder) turns those events into output, for example:

- progress bars
- JSON
- or a timing report at the end of a run

`tracing` goes back to being about diagnostics only.

```
moss lib ── task updates ──▶ Observer ─┬─▶ Human   (today's bars + messages)
                                       ├─▶ Json    (one line per event)
                                       └─▶ Timing  (task tree with durations)
```

## Why

* Today, the moss library lib prints:
  - progress bars
  - `println!`
  - column listings
  - ... and prompts in ~15 files
* boulder can't control how moss output looks; it gets whatever moss prints.
* Progress was pushed through `tracing` as an informal API (`event_type = "progress_*"`) with
  no schema.
* Library users have nothing typed to work with.

## Design Proposal

This Proposal outlines the various parts that makes the design tick.

It is important to note that it is a request for comments proposal,
and not a set of dictates.

### Tasks w/ example code proposal

A task is a type. Its phases, details and outcome are tied to it,
so the compiler rejects a task type that is wrong for the job it is being
asked to do.

```rust
pub trait Task {
    type Phase;    // what it is doing now
    type Detail;   // facts found along the way
    type Outcome;  // what it came to
}

pub struct IndexPackage { pub path: PathBuf }
pub enum IndexPackagePhase { Hashing, Indexing }

impl Task for IndexPackage {
    type Phase = IndexPackagePhase;
    type Detail = ();
    type Outcome = ();
}
```

### Reporting from the lib w/ example code proposal

```rust
let index_packages = sink.start_task(task::IndexPackages { count });
let total = index_packages.start_progress(Some(count), task::Unit::Items);

let index_package = index_packages.start_subtask(task::IndexPackage { path });
index_package.enter_phase(task::IndexPackagePhase::Hashing);
let bytes = index_package.start_progress(Some(size), task::Unit::Bytes);
// bytes.inc(n) as we hash...
index_package.enter_phase(task::IndexPackagePhase::Indexing);
index_package.complete_task(());   // dropping a handle early sends `Aborted`

total.inc(1);
```

Lower layers take a `&Sink` and don't care where they sit. The caller decides by passing a
**child sink**, so everything started from it nests under that task:

```rust
fstree::native::blit_root(.., &extract_package.child_sink())?;
```

### Events

```rust
pub enum Event {               // moss::event
    Task(task::Event),
    // one-off events (no lifecycle) go here directly later
}

pub enum task::Event {
    FetchPackage(Update<FetchPackage>),
    Blit(Update<Blit>),
    // ...one variant per task
}

pub enum task::Update<T: Task> {
    Started   { id, parent, task: T },
    Phase     { id, phase: T::Phase },
    Progress  { id, current, total, unit },
    Detail    { id, detail: T::Detail },
    Completed { id, outcome: T::Outcome },
    Aborted   { id },
}
```

```json
{"type":"task","task":"blit","update":"started","id":2,"parent":1,"data":{"total":185}}
```

### Subtask, phase, detail or outcome overview

| Use a | When it | Example |
|---|---|---|
| subtask | is work with its own lifetime (could have progress, phases, children) | extracting one stone |
| phase | is what *this* task is doing now, one at a time | hashing → indexing |
| detail | is a fact found or decided at one moment | stone skipped, no layouts |
| outcome | is what the task came to, once, at the end | path the index was written to |

### Observers w/ example code proposal

```rust
pub trait Observer: Send + Sync {
    fn event(&self, event: &Event);
}

// observers compose
let observer = Arc::new(observer::Tee::new([format.observer(), timing.clone()]));
```

Rules:

* **Called inline, so it must return quickly.** moss downloads on a single-thread runtime; a
  slow observer stalls them.
* **No long-lived threads.** Some operations (cloning into a container) need the process to be
  single threaded, so any thread an observer starts must stop when its task finishes. Slow work
  is buffered and done once moss returns.
* Must not call back into moss, enter an async runtime, or panic.
* Every task sends `Started` first and either `Completed` or `Aborted` last.
* Output shown *before* or *after* a call (listings, results) comes from return values, not
  events.

## CLI w/ example structure proposal

| Flag | Effect |
|---|---|
| `--format human` (default) | today's bars and messages |
| `--format json` | every event, one line each; progress throttled to 10 Hz per task |
| `--timing` | task tree with durations on stderr at the end; works with any format |

```
Timing
extract_packages 2 packages                      43.5ms
├─ extract_package nano-9.0-40-1-x86_64.stone    32.9ms
│  └─ blit 185 entries                            1.8ms
└─ extract_package htop-3.5.1-10-1-x86_64.stone   7.2ms
   └─ blit 17 entries                             0.9ms
```

### Next: a CLI `Shell` and `--quiet` structure proposal

`--format` picks the encoding; how much to show is a separate axis. A CLI `Shell` holds
format, verbosity and colour, owns the adapters, and also gates the CLI's own prints
(`shell.status(..)`, `shell.warn(..)`, `shell.result(..)`), so lib and CLI output follow one
policy.

| Output | normal | `-q` | `-v` | `--format json` |
|---|---|---|---|---|
| progress / status | shown | hidden | + details | every event |
| warnings | shown | shown | shown | as events |
| results (`pkg list`, …) | shown | shown | shown | JSON later |
| errors, `--timing` | stderr | stderr | stderr | stderr |
| prompts | asked | asked | asked | asked unless `-y` |

### Next: confirmations proposal w/ exmaple code

Prompts move out of the lib the same way, through a trait the CLI provides:

```rust
pub trait Prompter: Send + Sync {
    fn confirm(&self, confirm: &prompt::Confirm<'_>) -> io::Result<bool>;
}

if !client.prompter.confirm(&prompt::Confirm::Remove { packages: &packages })? {
    return Err(Error::Cancelled);
}
```

The request is typed data, not a string, so non-terminal consumers can use it.

### Next: shared adapters structure proposal

Move `cli` to a `moss_cli` crate, and carve `cli::shell` out into a `moss_shell` crate reused
by all moss callers, such as boulder.
