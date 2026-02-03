// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{env, os::fd::AsFd, time::Instant};

use rustix::{
    event::epoll,
    process::{Pid, set_child_subreaper},
};

use svlopp::{
    SupervisorState,
    service::{
        Service, ServiceConfigData, ServiceIdGen, ServiceRegistry,
        ServiceState, force_kill_service, handle_sigchld, reload_services,
        start_service, stop_service,
    },
    signalfd::{
        SigSet, SignalfdFlags, SignalfdSiginfo, block_thread_signals,
        read_signalfd_batch, signalfd,
    },
    timerfd::{create_timerfd_1s_periodic, read_timerfd},
};

const ID_SFD: u64 = 1;
const ID_TFD: u64 = 2;
const SIGINFO_BUF_LEN: usize = 16;
const EVENTS_BUF_LEN: usize = 16;

fn main() -> std::io::Result<()> {
    let mut args = env::args().skip(1);
    let cfg_file_path = match args.next() {
        Some(path) => path,
        None => {
            eprintln!("usage: svlopp <config_file>");
            std::process::exit(1);
        }
    };

    let mut sv_state = SupervisorState::default();

    // set the `child subreaper` attribute. `rustix::process::set_child_subreaper`
    // takes an `Option<Pid>`, which is odd since the kernel expects a `long`
    // (non-zero sets the attribut, zero unsets it). Presumably this id done
    // because `None` maps to zero, while `rustix::process::Pid` guarantees a
    // non-zero value
    unsafe { set_child_subreaper(Some(Pid::from_raw_unchecked(1)))? };

    let original_sigset = SigSet::current()?;
    let mut sigset = SigSet::empty()?;
    sigset.add(libc::SIGHUP)?;
    sigset.add(libc::SIGCHLD)?;
    sigset.add(libc::SIGTERM)?;
    sigset.add(libc::SIGINT)?;
    block_thread_signals(&sigset)?;

    let sfd =
        signalfd(&sigset, SignalfdFlags::CLOEXEC | SignalfdFlags::NONBLOCK)?;

    let tfd = create_timerfd_1s_periodic()?;

    let epfd = epoll::create(epoll::CreateFlags::CLOEXEC)?;
    epoll::add(
        &epfd,
        &sfd,
        epoll::EventData::new_u64(ID_SFD),
        epoll::EventFlags::IN,
    )?;
    epoll::add(
        &epfd,
        &tfd,
        epoll::EventData::new_u64(ID_TFD),
        epoll::EventFlags::IN,
    )?;

    let mut service_id_generator = ServiceIdGen::new();
    let mut service_registry = ServiceRegistry::new();
    let service_configs = ServiceConfigData::from_config_file(&cfg_file_path)?;

    for (name, cfg) in service_configs.services.into_iter() {
        service_registry.insert_service(Service::new(
            service_id_generator.nextval().unwrap(),
            name,
            cfg,
        )?);
    }

    for svc_id in 0..(service_registry.services().len() as u64) {
        if let Some(svc) = service_registry.service_mut(svc_id) {
            match start_service(svc, &original_sigset) {
                Ok(()) => {
                    eprintln!(
                        "started service '{}' with pid {:?}",
                        svc.name, svc.pid
                    );
                    if let Some(pid) = svc.pid {
                        service_registry.register_pid(pid, svc_id);
                    }
                }
                Err(e) => {
                    eprintln!("failed to start service '{}': {}", svc.name, e)
                }
            }
        }
    }

    let mut siginfo_buf = [SignalfdSiginfo::empty(); SIGINFO_BUF_LEN];
    let mut events_buf = [epoll::Event {
        flags: epoll::EventFlags::empty(),
        data: epoll::EventData::new_u64(0),
    }; EVENTS_BUF_LEN];

    eprintln!(
        "supervisor core started (epoll + signalfd + timerfd). Ctrl+C to exit."
    );

    'outer: loop {
        let n = epoll::wait(&epfd, &mut events_buf, None)?;

        for ev in &events_buf[..n as usize] {
            match ev.data.u64() {
                ID_SFD => {
                    // TODO: if we want to make sure to drain `sfd`, we could call
                    // `read_signalfd_batch` in a loop until it returns 0
                    let siginfo_read =
                        read_signalfd_batch(sfd.as_fd(), &mut siginfo_buf)?;
                    for info in &siginfo_buf[..siginfo_read] {
                        let signo = info.signal();
                        if (signo.cast_signed() == libc::SIGHUP)
                            && (sv_state == SupervisorState::Running)
                        {
                            eprintln!("reload requested (SIGHUP)");
                            match reload_services(
                                &mut service_registry,
                                &cfg_file_path,
                                &mut service_id_generator,
                                &original_sigset,
                            ) {
                                Ok(()) => {
                                    eprintln!("finished reloading services")
                                }
                                Err(_) => eprintln!(
                                    "failed reading new configuration"
                                ),
                            }
                        }
                        if signo.cast_signed() == libc::SIGCHLD {
                            handle_sigchld(
                                &mut service_registry,
                                &original_sigset,
                            )?;
                            if (sv_state == SupervisorState::ShutdownRequested)
                                && service_registry
                                    .services()
                                    .all(|svc| svc.is_stopped())
                            {
                                eprintln!("all services stopped, exiting");
                                break 'outer;
                            }
                        }
                        if signo.cast_signed() == libc::SIGINT
                            || signo.cast_signed() == libc::SIGTERM
                        {
                            if sv_state == SupervisorState::Running {
                                eprintln!("shutdown requested");
                                sv_state = SupervisorState::ShutdownRequested;
                                for svc in service_registry.services_mut() {
                                    stop_service(svc)?;
                                }
                            }
                            if service_registry
                                .services()
                                .all(|svc| svc.is_stopped())
                            {
                                eprintln!("all services stopped, exiting");
                                break 'outer;
                            }
                        }
                    }
                }
                ID_TFD => {
                    // `timerfd` is currently unused, read just to drain it
                    let _ = read_timerfd(tfd.as_fd())?;
                    let now = Instant::now();
                    for svc in service_registry.services() {
                        match svc.state {
                            ServiceState::Stopping(kill_deadline)
                                if now >= kill_deadline =>
                            {
                                force_kill_service(svc)?;
                            }
                            _ => {}
                        }
                    }
                }
                other => eprintln!("unknown epoll event id={}", other),
            }
        }
    }

    Ok(())
}
