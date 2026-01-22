# svlopp

svlopp is a Linux-only, event-driven process supervisor built around a single epoll loop.

**Alpha Software.**

svlopp is under active development and is not production-ready. It currently targets contributors and experimentations rather than practical deployments.

## What is this?

svlopp is a linux process supervisor

## Design Principles

While svlopp is a recent project, it was preceded by a lot of reading about init systems - source code, docs, articles - to the
point where I had already formed strong opinions on the topic before starting to write the code. However, my opinions on init
systems in general might not always reflect one-to-one in svlopp.
One example of this is portability: while I'm firmly convinced that's important, I've found the Unix world already well served,
leading me to sacrifice portability in order to optimize svlopp for a single target platform - Linux. That drove some of the other
principles. svlopp uses a `signalfd` for signal handling, receiving signals as events, which makes it convenient to have a single
event loop rather than having a *per-process* watcher approach.
Another important aspect is clarity. While I was reading the source code of other init systems / process supervisors, I noticed a
clear distinction between designs that were self-explanatory and easy to follow locally - most of them - and those whose functionality
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

[Minimal example to get someone running it - config file + command]

## Configuration

[Brief overview of the TOML format]

## Project Status

### Working
[Features currently working]

### Planned
[Features planned but still missing and/or planned improvements to existing features]

## Limitations & Known Issues

[Be upfront about what's missing or broken]

## Building

[Compilation instructions]

## Contributing

[How people can help - even just "open issues for now"]

## License

This project is licensed under the Mozilla Public License, version 2.0 (MPL-2.0).
