use std::{ffi::CString, os::fd::AsFd};

use rustix::event::epoll;

use svloop::{
    service::{
        handle_sigchld, start_service, stop_service, Service, ServiceIdGen,
        ServiceRegistry, ServiceState,
    },
    signalfd::{
        block_thread_signals, read_signalfd_all, signalfd, SigSet,
        SignalfdFlags,
    },
    timerfd::{create_timerfd_1s_periodic, read_timerfd},
};

const ID_SFD: u64 = 1;
const ID_TFD: u64 = 2;

fn main() -> std::io::Result<()> {
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

    let mut service_id_generator = ServiceIdGen::new();
    let mut service_registry = ServiceRegistry::new();
    service_registry.insert_service(Service::new(
        service_id_generator.nextval().unwrap(),
        "ping_google".to_owned(),
        vec![
            CString::new("ping").unwrap(),
            CString::new("8.8.8.8").unwrap(),
        ],
    ));
    service_registry.insert_service(Service::new(
        service_id_generator.nextval().unwrap(),
        "ping_cloudfare".to_owned(),
        vec![
            CString::new("ping").unwrap(),
            CString::new("1.1.1.1").unwrap(),
        ],
    ));

    for svc_id in 0..2 {
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
                            if service_registry
                                .iter_services()
                                .all(|svc| svc.state == ServiceState::Stopped)
                            {
                                eprintln!("all services stopped, exiting");
                                break 'outer;
                            }
                        }
                        if signo.cast_signed() == libc::SIGINT
                            || signo.cast_signed() == libc::SIGTERM
                        {
                            eprintln!("shutdown requested");
                            for svc in service_registry.iter_services_mut() {
                                stop_service(svc)?;
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
