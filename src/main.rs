// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{os::fd::AsFd, time::Instant};

use rustix::{
    event::epoll,
    process::{Pid, set_child_subreaper},
};

use svlopp::{
    SupervisorState, cli,
    control::{ControlError, create_control_fifo, read_control_command},
    service::{
        Service, ServiceConfigData, ServiceIdGen, ServiceRegistry,
        ServiceState, apply_control_op, force_kill_service, handle_sigchld,
        reload_services, start_service, stop_service,
    },
    signalfd::{
        SigSet, SignalfdFlags, SignalfdSiginfo, block_thread_signals,
        read_signalfd_batch, signalfd,
    },
    status::{StatusFilePath, write_status_file},
    timerfd::{create_timerfd_1s_periodic, read_timerfd},
};

const ID_SFD: u64 = 1;
const ID_TFD: u64 = 2;
const ID_PFD: u64 = 3;
const SIGINFO_BUF_LEN: usize = 16;
const EVENTS_BUF_LEN: usize = 16;
const CONTROL_PIPE_NAME: &str = "control";
const STATUS_FILE_PATH: &str = "status";

fn main() -> std::io::Result<()> {
    let args = cli::parse();
    let status_file_path =
        StatusFilePath::new(args.run_dir.join(STATUS_FILE_PATH));

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

    let (pfd, _wr_pfd) =
        create_control_fifo(&args.run_dir.join(CONTROL_PIPE_NAME))?;

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
    epoll::add(
        &epfd,
        &pfd,
        epoll::EventData::new_u64(ID_PFD),
        epoll::EventFlags::IN,
    )?;

    let mut siginfo_buf = [SignalfdSiginfo::empty(); SIGINFO_BUF_LEN];
    let mut events_buf = [epoll::Event {
        flags: epoll::EventFlags::empty(),
        data: epoll::EventData::new_u64(0),
    }; EVENTS_BUF_LEN];
    let mut status_buf = String::new();

    let mut service_id_generator = ServiceIdGen::new();
    let mut service_registry = ServiceRegistry::new();
    let service_configs =
        ServiceConfigData::from_config_file(&args.config_path)?;

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

    status_buf.clear();
    if service_registry.format_status(&mut status_buf).is_ok()
        && let Err(e) = write_status_file(&status_file_path, &status_buf)
    {
        eprintln!("failed to write status file: {}", e);
    }

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
                                &args.config_path,
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
                    status_buf.clear();
                    if service_registry.format_status(&mut status_buf).is_ok()
                        && let Err(e) =
                            write_status_file(&status_file_path, &status_buf)
                    {
                        eprintln!("failed to write status file: {}", e);
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
                ID_PFD => match read_control_command(pfd.as_fd()) {
                    Ok(Some(cmd)) => {
                        apply_control_op(
                            &mut service_registry,
                            cmd.service_id,
                            cmd.op,
                            &original_sigset,
                        )?;
                        status_buf.clear();
                        if service_registry
                            .format_status(&mut status_buf)
                            .is_ok()
                            && let Err(e) = write_status_file(
                                &status_file_path,
                                &status_buf,
                            )
                        {
                            eprintln!("failed to write status file: {}", e);
                        }
                    }
                    Ok(None) => {}
                    Err(ControlError::InvalidCommand(e)) => {
                        eprintln!("invalid command: {}", e);
                    }
                    Err(ControlError::Io(e)) => return Err(e),
                },
                other => eprintln!("unknown epoll event id={}", other),
            }
        }
    }

    Ok(())
}
