// SPDX-License-Identifier: GPL-3.0-or-later
//! The stream's rules that need no Android: the launches queued for an
//! activity when it ends, a launch that arrives while another is in front,
//! a launch's start while its layers and its server certificate are made
//! side by side, the end of the runtime's thread, the
//! keys that go on to Android, the files of the pinned server certificates,
//! the blocking calls from korrid's async code, and the screen that the
//! viewer reports. The
//! library builds for Android only, so the build machine runs these tests
//! through `tests/host.rs` (`cargo test --test host`).
use std::{
    io,
    path::Path,
    sync::{mpsc, Mutex, PoisonError},
    thread,
    time::{Duration, Instant},
};

/// Where the adapter hands launches to the activity in front: that
/// activity's number and the sender of its stream's queue.
pub(crate) type Slot<T> = Mutex<Option<(u64, mpsc::Sender<T>)>>;

/// The activity `id` ends. No launch reaches its queue any more, and each
/// launch still in it goes to `refuse`, so that its lifecycle fails and
/// korrid's start hears why. A launch that is only dropped leaves its
/// lifecycle unfinished, and the adapter then refuses every later start.
///
/// The adapter sends while it holds the slot's lock, so after the slot is
/// emptied under that lock nothing more arrives.
pub(crate) fn close<T>(
    slot: &Slot<T>,
    id: u64,
    queue: &mpsc::Receiver<T>,
    mut refuse: impl FnMut(T),
) {
    let mut shows = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if shows.as_ref().is_some_and(|(owner, _)| *owner == id) {
        *shows = None;
    }
    drop(shows);
    while let Ok(launch) = queue.try_recv() {
        refuse(launch);
    }
}

/// What the loop thread has in front when a launch arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Front {
    Idle,
    /// A launch whose layers or server certificate are still being made.
    Starting,
    /// A stream that plays.
    Playing,
    /// A stream that ends: its stop was asked for, or its runtime has
    /// reported its end. Its layers go at one of the next ticks.
    Ending,
}

/// What the loop thread does with a launch that arrives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Arriving {
    Begin,
    /// It waits until the stream in front has ended, then begins. korrid
    /// starts again as soon as it reads the old stream's Stopped, which can
    /// come before the loop thread's next tick lets that stream go.
    Wait,
    Refuse(&'static str),
}

/// A launch that arrives: it begins on an idle loop, waits behind a stream
/// that ends, and is refused beside a launch that starts or a stream that
/// plays.
pub(crate) fn arriving(front: Front) -> Arriving {
    match front {
        Front::Idle => Arriving::Begin,
        Front::Ending => Arriving::Wait,
        Front::Playing => Arriving::Refuse("another launch is in front"),
        Front::Starting => Arriving::Refuse("another launch is starting"),
    }
}

/// The loop thread's one launch that waits for the stream in front to end
/// (`Arriving::Wait`), and since when. A refused launch goes to `refuse`
/// with why, so that its lifecycle fails and korrid's start hears it, as
/// [`close`] refuses a queue's launches.
pub(crate) struct Waiting<T> {
    limit: Duration,
    held: Option<(T, Instant)>,
}

impl<T> Waiting<T> {
    /// A slot whose launch waits at most `limit`.
    pub(crate) fn new(limit: Duration) -> Self {
        Self { limit, held: None }
    }

    /// `launch` waits from `now`. One that waited already is refused:
    /// korrid gave up its start and asked again, so the newer one waits.
    pub(crate) fn hold(&mut self, launch: T, now: Instant, mut refuse: impl FnMut(T, String)) {
        if let Some((older, _)) = self.held.replace((launch, now)) {
            refuse(older, "a newer launch came while it waited".into());
        }
    }

    /// One tick, after the stream in front had its turn: the launch to
    /// begin once the loop is `idle`. One that has waited `limit` while the
    /// stream in front still ends is refused: a stream that never ends must
    /// not hold korrid's start, and the adapter's one start or stop at a
    /// time, for ever.
    pub(crate) fn tick(
        &mut self,
        idle: bool,
        now: Instant,
        mut refuse: impl FnMut(T, String),
    ) -> Option<T> {
        let (launch, since) = self.held.take()?;
        if idle {
            return Some(launch);
        }
        if now.saturating_duration_since(since) >= self.limit {
            refuse(
                launch,
                format!(
                    "the stream in front did not end within {} s",
                    self.limit.as_secs()
                ),
            );
        } else {
            self.held = Some((launch, since));
        }
        None
    }

    /// The window or the activity went: the waiting launch is refused with
    /// `why`.
    pub(crate) fn close(&mut self, why: &str, mut refuse: impl FnMut(T, String)) {
        if let Some((launch, _)) = self.held.take() {
            refuse(launch, why.to_owned());
        }
    }
}

/// What a launch's certificate channel gave at one tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Certificate {
    Waiting,
    Provisioned,
    /// korrid's broker did not provision it, with why.
    Refused(String),
    /// The channel closed with nothing in it: korrid dropped its start call
    /// before the certificate came.
    StartEnded,
}

/// The certificate's part of a launch.
pub(crate) fn certificate(certificate: Certificate) -> Part {
    match certificate {
        Certificate::Waiting => Part::Waiting,
        Certificate::Provisioned => Part::Ready,
        Certificate::Refused(why) => Part::Failed(why),
        Certificate::StartEnded => {
            Part::Failed("korrid's start ended before the server certificate came".into())
        }
    }
}

/// Where one of a launch's two parts is while the loop thread waits: its
/// layers, which Android's main thread makes, and its server certificate,
/// which korrid's broker provisions meanwhile (#391).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Waiting,
    Ready,
    /// It failed, with why.
    Failed(String),
}

/// What the loop thread does with a launch whose parts are made side by
/// side, at one of its ticks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Making {
    /// A part is still on its way.
    Wait,
    /// Both parts are there: the runtime starts, and Start goes to it.
    Start,
    /// korrid asked for Stop meanwhile: nothing started, and the launch
    /// stopped as asked.
    Stopped,
    /// The launch is refused, with why: what was made goes, and korrid's
    /// start hears why.
    Refuse(String),
}

/// The next step of a launch whose layers and server certificate are made
/// side by side. A stop goes first, then a window that went, then a part
/// that failed: none waits for the other part.
pub(crate) fn making(stop: bool, window: bool, layers: &Part, certificate: &Part) -> Making {
    if stop {
        return Making::Stopped;
    }
    if !window {
        return Making::Refuse("the app's window went while the layers were made".into());
    }
    match (layers, certificate) {
        (Part::Failed(why), _) => Making::Refuse(format!("no video layer: {why}")),
        (_, Part::Failed(why)) => Making::Refuse(why.clone()),
        (Part::Ready, Part::Ready) => Making::Start,
        _ => Making::Wait,
    }
}

/// What the runtime's lifecycle must still hear once its thread has ended.
/// A runtime that returns has reported its own end, a failure too. One whose
/// thread panicked reported nothing: its lifecycle would never finish, and
/// the adapter would refuse every later start.
pub(crate) fn unreported<T>(joined: &thread::Result<T>) -> Option<&'static str> {
    joined
        .as_ref()
        .err()
        .map(|_| "the viewer's thread panicked")
}

/// Whether the stream keeps a key from the shell under it. `android`: one of
/// Android's own keys (volume, power, `overlay::is_system_key`). `taken`:
/// the stream or its overlay took it.
///
/// While the stream stops nothing takes a key, and Android's own keys go on
/// to Android. Any other key stays the stream's, taken or not: one that
/// reached Slint under the video once raised the soft keyboard over it.
pub(crate) fn keeps_key(android: bool, stopping: bool, taken: bool) -> bool {
    if stopping {
        !android
    } else {
        taken || !android
    }
}

/// The file name of a pinned server certificate: one launch's, at one of
/// its starts (korrid's recovery starts a launch again).
pub(crate) fn pin_name(launch: &str, start: u64) -> String {
    format!("viewer-pin-{launch}-{start}.pem")
}

/// Whether `name` is a pinned server certificate's (`pin_name`).
fn is_pin(name: &str) -> bool {
    name.starts_with("viewer-pin-") && name.ends_with(".pem")
}

/// Remove the pinned server certificates that an earlier process left in
/// `directory`: a process that Android ends drops no `Pin`. When the adapter
/// is made no launch plays yet, so every one is stale. Only regular files
/// go; a link or a folder of that name stays. Returns how many it removed.
pub(crate) fn remove_stale_pins(directory: &Path) -> io::Result<usize> {
    let mut removed = 0;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let pin = entry.file_name().to_str().is_some_and(is_pin);
        // The entry's own type: a link is not followed.
        if pin && entry.file_type()?.is_file() {
            std::fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// A treaty request, or any call that waits or goes into Android, from
/// korrid's async code: on a thread of the runtime's blocking pool, so that
/// korrid's async worker stays free. `None` when it panicked.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    tokio::task::spawn_blocking(work).await.ok()
}

/// Start `work` with `spawn`; its value goes to `answer`. When no thread
/// starts, `refused` goes to `answer` here instead, so that whoever waits for
/// the answer is not left waiting. The error says why no thread started.
pub(crate) fn spawn_answering<T: Send + 'static>(
    spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> io::Result<()>,
    answer: mpsc::Sender<T>,
    work: impl FnOnce() -> T + Send + 'static,
    refused: T,
) -> io::Result<()> {
    let sender = answer.clone();
    spawn(Box::new(move || {
        let _ = sender.send(work());
    }))
    .inspect_err(|_| {
        let _ = answer.send(refused);
    })
}

/// The viewer's screen in its Start (#476): the window's size, its longer
/// side first, as the Artemis app reported its screen
/// (`KorriStreamViewer.screenStreamSize`), because Portrait is a stream
/// control and not the way the phone is held. With it the viewer reports its
/// shape, which korrid's live-shape reads. None while the window has no size.
pub(crate) fn screen(window: (i32, i32)) -> Option<(u32, u32)> {
    let (width, height) = window;
    (width > 0 && height > 0).then(|| (width.max(height) as u32, width.min(height) as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::symlink,
        path::PathBuf,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    /// The launches that a slot refused, with why.
    fn refused<'a>(
        into: &'a mut Vec<(&'static str, String)>,
    ) -> impl FnMut(&'static str, String) + 'a {
        |launch, why| into.push((launch, why))
    }

    // Review of #391: the slot of a launch that waits behind a stream that
    // ends, as the loop thread keeps it.
    #[test]
    fn a_waiting_launch_begins_once_the_loop_is_idle() {
        let (mut slot, mut out) = (Waiting::new(Duration::from_secs(30)), Vec::new());
        let now = Instant::now();
        assert_eq!(
            slot.tick(true, now, refused(&mut out)),
            None,
            "nothing waits"
        );
        slot.hold("first", now, refused(&mut out));
        assert_eq!(
            slot.tick(false, now + Duration::from_secs(1), refused(&mut out)),
            None
        );
        assert_eq!(
            slot.tick(true, now + Duration::from_secs(2), refused(&mut out)),
            Some("first")
        );
        assert_eq!(
            slot.tick(true, now + Duration::from_secs(3), refused(&mut out)),
            None
        );
        assert!(out.is_empty());
    }

    #[test]
    fn a_newer_launch_takes_the_slot_and_the_older_is_refused() {
        let (mut slot, mut out) = (Waiting::new(Duration::from_secs(30)), Vec::new());
        let now = Instant::now();
        slot.hold("older", now, refused(&mut out));
        slot.hold("newer", now + Duration::from_secs(1), refused(&mut out));
        assert_eq!(
            out,
            [("older", "a newer launch came while it waited".to_owned())]
        );
        assert_eq!(
            slot.tick(true, now + Duration::from_secs(2), refused(&mut out)),
            Some("newer")
        );
    }

    #[test]
    fn a_launch_that_waits_too_long_is_refused_so_korrids_start_ends() {
        let limit = Duration::from_secs(30);
        let (mut slot, mut out) = (Waiting::new(limit), Vec::new());
        let now = Instant::now();
        slot.hold("waits", now, refused(&mut out));
        let almost = now + limit - Duration::from_millis(1);
        assert_eq!(slot.tick(false, almost, refused(&mut out)), None);
        assert!(out.is_empty());
        assert_eq!(slot.tick(false, now + limit, refused(&mut out)), None);
        assert_eq!(
            out,
            [(
                "waits",
                "the stream in front did not end within 30 s".to_owned()
            )]
        );
        assert_eq!(slot.tick(true, now + limit, refused(&mut out)), None);
    }

    #[test]
    fn the_wait_of_a_newer_launch_starts_when_it_arrives() {
        let limit = Duration::from_secs(30);
        let (mut slot, mut out) = (Waiting::new(limit), Vec::new());
        let now = Instant::now();
        slot.hold("older", now, refused(&mut out));
        slot.hold("newer", now + Duration::from_secs(20), refused(&mut out));
        assert_eq!(slot.tick(false, now + limit, refused(&mut out)), None);
        assert_eq!(out.len(), 1, "only the older, replaced: {out:?}");
        assert_eq!(
            slot.tick(true, now + Duration::from_secs(49), refused(&mut out)),
            Some("newer")
        );
    }

    #[test]
    fn a_window_or_an_activity_that_goes_refuses_the_waiting_launch() {
        let (mut slot, mut out) = (Waiting::new(Duration::from_secs(30)), Vec::new());
        let now = Instant::now();
        slot.close("the activity ended", refused(&mut out));
        assert!(out.is_empty(), "nothing waits");
        slot.hold("waits", now, refused(&mut out));
        slot.close("the activity ended", refused(&mut out));
        assert_eq!(out, [("waits", "the activity ended".to_owned())]);
        assert_eq!(slot.tick(true, now, refused(&mut out)), None);
    }

    // D11 review: Snapshot ran the lifecycle's request, with the viewer's
    // battery read through JNI, on korrid's async worker. On a runtime with
    // one worker, work that waits must leave that worker free.
    #[test]
    fn blocking_work_leaves_korrids_async_worker_free() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let freed = runtime.block_on(async {
            let (ran, heard) = mpsc::channel();
            let other = tokio::spawn(async move { ran.send(()).unwrap() });
            let freed = blocking(move || heard.recv_timeout(Duration::from_secs(2)).is_ok()).await;
            other.await.unwrap();
            freed
        });
        assert_eq!(freed, Some(true));
    }

    // Review of D5b: when the leave's thread does not start, the opening
    // must still hear an answer, or it stays lent with the input held.
    #[test]
    fn a_thread_that_does_not_start_still_answers_with_the_refusal() {
        let (answer, answers) = mpsc::channel();
        let failed = spawn_answering(
            |_| Err(io::Error::other("no thread")),
            answer,
            || (7, true),
            (7, false),
        );
        assert!(failed.is_err());
        assert_eq!(answers.recv_timeout(Duration::from_secs(1)), Ok((7, false)));
    }

    #[test]
    fn a_thread_that_starts_answers_with_its_work() {
        let (answer, answers) = mpsc::channel();
        let started = spawn_answering(
            |work| thread::Builder::new().spawn(work).map(drop),
            answer,
            || (8, true),
            (8, false),
        );
        assert!(started.is_ok());
        assert_eq!(answers.recv_timeout(Duration::from_secs(5)), Ok((8, true)));
    }

    // #391: the layers and the server certificate are made side by side.
    #[test]
    fn a_launch_starts_once_its_layers_and_its_certificate_are_both_there() {
        use Part::{Ready, Waiting};
        assert_eq!(making(false, true, &Waiting, &Waiting), Making::Wait);
        assert_eq!(making(false, true, &Ready, &Waiting), Making::Wait);
        assert_eq!(making(false, true, &Waiting, &Ready), Making::Wait);
        assert_eq!(making(false, true, &Ready, &Ready), Making::Start);
    }

    #[test]
    fn a_part_that_fails_refuses_the_launch_without_waiting_for_the_other() {
        use Part::{Failed, Ready, Waiting};
        assert_eq!(
            making(
                false,
                true,
                &Waiting,
                &Failed("refused by the holder".into())
            ),
            Making::Refuse("refused by the holder".into())
        );
        assert_eq!(
            making(false, true, &Failed("5 s".into()), &Waiting),
            Making::Refuse("no video layer: 5 s".into())
        );
        assert_eq!(
            making(false, true, &Failed("5 s".into()), &Ready),
            Making::Refuse("no video layer: 5 s".into())
        );
    }

    #[test]
    fn a_stop_or_a_window_that_went_ends_the_launch_before_anything_else() {
        use Part::{Failed, Ready, Waiting};
        for (layers, certificate) in [
            (Waiting, Waiting),
            (Ready, Ready),
            (Failed("x".into()), Waiting),
        ] {
            assert_eq!(making(true, true, &layers, &certificate), Making::Stopped);
            assert_eq!(making(true, false, &layers, &certificate), Making::Stopped);
            assert_eq!(
                making(false, false, &layers, &certificate),
                Making::Refuse("the app's window went while the layers were made".into())
            );
        }
    }

    // Review of #391: korrid's recovery started again while the old stream
    // was still ending, and the launch was refused.
    #[test]
    fn a_launch_waits_behind_a_stream_that_ends_and_is_refused_only_beside_one_that_runs() {
        assert_eq!(arriving(Front::Idle), Arriving::Begin);
        assert_eq!(arriving(Front::Ending), Arriving::Wait);
        assert_eq!(
            arriving(Front::Playing),
            Arriving::Refuse("another launch is in front")
        );
        assert_eq!(
            arriving(Front::Starting),
            Arriving::Refuse("another launch is starting")
        );
    }

    // Review of #391: a start that korrid dropped is not a certificate
    // that korrid's broker could not provision.
    #[test]
    fn a_certificate_that_never_came_says_whether_korrid_ended_its_start() {
        assert_eq!(certificate(Certificate::Waiting), Part::Waiting);
        assert_eq!(certificate(Certificate::Provisioned), Part::Ready);
        assert_eq!(
            certificate(Certificate::Refused("the holder refused".into())),
            Part::Failed("the holder refused".into())
        );
        let Part::Failed(why) = certificate(Certificate::StartEnded) else {
            panic!("a start that ended is a failed part");
        };
        assert_eq!(
            why,
            "korrid's start ended before the server certificate came"
        );
        assert!(!why.contains("not provisioned"));
    }

    #[test]
    fn an_activity_that_ends_refuses_every_launch_still_queued() {
        let (sender, queue) = mpsc::channel();
        let slot: Slot<&str> = Mutex::new(Some((1, sender.clone())));
        sender.send("first").unwrap();
        sender.send("second").unwrap();
        drop(sender);
        let mut refused = Vec::new();
        close(&slot, 1, &queue, |launch| refused.push(launch));
        assert_eq!(refused, ["first", "second"]);
        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn a_newer_activity_keeps_its_slot_and_the_old_queue_is_still_refused() {
        let (old, queue) = mpsc::channel();
        old.send("queued before the newer activity came").unwrap();
        drop(old);
        let (newer, _newer_queue) = mpsc::channel::<&str>();
        let slot: Slot<&str> = Mutex::new(Some((2, newer)));
        let mut refused = Vec::new();
        close(&slot, 1, &queue, |launch| refused.push(launch));
        assert_eq!(refused, ["queued before the newer activity came"]);
        assert_eq!(slot.lock().unwrap().as_ref().map(|(id, _)| *id), Some(2));
    }

    #[test]
    fn a_runtime_thread_that_panicked_still_fails_its_lifecycle() {
        let panicked =
            thread::spawn(|| -> Result<(), String> { panic!("the runtime panicked") }).join();
        assert_eq!(unreported(&panicked), Some("the viewer's thread panicked"));
        // A runtime that returns reports its own end, also a failure.
        let ended = thread::spawn(|| -> Result<(), String> { Ok(()) }).join();
        assert_eq!(unreported(&ended), None);
        let failed = thread::spawn(|| -> Result<(), String> { Err("refused".into()) }).join();
        assert_eq!(unreported(&failed), None);
    }

    #[test]
    fn android_keys_go_on_to_android_also_while_the_stream_stops() {
        assert!(
            !keeps_key(true, true, false),
            "volume while the stream stops"
        );
        assert!(
            keeps_key(false, true, false),
            "a letter while the stream stops"
        );
        assert!(
            !keeps_key(true, false, false),
            "volume that the stream leaves"
        );
        assert!(
            keeps_key(false, false, false),
            "a letter that the stream leaves"
        );
        assert!(keeps_key(false, false, true), "a key that the stream takes");
    }

    fn scratch() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "korri-android-app-pins-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn the_screen_is_the_window_with_its_longer_side_first() {
        assert_eq!(screen((2400, 1080)), Some((2400, 1080)));
        assert_eq!(screen((1080, 2400)), Some((2400, 1080)));
        assert_eq!(screen((1840, 2208)), Some((2208, 1840)));
        assert_eq!(screen((0, 0)), None);
        assert_eq!(screen((1080, 0)), None);
    }

    #[test]
    fn stale_pins_go_and_nothing_else_does() {
        let directory = scratch();
        let stale = [pin_name("0123", 1), pin_name("4567", 2)];
        for name in &stale {
            fs::write(directory.join(name), "certificate").unwrap();
        }
        fs::write(directory.join("other.pem"), "kept").unwrap();
        fs::write(directory.join("viewer-pin-note.txt"), "kept").unwrap();
        symlink("other.pem", directory.join(pin_name("link", 3))).unwrap();
        fs::create_dir(directory.join(pin_name("folder", 4))).unwrap();

        assert_eq!(remove_stale_pins(&directory).unwrap(), 2);
        let mut left: Vec<String> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "other.pem",
                "viewer-pin-folder-4.pem",
                "viewer-pin-link-3.pem",
                "viewer-pin-note.txt",
            ]
        );
        fs::remove_dir_all(&directory).unwrap();
    }
}
