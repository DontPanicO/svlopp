use std::os::fd::AsFd;

use rustix::event::epoll;

use svloop::{
    cvt,
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
                        if signo.cast_signed() == libc::SIGINT
                            || signo.cast_signed() == libc::SIGTERM
                        {
                            eprintln!("Shutdown requested");
                            break 'outer;
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
