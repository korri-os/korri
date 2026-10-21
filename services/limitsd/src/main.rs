use korri_limitsd::{serve_connection, Defaults, Limitsd, Roots};
use std::{
    env, io,
    os::{fd::FromRawFd, unix::net::UnixListener},
    path::PathBuf,
    time::{Duration, Instant},
};

fn run() -> Result<(), String> {
    if env::var("LISTEN_FDS").as_deref() != Ok("1")
        || env::var("LISTEN_PID")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            != Some(std::process::id())
    {
        return Err("expected one systemd activation socket".into());
    }
    // systemd owns the pathname, permissions and socket lifetime. No request
    // can select another socket or change the trusted startup configuration.
    let listener = unsafe { UnixListener::from_raw_fd(3) };
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let defaults: Defaults =
        serde_json::from_str(&env::var("LIMITSD_DEFAULTS").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let peer_uid: u32 = env::var("LIMITSD_PEER_UID")
        .map_err(|e| e.to_string())?
        .parse()
        .map_err(|_| "invalid LIMITSD_PEER_UID")?;
    let gpu_nodes = env::var("LIMITSD_GPU_NODES")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut service = Limitsd::start(
        Roots {
            sysfs: PathBuf::from("/sys"),
            cgroup: PathBuf::from("/sys/fs/cgroup"),
        },
        defaults,
        &gpu_nodes,
    )?;
    log::info!("device defaults applied; accepting korrid uid {peer_uid}");
    let mut last_reassert = Instant::now();
    loop {
        if let Err(error) = service.poll_session() {
            log::error!("session recovery: {error}");
        }
        if last_reassert.elapsed() >= Duration::from_secs(30) {
            if let Err(error) = service.reassert() {
                log::error!("reassert limits: {error}");
            }
            last_reassert = Instant::now();
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(error) = serve_connection(stream, peer_uid, &mut service) {
                    log::warn!("socket request: {error}");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(500))
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("accept: {error}")),
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if let Err(error) = run() {
        log::error!("{error}");
        std::process::exit(1);
    }
}
