//! The clock for surface logic budgets. Device runtime uses wall time; build
//! verification and tests explicitly select the calling thread's CPU time (#368).

use std::{
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default)]
pub enum BudgetClock {
    #[default]
    Wall,
    ThreadCpu,
}

impl BudgetClock {
    /// Start on the thread that runs QuickJS, not the thread waiting for it.
    pub fn start(self) -> Timer {
        Timer {
            started: match self {
                Self::Wall => Started::Wall(Instant::now()),
                Self::ThreadCpu => Started::ThreadCpu(thread_cpu_time()),
            },
            same_thread: PhantomData,
        }
    }
}

#[derive(Clone, Copy)]
enum Started {
    Wall(Instant),
    ThreadCpu(Duration),
}

/// A timer must be read on its starting thread. In particular, a UI thread
/// must not subtract its own CPU time from a logic thread's CPU time.
#[derive(Clone, Copy)]
pub struct Timer {
    started: Started,
    same_thread: PhantomData<Rc<()>>,
}

impl Timer {
    pub fn elapsed(self) -> Duration {
        match self.started {
            Started::Wall(started) => started.elapsed(),
            Started::ThreadCpu(started) => thread_cpu_time() - started,
        }
    }
}

fn thread_cpu_time() -> Duration {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // Linux and Android provide this clock. Fail closed if the clock cannot
    // be read: a zero or wall-time fallback would measure a different budget.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) };
    assert_eq!(
        result,
        0,
        "thread CPU clock: {}",
        std::io::Error::last_os_error()
    );
    Duration::new(
        now.tv_sec.try_into().expect("nonnegative CPU seconds"),
        now.tv_nsec.try_into().expect("valid CPU nanoseconds"),
    )
}
