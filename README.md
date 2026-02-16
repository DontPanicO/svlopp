# svlopp

svlopp is a Linux only, event driven process supervisor built around a single epoll loop.

**Alpha Software.**

svlopp is under active development and is not production ready. It currently targets contributors and experimentations rather than practical deployments.

## What is this?

Right now, svlopp is a Linux process supervisor that lives in user space: you define a set of processes and svlopp
starts them, tracks them, and reaps them when they exit. The long term goal is to evolve svlopp into a minimal init
system for Linux, focused on the essential responsibilities of init.

## Design Principles

While svlopp is a recent project, it was preceded by a lot of reading about init systems - source code, docs, articles - to the
point where I had already formed strong opinions on the topic before writing any code.
svlopp largely reflects those opinions, with the single exception of portability: while I generally consider it important, I found
the Unix ecosystem already well served in that regard and chose instead to optimize svlopp for a single target platform - Linux.
That decision drove some of the other principles. svlopp uses a `signalfd` for signal handling, receiving signals as events, which
makes it convenient to have a single event loop rather than having a *per process* watcher approach.

Another important aspect is clarity. While I was reading the source code of other init systems / process supervisors, I noticed a
clear distinction between designs that were self explanatory and easy to follow locally - most of them - and those whose functionality
is hidden behind multiple layers of abstraction - a few -, requiring me to keep track of long call chains and data flows before even
stumbling across a syscall, to the point where I just gave up. svlopp tries to stay in the former category. That's not to scoff at
the latter: I have deep respect for *any* software that's running - and working - in production.
Something else I took away from my readings is the idea that - quoting the s6 author - *system software should stay the heck out of
the way*. That means to me that system software should not pollute the user space from both the perspective of other system software
it might interact with, and the user's point of view by not trying to reduce the spectrum of software they are able to run on their
machines.

This naturally leads to the last point: avoiding feature creep. Features that are not clearly in scope are better left out, and
the burden — or the pleasure — of addressing those concerns should be left to other software, built specifically for that purpose,
and better suited to solving that particular problem. That's something I see people - including myself - intuitively understanding
even before knowing that thing has a name and it's an idea well defined by the Unix philosophy.

## High-level architecture

svlopp runs a single epoll loop that currently watches a `signalfd`, `timerfd` and a control FIFO.

Signals are handled via `signalfd`, allowing them to be treated as regular events rather than asynchronous interruptions.
This makes it possible to have all the supervision logic driven by that loop, instead of creating per service monitoring
threads or processes.
The following signals are currently handled:
- **SIGCHLD**: one or more child processes exited, svlopp reaps all exited children
- **SIGHUP**: configuration reload, svlopp re-reads the configuration, diffs it against the current state and reconciles
- **SIGTERM / SIGINT**: graceful shutdown, svlopp sends `SIGTERM` to all running services and waits for them to exit

The `timerfd` fires periodically and is currently used to enforce shutdown deadlines, allowing svlopp to forcefully
terminate child processes that ignore `SIGTERM`.

On reload (`SIGHUP`), svlopp reads the configuration and reconciles it with the current runtime state: new services get
added and started, removed services get stopped and removed, and changed services get restarted with their updated
definition. Some of those actions can be performed immediately (for example, starting a new service or removing a stopped
one), while others have to be deferred until the service process has exited. This preserves a single reaping path through
`SIGCHLD` and keeps process lifecycle handling centralized and predictable.

svlopp maintains a runtime directory (by default `/run/svlopp`) that is expected to reside on a tmpfs and contains only
runtime generated state.

### Status file

svlopp maintains a status file in the runtime directory, which contains a snapshot of the current runtime
state, one line per service.
For running services:
`<name> <id> <state> <pid>`

For stopped services:
`<name> <id> <state> <stop_reason>`

The file is rewritten whenever the runtime state changes which makes it important for the runtime directory to reside on a tmpfs.

### Control FIFO

The control FIFO is a named pipe that accepts binary commands from external sources, to start stop and restart individual services.
The protocol uses fixed-size frames of 9 bytes. The first byte encodes the operation, and the remaining 8 bytes carry the service
id as a little-endian `u64`.
Service ids are published in the status file. Writers are expected to resolve service names to ids by reading it.

## Quick Start

Build svlopp with cargo:
```
cargo build --release
```

Create a configuration file:
```toml
[services.sleep_forever]
command = "sleep"
args = ["infinity"]

[services.my_daemon]
command = "/usr/local/bin/my_service"
args = ["--config", "/etc/my_service.conf"]
```

Run svlopp (the runtime directory defaults to `/run/svlopp`. Use `--run-dir PATH`):
```
./target/release/svlopp --run-dir /some/dir services.toml
```

To reload configuration, send `SIGHUP`:
```
kill -HUP $(pidof svlopp)
```

To shutdown gracefully, send `SIGTERM` or `SIGINT`:
```
kill -TERM $(pidof svlopp)
```

## Configuration

Configuration is required to define services. As of now svlopp is configured via a single TOML
file, and service definition support just the bare minimum needed to launch and supervise a process:
a service name, a command, its arguments, and an optional termination reaction:
```toml
[service.service_name]
command = "service_bin"
args = ["service", "options"]
on_exit = "Restart" # optional
```

Services are expected to run in the foreground. svlopp supervises the processes it starts and reaps
them directly; services that daemonize themselves, double-fork, or are explicitly backgrounded
(e.g. using `&`) will break supervision and are not supported.

The optional on exit field defines what svlopp should do after a service process exists.
It is a fallback action, taken only when no other explicit action is pending (for example after a
configuration reload triggered by `SIGHUP`).

Supported values are:

- `None` (default): do nothing. The service will not be restarted automatically, but svlopp will keep
it in memory, so it can be started again via an explicit command (not yet supported).
- `Restart`: restart the service after it exits.
- `Remove`: remove the service from supervision after it exits. A configuration reload (`SIGHUP`) is
required to start the service again.

If a service is stopped by svlopp itself (e.g during shutdown, configuration reload or - once
supported - via an explicit command) the fallback action is not taken.

svlopp in still in its early stages, and the configuration format should be expected to evolve.
Service definitions will likely expand beyond what is currently available, and the overall
configuration structure may change as new features are introduced.

## Project Status

### Working

- Basic service definition
- Configuration parsing
- Process spawning and lifecycle tracking
- Event-driven supervision
- Child processes reaping
- Subreaping of orhpaned descendant processes (`PR_SET_CHILD_SUBREAPER`)
- Graceful shutdown
- Static configuration reload
- Runtime control commands (stop/start/restart) via control FIFO
- Service status reporting with status file

### Not yet implemented / still thinking about

- Useful logging
- pidfd based process management
- Extend service definition

## Limitations & Known Issues

svlopp is still in an early stage, and several important pieces are either missing or incomplete:
- svlopp is currently a user space process supervisor, and a bare-bones one at that
- Subreaping support is minimal. Orphaned descendant are reaped, but no additional semantics (such
  as attribution to services) are currently implemented.
- Logging is very limited and poorly structured, if at all

These limitations are known and sometimes intentional at this stage. The focus so far has been on
process supervision before expanding other capabilities.

## Building

svlopp only builds on Linux. Make sure you have a recent Rust toolchain installed:
```
cargo build
```

For a release build:
```
cargo build --release
```

## Contributing

svlopp is in early development and I'm happy to have people look at it, poke at it, and share their thoughts.
If you find a bug or have an idea, open an issue. If you want to contribute code, feel free to open a PR, but
for anything non trivial it might still be worth opening an issue.

There's no formal contributing guide yet. That might change as the project matures.

## License

This project is licensed under the Mozilla Public License, version 2.0 (MPL-2.0).
