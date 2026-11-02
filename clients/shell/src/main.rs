//! `korri-shell`: see README.md.

use korri_shell::{
    backend::{Backend, Placement},
    config::{self, Connection},
    engine::Limits,
    input::native::NativeInput,
    korrid::Korrid,
    package::Package,
    shell::{Event, Options, Post, Shell},
    verify::{self, Cache},
};
use std::{
    cell::RefCell,
    path::PathBuf,
    process::ExitCode,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const USAGE: &str = "usage:
  korri-shell --packages DIR [--cache DIR] [--installed] [--surfaces ID,ID…] [--placement fullscreen|WxH] [--stats]
  korri-shell install --packages DIR [--cache DIR]
  korri-shell verify PACKAGE_DIR";

/// How long the shell waits for korrid at start; systemd then restarts it.
const KORRID_START_LIMIT: Duration = Duration::from_secs(60);

struct Arguments {
    command: Command,
    packages: Option<PathBuf>,
    cache: Option<PathBuf>,
    surfaces: Option<Vec<String>>,
    placement: Placement,
    stats: bool,
    /// Read the summaries `install` wrote; never verify at run time.
    installed: bool,
}

#[derive(PartialEq)]
enum Command {
    Run,
    Install,
    Verify(PathBuf),
}

fn arguments() -> Result<Arguments, String> {
    let mut args = std::env::args_os().skip(1).map(|a| {
        a.into_string()
            .map_err(|_| "arguments must be UTF-8".to_owned())
    });
    let mut parsed = Arguments {
        command: Command::Run,
        packages: None,
        cache: None,
        surfaces: None,
        placement: Placement::Fullscreen,
        stats: false,
        installed: false,
    };
    let mut first = true;
    while let Some(arg) = args.next() {
        let arg = arg?;
        let mut value = |name: &str| {
            args.next()
                .unwrap_or_else(|| Err(format!("{name} needs a value")))
        };
        match arg.as_str() {
            "install" if first => parsed.command = Command::Install,
            "verify" if first => parsed.command = Command::Verify(PathBuf::from(value("verify")?)),
            "--packages" => parsed.packages = Some(PathBuf::from(value("--packages")?)),
            "--cache" => parsed.cache = Some(PathBuf::from(value("--cache")?)),
            "--surfaces" => {
                parsed.surfaces = Some(value("--surfaces")?.split(',').map(str::to_owned).collect())
            }
            "--placement" => {
                parsed.placement = Placement::parse(&value("--placement")?)
                    .ok_or("--placement is fullscreen or WxH")?
            }
            "--stats" => parsed.stats = true,
            "--installed" => parsed.installed = true,
            _ => return Err(USAGE.to_owned()),
        }
        first = false;
    }
    Ok(parsed)
}

/// Delivers shell events through Slint's event loop.
struct EventLoopPost;

thread_local! {
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
}

impl Post for EventLoopPost {
    fn post(&self, event: Event) {
        let _ = slint::invoke_from_event_loop(move || {
            SHELL.with(|shell| {
                if let Ok(mut shell) = shell.try_borrow_mut() {
                    if let Some(shell) = shell.as_mut() {
                        shell.handle(event);
                    }
                }
            })
        });
    }
}

fn main() -> ExitCode {
    // No core dump may carry the in-memory capability (as clients/linux).
    let no_core = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: plain setrlimit on this process with a valid struct.
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &no_core) } != 0 {
        eprintln!("korri-shell: refusing to start: could not disable core dumps");
        return ExitCode::from(2);
    }
    // Contract 4: no root. Input comes from korrid, never /dev/input.
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("korri-shell: refusing to start: effective user is root");
        return ExitCode::from(2);
    }
    korri_shell::memory::fix_mmap_threshold();
    let arguments = match arguments() {
        Ok(arguments) => arguments,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    match run(arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("korri-shell: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Arguments) -> anyhow::Result<()> {
    let limits = Limits::default();
    if let Command::Verify(dir) = &arguments.command {
        let summary = verify::verify(&Package::at(dir)?, limits);
        println!("{}", serde_json::to_string_pretty(&summary)?);
        anyhow::ensure!(summary.pass, "{} failed verification", summary.surface_id);
        return Ok(());
    }
    let root = arguments
        .packages
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--packages is required\n{USAGE}"))?;
    let cache = Cache::new(
        &arguments
            .cache
            .clone()
            .or_else(Cache::default_root)
            .ok_or_else(|| {
                anyhow::anyhow!("no cache directory: set --cache, XDG_CACHE_HOME or HOME")
            })?,
    );
    let mut packages = Package::discover(&root)?;
    if let Some(order) = &arguments.surfaces {
        packages = order
            .iter()
            .filter_map(|id| packages.iter().find(|p| &p.id == id).cloned())
            .collect();
    }
    if arguments.command == Command::Install {
        for package in &packages {
            let summary = cache.install(package, limits)?;
            println!(
                "{} {}",
                package.id,
                if summary.pass {
                    "pass".to_owned()
                } else {
                    format!("fail: {}", summary.reasons.join("; "))
                }
            );
        }
        return Ok(());
    }

    let connection = Connection::from_environment().map_err(anyhow::Error::msg)?;
    config::scrub_environment();
    let loadable = if arguments.installed {
        cache.installed(&packages)
    } else {
        cache.loadable(&packages, limits)?
    };
    anyhow::ensure!(
        !loadable.is_empty(),
        "no verified surface package in {}",
        root.display()
    );

    #[cfg(feature = "desktop")]
    let backend: Rc<dyn Backend> = Rc::new(korri_shell::backend::winit::Winit::install()?);
    #[cfg(feature = "webos")]
    let backend: Rc<dyn Backend> = Rc::new(
        korri_shell::backend::sdl::Sdl::install(arguments.placement).map_err(anyhow::Error::msg)?,
    );

    let korrid = Korrid::new(connection.clone());
    anyhow::ensure!(
        korrid.wait_until_listening(KORRID_START_LIMIT),
        "korrid is not listening on 127.0.0.1:{}",
        connection.port
    );
    let post: Arc<dyn Post> = Arc::new(EventLoopPost);
    let mut shell = Shell::new(
        loadable,
        korrid,
        backend,
        post.clone(),
        Options {
            limits,
            placement: arguments.placement,
            stats: arguments.stats,
        },
    );
    let outcome: Rc<RefCell<Result<(), String>>> = Rc::new(RefCell::new(Ok(())));
    let quit_outcome = outcome.clone();
    shell.on_quit(Box::new(move |result| {
        *quit_outcome.borrow_mut() = result;
        let _ = slint::quit_event_loop();
    }));
    let input_post = post.clone();
    let native = Arc::new(Mutex::new(NativeInput::start(
        connection,
        true,
        Box::new(move |event| input_post.post(Event::Native(event))),
    )));
    let follow = native.clone();
    shell.on_native_active(Box::new(move |active| {
        follow.lock().expect("native input lock").set_active(active)
    }));
    shell.start()?;
    let frames = shell.frames();
    SHELL.with(|cell| *cell.borrow_mut() = Some(shell));

    let _stats = arguments.stats.then(|| {
        let timer = slint::Timer::default();
        let last = RefCell::new((Instant::now(), frames.get()));
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(5),
            move || {
                let (then, count) = *last.borrow();
                let now = (Instant::now(), frames.get());
                let seconds = now.0.duration_since(then).as_secs_f64();
                let heap = SHELL.with(|s| s.borrow().as_ref().map_or(0, |s| s.heap_bytes()));
                eprintln!(
                    "SHELL_STATS fps={:.3} frames={} seconds={seconds:.3} heap_bytes={heap}",
                    (now.1 - count) as f64 / seconds,
                    now.1 - count
                );
                *last.borrow_mut() = now;
            },
        );
        timer
    });
    slint::run_event_loop_until_quit()?;
    SHELL.with(|cell| cell.borrow_mut().take());
    drop(native);
    let result = outcome.borrow().clone();
    result.map_err(anyhow::Error::msg)
}
