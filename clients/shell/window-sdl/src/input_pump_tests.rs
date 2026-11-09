use super::*;
use slint::ComponentHandle;
use std::sync::atomic::AtomicUsize;

slint::slint! {
    export component InputPumpProbe inherits Window {
        in property <bool> native-ready;
        in property <bool> key-ready;
        // Exact test pixels encode whether this frame saw BOTH input paths.
        background: native-ready && key-ready ? #00ff00 : #ff0000;
    }
}

struct TestPlatform(Rc<SdlPlatform>);
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        self.0.create_window_adapter()
    }
    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        self.0.new_event_loop_proxy()
    }
}

#[test]
#[ignore = "requires workstation SDL and Xvfb; never run on the TV"]
fn native_frame_and_sdl_key_reach_the_same_presented_frame() {
    let keys = Arc::new(AtomicUsize::new(0));
    let mut platform = SdlPlatform::open(
        "Input pump test",
        Placement::Windowed {
            width: 64,
            height: 64,
        },
    )
    .unwrap();
    let proxy = Proxy(platform.shared.clone());
    let shared = platform.shared.clone();
    let view = Rc::new(RefCell::new(None::<slint::Weak<InputPumpProbe>>));
    let key_view = view.clone();
    let seen = keys.clone();
    platform.host_keys = Some(Box::new(move |key, pressed| {
        if key == HostKey::Right && pressed {
            let seen = seen.clone();
            let view = key_view.borrow().clone().unwrap();
            Proxy(shared.clone())
                .invoke_from_event_loop(Box::new(move || {
                    seen.fetch_add(1, Ordering::SeqCst);
                    view.upgrade().unwrap().set_key_ready(true);
                }))
                .unwrap();
        }
    }));
    let platform = Rc::new(platform);
    slint::platform::set_platform(Box::new(TestPlatform(platform.clone()))).unwrap();
    let component = InputPumpProbe::new().unwrap();
    *view.borrow_mut() = Some(component.as_weak());
    component.show().unwrap();
    platform.render().unwrap();
    let before = platform.stats.borrow().frames;
    // A frame posted while a model job is executing must not wait through
    // another render because it missed an already-drained job snapshot.
    let late = Arc::new(AtomicUsize::new(0));
    let seen_late = late.clone();
    let late_shared = platform.shared.clone();
    Proxy(platform.shared.clone())
        .invoke_from_event_loop(Box::new(move || {
            std::thread::spawn(move || {
                Proxy(late_shared)
                    .invoke_from_event_loop(Box::new(move || {
                        seen_late.fetch_add(1, Ordering::SeqCst);
                    }))
                    .unwrap();
            })
            .join()
            .unwrap();
        }))
        .unwrap();
    platform.run_jobs();
    assert_eq!(late.load(Ordering::SeqCst), 1);
    platform
        .pump_events(None, &mut FrameBudget::default())
        .unwrap();
    // A complete native frame arrives from a worker after the old drain.
    platform.run_jobs();
    let native = Arc::new(AtomicUsize::new(0));
    let seen = native.clone();
    let native_view = component.as_weak();
    std::thread::spawn(move || {
        let frame = [(3, 16, 1), (0, 0, 0)];
        proxy
            .invoke_from_event_loop(Box::new(move || {
                assert_eq!(frame.last(), Some(&(0, 0, 0)));
                seen.fetch_add(1, Ordering::SeqCst);
                native_view.upgrade().unwrap().set_native_ready(true);
            }))
            .unwrap();
    })
    .join()
    .unwrap();
    let mut wake = ffi::Event::zeroed();
    loop {
        // SAFETY: valid event storage and live SDL on its UI thread.
        assert_eq!(
            unsafe { (platform.sdl().SDL_WaitEventTimeout)(&mut wake, 1000) },
            1,
            "the worker must wake SDL without any keyboard input"
        );
        if wake.kind() == platform.shared.wake_event {
            break;
        }
        platform.handle(&wake).unwrap();
    }
    let mut key = ffi::Event::zeroed();
    key.set_kind(ffi::SDL_KEYDOWN);
    key.0[12] = 1; // SDL_PRESSED, not a synthetic key-up record.
    key.0[16..20].copy_from_slice(&79i32.to_ne_bytes());
    key.0[20..24].copy_from_slice(&(79i32 | (1 << 30)).to_ne_bytes());
    // SAFETY: live SDL, complete upstream SDL key layout in valid storage.
    assert_eq!(unsafe { (platform.sdl().SDL_PushEvent)(&mut key) }, 1);
    // This is the same pump checkpoint used by the real event loop, with
    // native work and a real SDL key queued in the same iteration.
    assert!(platform.frame_iteration(Some(wake)).unwrap());
    assert_eq!(
        native.load(Ordering::SeqCst),
        1,
        "native wake must drain before rendering"
    );
    assert_eq!(
        keys.load(Ordering::SeqCst),
        1,
        "SDL key must drain at the same checkpoint"
    );
    assert!(component.get_native_ready() && component.get_key_ready());
    assert_eq!(platform.stats.borrow().frames, before + 1);
    let screen = platform.screen.borrow();
    let pixel = screen.buffer[screen.buffer.len() / 2 + screen.width as usize / 2];
    assert_eq!((pixel.red, pixel.green, pixel.blue), (0, 255, 0));
    drop(screen);
    // A continuing producer must not starve the next rendered frame. Keep
    // the load finite so the unbounded implementation fails rather than hangs.
    let work = Arc::new(AtomicUsize::new(0));
    for _ in 0..4096 {
        let work = work.clone();
        platform
            .shared
            .jobs
            .lock()
            .unwrap()
            .push_back(Box::new(move || {
                work.fetch_add(1, Ordering::SeqCst);
            }));
    }
    component.set_native_ready(false);
    let before_fair_frame = platform.stats.borrow().frames;
    assert!(platform.frame_iteration(None).unwrap());
    assert!(
        work.load(Ordering::SeqCst) < 4096,
        "queued work must yield before exhausting a producer's backlog"
    );
    assert!(
        platform.stats.borrow().frames > before_fair_frame,
        "clock and Problem updates must still present during a backlog"
    );
    platform.close();
}
