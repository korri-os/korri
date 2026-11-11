//! Native Sway protocol peer for existing file-driven compositor scenarios.
//! Scenario scripts run on the peer side, never inside the production adapter.

use super::compositor_focus::{CompositorControl, SwayCompositorControl};
use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Debug)]
pub(crate) struct SwayTestPeer {
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
}

impl SwayTestPeer {
    pub(crate) fn start(program: PathBuf, socket: PathBuf) -> Self {
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&stop);
        let task = thread::spawn(move || {
            let mut clients = Vec::new();
            while !done.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let program = program.clone();
                        let socket = socket.clone();
                        let done = Arc::clone(&done);
                        clients.push(thread::spawn(move || {
                            serve(stream, &program, &socket, &done)
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("Sway test peer accept: {error}"),
                }
            }
            for client in clients {
                client.join().unwrap();
            }
        });
        Self {
            stop,
            task: Some(task),
        }
    }
}

impl Drop for SwayTestPeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(task) = self.task.take() {
            task.join().unwrap();
        }
    }
}

fn serve(mut stream: UnixStream, program: &PathBuf, socket: &PathBuf, stop: &AtomicBool) {
    stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    while !stop.load(Ordering::Acquire) {
        let mut header = [0; 14];
        match stream.read(&mut header[..1]) {
            Ok(0) => return,
            Ok(_) => (),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(_) => return,
        }
        stream.read_exact(&mut header[1..]).unwrap();
        assert_eq!(&header[..6], b"i3-ipc");
        let length = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
        assert!(length <= 4 * 1024 * 1024);
        let kind = u32::from_le_bytes(header[10..].try_into().unwrap());
        let mut payload = vec![0; length];
        stream.read_exact(&mut payload).unwrap();
        let mut command = Command::new(program);
        command.arg("-s").arg(socket);
        match kind {
            0 => {
                command.arg("--").arg(String::from_utf8(payload).unwrap());
            }
            3 => {
                command.args(["-r", "-t", "get_outputs"]);
            }
            4 => {
                command.args(["-r", "-t", "get_tree"]);
            }
            _ => panic!("unexpected Sway test request {kind}"),
        }
        let output = command.output().unwrap();
        if !output.status.success() {
            return;
        }
        let mut reply = b"i3-ipc".to_vec();
        reply.extend_from_slice(&(output.stdout.len() as u32).to_le_bytes());
        reply.extend_from_slice(&kind.to_le_bytes());
        reply.extend_from_slice(&output.stdout);
        if stream.write_all(&reply).is_err() {
            return;
        }
    }
}

#[derive(Debug)]
pub(crate) struct ScriptedSwayControl {
    control: SwayCompositorControl,
    _peer: SwayTestPeer,
}

impl ScriptedSwayControl {
    pub(crate) fn new(program: PathBuf, socket: PathBuf, timeout: Duration) -> Option<Self> {
        let control = SwayCompositorControl::new(socket.clone(), timeout)?;
        Some(Self {
            control,
            _peer: SwayTestPeer::start(program, socket),
        })
    }
}

impl CompositorControl for ScriptedSwayControl {
    fn tree(&self) -> Result<String, String> {
        self.control.tree()
    }
    fn outputs(&self) -> Result<String, String> {
        self.control.outputs()
    }
    fn command(&self, command: &str) -> Result<(), String> {
        self.control.command(command)
    }
    fn focus(&self, id: i64) -> Result<(), String> {
        self.control.focus(id)
    }
    fn focus_game(&self, id: i64) -> Result<(), String> {
        self.control.focus_game(id)
    }
}
