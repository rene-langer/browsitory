//! A `git://` server that answers a fetch's connect and then goes silent (#1064): a transfer
//! that stalls after connecting, as when the network goes away mid-fetch without a reset, but
//! on the loopback interface and at once.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// The one branch the server advertises: an object no test repository has, so a fetch always
/// asks for it.
pub const STALLED_BRANCH_TIP: &str = "1111111111111111111111111111111111111111";

/// Answers each connection's request with a ref advertisement (`refs/heads/main` at
/// [`STALLED_BRANCH_TIP`]), so the client connects, then reads whatever the client sends and
/// never answers. Dropping it closes every connection, so a client still waiting fails.
pub struct StalledServer {
    /// `git://127.0.0.1:<port>/repo.git`.
    pub url: String,
    /// Gets a message each time a client sent something after the advertisement: it is past
    /// its connect and now waits for an answer that never comes.
    pub stalled: Receiver<()>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StalledServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let url = format!(
            "git://127.0.0.1:{}/repo.git",
            listener.local_addr().expect("address").port()
        );
        let (stalled_tx, stalled) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let mut connections: Vec<(TcpStream, JoinHandle<()>)> = Vec::new();
            while !stopped.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        // Accepted sockets inherit the listener's non-blocking mode on some
                        // platforms; the connection's thread reads blocking.
                        stream.set_nonblocking(false).expect("blocking");
                        let held = stream.try_clone().expect("clone stream");
                        let stalled_tx = stalled_tx.clone();
                        let handler = thread::spawn(move || serve(stream, &stalled_tx));
                        connections.push((held, handler));
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
            for (stream, handler) in connections {
                let _ = stream.shutdown(Shutdown::Both);
                let _ = handler.join();
            }
        });
        Self {
            url,
            stalled,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for StalledServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn pkt_line(payload: &str) -> String {
    format!("{:04x}{payload}", payload.len() + 4)
}

fn serve(mut stream: TcpStream, stalled: &Sender<()>) {
    // The request: one pkt-line (`git-upload-pack /repo.git\0host=…\0`).
    let mut length = [0u8; 4];
    if stream.read_exact(&mut length).is_err() {
        return;
    }
    let Some(length) = std::str::from_utf8(&length)
        .ok()
        .and_then(|hex| usize::from_str_radix(hex, 16).ok())
    else {
        return;
    };
    let mut request = vec![0u8; length.saturating_sub(4)];
    if stream.read_exact(&mut request).is_err() {
        return;
    }
    let advertisement = format!(
        "{}0000",
        pkt_line(&format!(
            "{STALLED_BRANCH_TIP} refs/heads/main\0side-band-64k ofs-delta\n"
        ))
    );
    if stream.write_all(advertisement.as_bytes()).is_err() {
        return;
    }
    let mut buffer = [0u8; 4096];
    let mut told = false;
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 {
            return;
        }
        if !told {
            told = true;
            let _ = stalled.send(());
        }
    }
}
