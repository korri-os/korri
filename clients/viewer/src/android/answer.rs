// SPDX-License-Identifier: GPL-3.0-or-later
//! One answer from Android's main thread to the thread that asked for it,
//! which may stop waiting first (#497). The Korri app's loop stops waiting
//! for its layers when the stream stops or the window goes while the main
//! thread makes them. What the main thread made must then be undone, also
//! when it sent it before the loop stopped waiting.
//!
//! The asking thread's end, dropped while the answer may still come, goes to
//! the main thread ([`Answer`]'s `Drop`). There the main thread takes and
//! undoes whatever was sent, then closes the channel. It runs one closure at
//! a time, so nothing is sent between that last read and the close. An
//! answer sent after the close is undone by the main thread when it sends it
//! ([`Reply::send`]).
use std::{
    sync::{
        Arc,
        mpsc::{self, RecvTimeoutError},
    },
    time::Duration,
};

/// Work for Android's main thread.
pub(crate) type Work = Box<dyn FnOnce() + Send>;

/// Runs work on Android's main thread after the work posted before it:
/// `AndroidApp::run_on_java_main_thread`.
pub(crate) type Post = Box<dyn Fn(Work) + Send>;

type Undo<T> = Arc<dyn Fn(T) + Send + Sync>;

/// A channel for one answer. `post` reaches the main thread; `undo` undoes
/// an answer that no one reads, on the main thread.
pub(crate) fn channel<T: Send + 'static>(
    post: Post,
    undo: impl Fn(T) + Send + Sync + 'static,
) -> (Reply<T>, Answer<T>) {
    let (sender, receiver) = mpsc::channel();
    let undo: Undo<T> = Arc::new(undo);
    (
        Reply {
            sender,
            undo: undo.clone(),
        },
        Answer {
            receiver: Some(receiver),
            settled: false,
            post,
            undo,
        },
    )
}

/// The main thread's end.
pub(crate) struct Reply<T> {
    sender: mpsc::Sender<T>,
    undo: Undo<T>,
}

impl<T> Reply<T> {
    /// Send the answer. When no one waits for it any more, undo it now.
    pub(crate) fn send(self, answer: T) {
        if let Err(mpsc::SendError(answer)) = self.sender.send(answer) {
            (self.undo)(answer);
        }
    }
}

/// The asking thread's end. Dropped before it gave the answer, it has the
/// main thread undo the answer, sent or still to come.
pub(crate) struct Answer<T: Send + 'static> {
    receiver: Option<mpsc::Receiver<T>>,
    /// The answer was read, or the main thread dropped its end: nothing more
    /// can come.
    settled: bool,
    post: Post,
    undo: Undo<T>,
}

impl<T: Send + 'static> Answer<T> {
    /// The answer, waiting at most `timeout`, as
    /// `mpsc::Receiver::recv_timeout` gives it.
    pub(crate) fn recv_timeout(&mut self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        let Some(receiver) = &self.receiver else {
            return Err(RecvTimeoutError::Disconnected);
        };
        let answer = receiver.recv_timeout(timeout);
        self.settled |= !matches!(answer, Err(RecvTimeoutError::Timeout));
        answer
    }
}

impl<T: Send + 'static> Drop for Answer<T> {
    fn drop(&mut self) {
        let Some(receiver) = self.receiver.take().filter(|_| !self.settled) else {
            return;
        };
        let undo = self.undo.clone();
        (self.post)(Box::new(move || {
            receiver.try_iter().for_each(|answer| undo(answer));
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        thread::{self, ThreadId},
    };

    /// Android's main thread, as the tests have it: one thread that runs
    /// the work posted to it, in order.
    struct Main {
        work: mpsc::Sender<Work>,
        id: ThreadId,
    }

    impl Main {
        fn start() -> Self {
            let (work, queue) = mpsc::channel::<Work>();
            let thread = thread::spawn(move || queue.into_iter().for_each(|work| work()));
            Self {
                work,
                id: thread.thread().id(),
            }
        }

        fn post(&self) -> Post {
            let work = self.work.clone();
            Box::new(move |posted: Work| work.send(posted).unwrap())
        }

        /// Run `work` there, and wait until it and the work posted before it
        /// have run.
        fn run(&self, work: impl FnOnce() + Send + 'static) {
            let (done, ran) = mpsc::channel();
            self.work
                .send(Box::new(move || {
                    work();
                    done.send(()).unwrap();
                }))
                .unwrap();
            ran.recv_timeout(Duration::from_secs(10))
                .expect("the main thread did not run the work in 10 s");
        }
    }

    /// Each undone answer, with the thread that undid it.
    #[derive(Clone, Default)]
    struct Undone(Arc<Mutex<Vec<(u32, ThreadId)>>>);

    impl Undone {
        fn undo(&self) -> impl Fn(u32) + Send + Sync + 'static {
            let undone = self.0.clone();
            move |answer| {
                undone
                    .lock()
                    .unwrap()
                    .push((answer, thread::current().id()))
            }
        }

        fn take(&self) -> Vec<(u32, ThreadId)> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    // #497: the layers came after the loop's deadline, before the stream
    // stopped. They waited in the channel, which dropped them unremoved.
    #[test]
    fn an_answer_that_waits_unread_when_the_asker_stops_is_undone_on_the_main_thread() {
        let main = Main::start();
        let undone = Undone::default();
        let (reply, mut answer) = channel(main.post(), undone.undo());
        assert_eq!(
            answer.recv_timeout(Duration::ZERO),
            Err(RecvTimeoutError::Timeout)
        );
        main.run(move || reply.send(1));
        drop(answer);
        main.run(|| {});
        assert_eq!(undone.take(), [(1, main.id)]);
    }

    #[test]
    fn an_answer_still_being_made_when_the_asker_stops_is_undone_once_it_is_sent() {
        let main = Main::start();
        let undone = Undone::default();
        let (reply, answer) = channel(main.post(), undone.undo());
        // The main thread makes it while the asker stops waiting.
        let (go, making) = mpsc::channel::<()>();
        main.work
            .send(Box::new(move || {
                making.recv().unwrap();
                reply.send(2);
            }))
            .unwrap();
        drop(answer);
        go.send(()).unwrap();
        main.run(|| {});
        assert_eq!(undone.take(), [(2, main.id)]);
    }

    #[test]
    fn an_answer_sent_after_the_channel_closed_is_undone_by_its_sender() {
        let main = Main::start();
        let undone = Undone::default();
        let (reply, answer) = channel(main.post(), undone.undo());
        drop(answer);
        main.run(|| {});
        main.run(move || reply.send(3));
        assert_eq!(undone.take(), [(3, main.id)]);
    }

    #[test]
    fn an_answer_that_was_read_is_kept_and_its_drop_posts_nothing() {
        let main = Main::start();
        let undone = Undone::default();
        let posts = Arc::new(AtomicUsize::new(0));
        let counted = || -> Post {
            let (posts, post) = (posts.clone(), main.post());
            Box::new(move |work: Work| {
                posts.fetch_add(1, Ordering::SeqCst);
                post(work);
            })
        };
        let (reply, mut answer) = channel(counted(), undone.undo());
        main.run(move || reply.send(4));
        assert_eq!(answer.recv_timeout(Duration::from_secs(10)), Ok(4));
        drop(answer);
        // A main thread that drops its end without an answer settles it too.
        let (reply, mut answer) = channel(counted(), undone.undo());
        main.run(move || drop(reply));
        assert_eq!(
            answer.recv_timeout(Duration::from_secs(10)),
            Err(RecvTimeoutError::Disconnected)
        );
        drop(answer);
        main.run(|| {});
        assert!(undone.take().is_empty());
        assert_eq!(posts.load(Ordering::SeqCst), 0);
    }
}
