//! Single instance: later `mdeer` invocations hand their files to the first one
//! over a per-user local socket (named pipe on Windows, abstract socket on Linux).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use interprocess::local_socket::{
    GenericNamespaced, Listener, ListenerOptions, Stream, prelude::*,
};

pub fn socket_name() -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    format!("mdeer-{user}.sock")
}

/// Sends `files` (absolute paths) to a running instance; false if there is none.
pub fn forward(name: &str, files: &[PathBuf]) -> bool {
    let Ok(name) = name.to_ns_name::<GenericNamespaced>() else {
        return false;
    };
    let Ok(mut stream) = Stream::connect(name) else {
        return false;
    };
    let msg: String = files.iter().map(|p| format!("{}\n", p.display())).collect();
    stream.write_all(msg.as_bytes()).is_ok()
}

/// Claims the name. `None` if another process won the race; we then run unshared.
pub fn bind(name: &str) -> Option<Listener> {
    let name = name.to_ns_name::<GenericNamespaced>().ok()?;
    ListenerOptions::new().name(name).create_sync().ok()
}

/// Receives forwarded paths on a background thread, calling `wake` after each batch.
pub fn serve(listener: Listener, wake: impl Fn() + Send + 'static) -> Receiver<PathBuf> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for conn in listener.incoming().filter_map(Result::ok) {
            for line in BufReader::new(conn).lines().map_while(Result::ok) {
                if tx.send(PathBuf::from(line)).is_err() {
                    return;
                }
            }
            wake();
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarded_paths_arrive() {
        let name = format!("mdeer-test-{}.sock", std::process::id());
        assert!(!forward(&name, &[]), "nobody listening yet");
        let rx = serve(bind(&name).unwrap(), || {});
        let files = [PathBuf::from("/a b/x.md"), PathBuf::from("/y.md")];
        assert!(forward(&name, &files));
        let got: Vec<PathBuf> = (0..2)
            .map(|_| rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap())
            .collect();
        assert_eq!(got, files);
        assert!(bind(&name).is_none(), "name is taken");
    }
}
