use std::{env, os::fd::AsFd};

use rustix::event::epoll;

use svloop::{
    service::{
        handle_sigchld, start_service, stop_service, Service,
        ServiceConfigData, ServiceRegistry,
    },
    signalfd::{
        block_thread_signals, read_signalfd_all, signalfd, SigSet,
        SignalfdFlags,
    },
    timerfd::{create_timerfd_1s_periodic, read_timerfd},
    SupervisorState,
};

const ID_SFD: u64 = 1;
const ID_TFD: u64 = 2;

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

    let mut sigset = SigSet::empty()?;
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

    let mut service_registry = ServiceRegistry::new();
    let service_configs = ServiceConfigData::from_config_file(&cfg_file_path)?;

    for (i, (name, cfg)) in service_configs.services.into_iter().enumerate() {
        service_registry.insert_service(Service::new(i as u64, name, cfg)?);
    }

    for svc_id in 0..(service_registry.services().len() as u64) {
        if let Some(svc) = service_registry.service_mut(svc_id) {
            match start_service(svc) {
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

    eprintln!(
        "supervisor core started (epoll + signalfd + timerfd). Ctrl+C to exit."
    );

    // Vec is uninit but `epoll_wait` will write to it.
    // As `epoll_wait` returns the number of bytes to read
    // accesses up to that index are safe.
    let mut events: Vec<epoll::Event> = Vec::with_capacity(16);
    #[allow(clippy::uninit_vec)]
    unsafe {
        events.set_len(16);
    }

    'outer: loop {
        let n = epoll::wait(&epfd, &mut events, None)?;

        for ev in &events[..n as usize] {
            match ev.data.u64() {
                ID_SFD => {
                    for info in read_signalfd_all(sfd.as_fd())? {
                        let signo = info.signal();
                        eprintln!("signal: {}", signo);
                        if signo.cast_signed() == libc::SIGCHLD {
                            handle_sigchld(&mut service_registry)?;
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
                    let exps = read_timerfd(tfd.as_fd())?;
                    eprintln!("timer fired (expirations={})", exps);
                }
                other => eprintln!("unknown epoll event id={}", other),
            }
        }
    }

    Ok(())
}
