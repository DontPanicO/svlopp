# svlopp

svlopp is a Linux only, event driven process supervisor built around a single epoll loop.

**Alpha Software.**

svlopp is under active development and is not production ready. It currently targets contributors and experimentations rather than practical deployments.

## What is this?

Right now, svlopp is a Linux process supervisor that lives in user space: you define a set of processes and svlopp
starts them, tracks them, and reaps them when they exit. The long term goal is to make svlopp a complete yet minimal
init system for Linux.

## Design Principles

While svlopp is a recent project, it was preceded by a lot of reading about init systems - source code, docs, articles - to the
point where I had already formed strong opinions on the topic before starting to write the code. However, my opinions on init
systems in general might not always reflect one to one in svlopp.
One example of this is portability: while I'm firmly convinced that's important, I've found the Unix world already well served,
leading me to sacrifice portability in order to optimize svlopp for a single target platform - Linux. That drove some of the other
principles. svlopp uses a `signalfd` for signal handling, receiving signals as events, which makes it convenient to have a single
event loop rather than having a *per process* watcher approach.
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

[Architecture overview - epoll, signalfd, service lifecycle, reload mechanism]

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
command = "/usr/local/bin/my_daemon"
args = ["--config", "/etc/my_daemon.conf"]
```

Run svlopp:
```
./target/release/svlopp services.toml
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
file, and service definition support just the bare minimum needed to launch a process: a service
name, a command, and its arguments:
```toml
[service.service_name]
command = "service_bin"
args = ["service", "options"]
```

svlopp in still in its early stages, and the configuration format should be expected to evolve.
Service definitions will likely expand beyond what is currently available, and the overall
configuration structure may change as new features are introduced.

## Project Status

### Working

- Basic service definition
- Configuration parsing
- Process spawning and lifecycle tracking
- Event-driven supervision
- Child process reaping
- Graceful shutdown
- Static configuration reload

### Not yet implemented / still thinking about

- Reaping of orphaned descendant processes (`PR_SET_CHILD_SUBREAPER`)
- Useful logging
- pidfd based process management
- Extend service definition

## Limitations & Known Issues

svlopp is still in an early stage, and several important pieces are either missing or incomplete:
- svlopp is currently a user space process supervisor, and a bare-bones one at that
- subreaping support is still missing
- Restart behavior is minimal and not configurable. Currently, to restart a stopped service one
  should first remove the service from the TOML configuration, send `SIGHUP`, add it back and send
  `SIGHUP` again
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
