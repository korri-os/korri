//! THROWAWAY SPIKE. Runtime surface loading; not a product client or treaty.
mod input;
mod logic;
mod view;

use anyhow::{bail, Context, Result};
use input::{Action, Input};
use slint::{
    winit_030::{winit, EventResult, WinitWindowAccessor},
    ComponentHandle,
};
use slint_interpreter::{Compiler, ComponentInstance};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc::{self, Sender},
    time::{Duration, Instant},
};

fn flag(name: &str) -> bool {
    std::env::var(name).as_deref() == Ok("1")
}
fn size() -> Result<slint::LogicalSize> {
    let value = std::env::var("SHELL_SIZE").unwrap_or_else(|_| "640x480".into());
    let (w, h) = value.split_once('x').context("SHELL_SIZE must be WxH")?;
    let (w, h): (u32, u32) = (w.parse()?, h.parse()?);
    if w == 0 || h == 0 {
        bail!("SHELL_SIZE dimensions must be positive");
    }
    Ok(slint::LogicalSize::new(w as f32, h as f32))
}

struct LoadTiming {
    package: String,
    started: Instant,
    compile_ms: f64,
    logic_ms: f64,
}

fn install_events(
    component: &ComponentInstance,
    tx: Sender<Input>,
    frames: Rc<Cell<u64>>,
    ready: Rc<Cell<bool>>,
    timing: LoadTiming,
) {
    let LoadTiming {
        package,
        started,
        compile_ms,
        logic_ms,
    } = timing;
    use winit::{
        event::{ElementState, WindowEvent},
        keyboard::{KeyCode, PhysicalKey},
    };
    let mut held = Vec::new();
    let mut first = true;
    let shown = Instant::now();
    component.window().on_winit_window_event(move |_,event| {
        match event {
            WindowEvent::RedrawRequested => {
                frames.set(frames.get()+1);
                if first {
                    first = false;
                    let ready = ready.clone();
                    let package = package.clone();
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        if flag("SHELL_STATS") {
                            eprintln!("SHELL_STATS package={package} compile_ms={compile_ms:.3} logic_load_ms={logic_ms:.3} first_frame_ms={:.3} show_to_frame_ms={:.3}",started.elapsed().as_secs_f64()*1000.,shown.elapsed().as_secs_f64()*1000.);
                        }
                        ready.set(true);
                    });
                }
            }
            // X11 replays held keys as synthetic presses when a new window
            // gains focus. Replaying held F2 would immediately switch again.
            WindowEvent::KeyboardInput { is_synthetic: true, .. } => return EventResult::PreventDefault,
            WindowEvent::KeyboardInput { event, .. } => {
                if event.repeat { return EventResult::PreventDefault; }
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    let direction = match code { KeyCode::ArrowUp=>Some(Action::Up),KeyCode::ArrowDown=>Some(Action::Down),KeyCode::ArrowLeft=>Some(Action::Left),KeyCode::ArrowRight=>Some(Action::Right),_=>None };
                    if let Some(direction) = direction {
                        held.retain(|v| *v != direction);
                        if pressed { held.push(direction); }
                        let _ = tx.send(Input::Direction("keyboard".into(),held.last().copied()));
                    } else if pressed {
                        let action = match code { KeyCode::F2=>Some(Action::Switch),KeyCode::Enter=>Some(Action::Confirm),KeyCode::Escape|KeyCode::Backspace=>Some(Action::Back),KeyCode::KeyM=>Some(Action::Menu),KeyCode::KeyO=>Some(Action::Options),_=>None };
                        if let Some(action) = action { let _ = tx.send(Input::Press("keyboard".into(), action)); }
                    }
                }
                return EventResult::PreventDefault;
            }
            WindowEvent::Focused(false) => { held.clear(); let _ = tx.send(Input::Clear("keyboard".into())); }
            WindowEvent::CloseRequested => { let _ = slint::quit_event_loop(); }
            _=>{}
        }
        EventResult::Propagate
    });
}

struct Surface {
    component: ComponentInstance,
    logic: logic::Logic,
    package: PathBuf,
    ready: Rc<Cell<bool>>,
}
struct Shell {
    root: PathBuf,
    packages: Vec<String>,
    index: usize,
    active: Option<Surface>,
    retired: Vec<Surface>,
    model: String,
    converter: view::Converter,
    tx: Sender<Input>,
    frames: Rc<Cell<u64>>,
}
impl Shell {
    fn load(&mut self, index: usize) -> Result<()> {
        let started = Instant::now();
        let name = &self.packages[index];
        let package = self.root.join("surfaces").join(name);
        let compiler = Compiler::default();
        let result = pollster::block_on(compiler.build_from_path(package.join("main.slint")));
        for diagnostic in result.diagnostics() {
            eprintln!("shell: {diagnostic}");
        }
        if result.has_errors() {
            bail!("compile {name} failed; current surface kept");
        }
        let definition = result
            .component("Surface")
            .context("main.slint must export Surface")?;
        let compile_ms = started.elapsed().as_secs_f64() * 1000.;
        let logic_started = Instant::now();
        let logic = logic::Logic::load(&package, &self.model)?;
        let initial = logic.view(None)?;
        let logic_ms = logic_started.elapsed().as_secs_f64() * 1000.;
        let component = definition.create()?;
        self.converter.apply(&component, &initial, &package)?;
        component.window().set_size(size()?);
        component.window().set_fullscreen(flag("SHELL_FULLSCREEN"));
        let ready = Rc::new(Cell::new(false));
        install_events(
            &component,
            self.tx.clone(),
            self.frames.clone(),
            ready.clone(),
            LoadTiming {
                package: name.clone(),
                started,
                compile_ms,
                logic_ms,
            },
        );
        component.show()?;
        let next = Surface {
            component,
            logic,
            package,
            ready,
        };
        if let Some(old) = self.active.replace(next) {
            self.retired.push(old);
        }
        self.index = index;
        Ok(())
    }
    fn action(&mut self, action: Action) -> Result<()> {
        if action == Action::Switch {
            return self.load((self.index + 1) % self.packages.len());
        }
        let surface = self.active.as_ref().context("no active surface")?;
        let view = surface.logic.view(Some(action.name()))?;
        self.converter
            .apply(&surface.component, &view, &surface.package)
    }
    fn retire(&mut self) {
        if self.active.as_ref().is_some_and(|s| s.ready.get()) {
            for old in self.retired.drain(..) {
                if let Err(error) = old.component.hide() {
                    eprintln!("shell: hide previous: {error}");
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let root = std::env::var_os("SHELL_ROOT").map(PathBuf::from).unwrap_or(
        std::env::current_exe()?
            .parent()
            .context("executable directory")?
            .to_owned(),
    );
    let packages: Vec<String> = std::env::var("SHELL_SURFACES")
        .unwrap_or_else(|_| "zone,pico".into())
        .split(',')
        .map(str::to_owned)
        .collect();
    for package in &packages {
        if package.is_empty()
            || Path::new(package)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            bail!("SHELL_SURFACES entries must be relative package paths");
        }
    }
    let model = std::fs::read_to_string(root.join("fixtures/model.json"))
        .context("read fixtures/model.json (set SHELL_ROOT)")?;
    let _: serde_json::Value = serde_json::from_str(&model)?;
    let (tx, rx) = mpsc::channel();
    input::gamepads(tx.clone());
    let frames = Rc::new(Cell::new(0));
    let mut shell = Shell {
        root,
        packages,
        index: 0,
        active: None,
        retired: Vec::new(),
        model,
        converter: view::Converter::default(),
        tx,
        frames: frames.clone(),
    };
    shell.load(0)?;
    let mut mapper = input::Mapper::default();
    let started = Instant::now();
    let mut last_stats = (Instant::now(), 0);
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(5),
        move || {
            let now = started.elapsed().as_millis() as u64;
            let mut actions = Vec::new();
            for event in rx.try_iter() {
                actions.extend(mapper.handle(event, now));
            }
            actions.extend(mapper.tick(now));
            for action in actions {
                if let Err(error) = shell.action(action) {
                    eprintln!("shell: {error:#}");
                }
            }
            shell.retire();
            if flag("SHELL_STATS") && last_stats.0.elapsed() >= Duration::from_secs(5) {
                let count = frames.get();
                eprintln!(
                    "SHELL_STATS fps={:.3} frames={} seconds={:.3}",
                    (count - last_stats.1) as f64 / last_stats.0.elapsed().as_secs_f64(),
                    count - last_stats.1,
                    last_stats.0.elapsed().as_secs_f64()
                );
                last_stats = (Instant::now(), count);
            }
        },
    );
    slint::run_event_loop()?;
    Ok(())
}
