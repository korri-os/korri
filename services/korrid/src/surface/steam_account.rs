//! `SurfaceModel.steamAccount` (ADR 0083, contract "Surface treaty"): the
//! current person's Steam sign-in, the QR flow, sign-out, and the wait of a
//! launch that needs a sign-in. Every action runs through `RunAction`.
//!
//! The facts come from korrid through `SteamEngineAccess`; the effects are
//! job-mode hosts. A launch on the engine runner with no sign-in waits here
//! (`forGame` set), continues after the sign-in, and stops on Cancel.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use korri_surface_treaty::SurfaceSteamAccount;

use super::{acquisition_gate::Step, Notify};
use crate::steam_engine::{
    account::{self, Account},
    availability::Unavailable,
    host::HostPlan,
    jobs::{JobHost, JobRunner},
    protocol::{Command, Event},
};

/// What korrid knows about the engine runner for the current person.
pub enum SteamFacts {
    /// No engine runner is installed and enabled: no Steam account entry.
    Absent,
    Unavailable(Unavailable),
    Ready {
        plan: HostPlan,
        runner: JobRunner,
    },
}

/// korrid's side, read in process (no surface request and no RPC).
pub trait SteamEngineAccess: Send + Sync {
    /// Blocking: reads the plugin registry, the artifacts and the person.
    fn facts(&self) -> SteamFacts;
    /// Blocking: whether this game's launch route is the engine runner.
    fn runs_on_engine(&self, game_id: &str, runner_id: Option<&str>) -> bool;
    /// Make the client runner the selected runner of the system `steam`, with
    /// Korri's runner selection at system scope, and queue whatever it needs.
    fn use_client_runner(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
    /// Blocking: the engine runner's artifacts (its engine package, Proton,
    /// FEX), to queue before a first sign-in.
    fn engine_artifacts(&self) -> Vec<String>;
    /// True once after the launch sync of this game (korrid's id) found a
    /// Steam Cloud conflict and korrid wrote the split.
    fn take_conflict(&self, game_id: &str) -> bool;
    /// Cheap: what decides whether an engine runner is here (the enabled
    /// Steam package and whether it carries the engine host). A change means
    /// the account is read again.
    fn presence(&self) -> String;
}

/// A QR session lasts at most this long; Steam rotates the code inside it.
const SIGN_IN_LIMIT: Duration = Duration::from_secs(15 * 60);
/// The status job and sign-out wait at most this long for the host.
const STATUS_LIMIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(200);
/// A sign-in waits this long for a job host that is ending (its shutdown
/// grace, 40 s while the engine saves a login, and a little more).
const JOB_END_WAIT: Duration = Duration::from_secs(45);
/// How often the Steam package's presence is read (one small file).
const PRESENCE_POLL: Duration = Duration::from_secs(2);
/// The engine package (48 MB) and Proton download before a first sign-in.
const PACKAGE_LIMIT: Duration = Duration::from_secs(30 * 60);

struct Pending {
    proceed: Step,
    abandon: Step,
}

#[derive(Default)]
struct Inner {
    /// `None` until the facts are read, and while no engine runner exists.
    account: Option<Account>,
    /// Set while a job host runs; storing true stops it.
    job: Option<Arc<AtomicBool>>,
    pending: Option<Pending>,
}

pub struct SteamAccount {
    korrid: super::korrid::Korrid,
    access: Option<Arc<dyn SteamEngineAccess>>,
    inner: Mutex<Inner>,
    notify: Notify,
}

impl SteamAccount {
    pub fn new(korrid: super::korrid::Korrid, notify: Notify) -> Arc<Self> {
        Arc::new(Self {
            access: korrid.steam_engine.clone(),
            korrid,
            inner: Mutex::new(Inner::default()),
            notify,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("steam account poisoned")
    }

    pub fn snapshot(&self) -> Option<SurfaceSteamAccount> {
        self.lock().account.as_ref().and_then(Account::surface)
    }

    fn set(&self, account: Option<Account>) {
        self.lock().account = account;
        (self.notify)();
    }

    /// Read the facts and, when the runner can run, ask a job host who is
    /// signed in. Called at start and when plugins change.
    pub fn refresh(self: &Arc<Self>) -> bool {
        let Some(access) = self.access.clone() else {
            return true;
        };
        if self.lock().job.is_some() {
            return false;
        }
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || match access.facts() {
            SteamFacts::Absent => this.set(None),
            SteamFacts::Unavailable(reason) => this.set(Some(Account::Problem {
                problem: account::Problem::Unavailable(reason),
            })),
            SteamFacts::Ready { plan, runner } => {
                if this.lock().account.is_none() {
                    this.set(Some(Account::Checking));
                }
                this.run(plan, runner, None, STATUS_LIMIT, |account| {
                    !matches!(account, Account::Checking)
                });
            }
        });
        true
    }

    /// A launch of this game hit a Steam Cloud conflict just now.
    pub fn take_conflict(&self, game_id: &str) -> bool {
        self.access
            .as_ref()
            .is_some_and(|access| access.take_conflict(game_id))
    }

    /// Read the account now, and again whenever the Steam package or its
    /// engine runner is installed, enabled or disabled (ADR 0083).
    pub fn start(self: &Arc<Self>) {
        let Some(access) = self.access.clone() else {
            return;
        };
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut seen: Option<String> = None;
            loop {
                let reader = Arc::clone(&access);
                if let Ok(now) = tokio::task::spawn_blocking(move || reader.presence()).await {
                    // A read waits while a sign-in job runs; it is tried again.
                    if seen.as_ref() != Some(&now) && this.refresh() {
                        seen = Some(now);
                    }
                }
                tokio::time::sleep(PRESENCE_POLL).await;
            }
        });
    }

    /// Run one job host until `done` holds for the account, the limit, or a
    /// stop. Publishes each state on the way.
    fn run(
        self: &Arc<Self>,
        plan: HostPlan,
        runner: JobRunner,
        command: Option<Command>,
        limit: Duration,
        done: impl Fn(&Account) -> bool,
    ) {
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.lock();
            if inner.job.is_some() {
                return;
            }
            inner.job = Some(Arc::clone(&stop));
        }
        let outcome = (|| -> Result<(), ()> {
            let mut host: JobHost = runner.start(&plan).map_err(|error| {
                eprintln!("korrid steam account: job host did not start: {error}");
            })?;
            if let Some(command) = command {
                host.send(command).map_err(|error| {
                    eprintln!("korrid steam account: job host took no command: {error}");
                })?;
            }
            let deadline = Instant::now() + limit;
            let unit = host.unit();
            let end = |host: JobHost| {
                let end = host.stop();
                eprintln!("korrid steam account: job host {unit} ended: {end}");
            };
            loop {
                for event in host.poll() {
                    let next = {
                        let inner = self.lock();
                        inner
                            .account
                            .clone()
                            .unwrap_or(Account::Checking)
                            .apply(&event)
                    };
                    match &event {
                        // The account name stays out of the journal.
                        Event::SignedIn { .. } => {
                            eprintln!("korrid steam account: signed in ({unit})")
                        }
                        Event::SignedOut { reason } => eprintln!(
                            "korrid steam account: signed out ({unit}): {}",
                            reason.as_deref().unwrap_or("no reason given")
                        ),
                        _ => {}
                    }
                    let finished = done(&next);
                    self.set(Some(next));
                    if finished {
                        end(host);
                        return Ok(());
                    }
                    if matches!(event, Event::Phase { ref phase } if phase == "stopped") {
                        end(host);
                        return Err(());
                    }
                }
                if stop.load(Ordering::SeqCst) {
                    end(host);
                    return Ok(());
                }
                if Instant::now() >= deadline || !host.running() {
                    eprintln!(
                        "korrid steam account: job host {} before it answered",
                        if host.running() {
                            "ran out of time"
                        } else {
                            "ended"
                        }
                    );
                    end(host);
                    return Err(());
                }
                std::thread::sleep(POLL);
            }
        })();
        let cancelled = stop.load(Ordering::SeqCst);
        let (next, pending) = {
            let mut inner = self.lock();
            inner.job = None;
            let current = inner.account.clone().unwrap_or(Account::Checking);
            let next = match (&outcome, cancelled) {
                (_, true) => Account::SignedOut,
                (Err(()), false) if !done(&current) => match current {
                    // A status job that never answered: nobody is known to
                    // be signed in, and Sign in stays on offer.
                    Account::Checking => Account::SignedOut,
                    other => Account::Problem {
                        problem: account::Problem::SignIn {
                            for_game: other.for_game().map(str::to_owned),
                        },
                    },
                },
                _ => current,
            };
            let pending = match &next {
                Account::SignedIn { .. } | Account::SignedOut | Account::Problem { .. } => {
                    inner.pending.take()
                }
                _ => None,
            };
            (next, pending)
        };
        let signed_in = next.signed_in();
        self.set(Some(next));
        if let Some(pending) = pending {
            if signed_in {
                (pending.proceed)()
            } else {
                (pending.abandon)()
            }
        }
    }

    /// A job that has already answered may still be ending (its host shuts
    /// down and the engine writes its stores). True once none runs.
    async fn job_ended(&self) -> bool {
        let deadline = Instant::now() + JOB_END_WAIT;
        while self.lock().job.is_some() {
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(POLL).await;
        }
        true
    }

    fn sign_in(self: &Arc<Self>, for_game: Option<String>) {
        let Some(access) = self.access.clone() else {
            return;
        };
        let this = Arc::clone(self);
        let korrid = self.korrid.clone();
        tokio::spawn(async move {
            if !this.job_ended().await {
                eprintln!("korrid steam account: sign-in not started: a job host still runs");
                return;
            }
            this.set(Some(Account::Starting {
                for_game: for_game.clone(),
            }));
            // The pinned engine package comes first, through korrid's queue.
            let mut facts = {
                let access = Arc::clone(&access);
                tokio::task::spawn_blocking(move || access.facts()).await
            };
            if matches!(
                facts,
                Ok(SteamFacts::Unavailable(Unavailable::PackageNotDownloaded))
            ) {
                let wanted = {
                    let access = Arc::clone(&access);
                    tokio::task::spawn_blocking(move || access.engine_artifacts())
                        .await
                        .unwrap_or_default()
                };
                let _ = korrid.acquisition_start(wanted).await;
                let deadline = Instant::now() + PACKAGE_LIMIT;
                while matches!(
                    facts,
                    Ok(SteamFacts::Unavailable(Unavailable::PackageNotDownloaded))
                ) && Instant::now() < deadline
                    && this.lock().account.as_ref().is_some_and(|a| {
                        a.for_game() == for_game.as_deref() && matches!(a, Account::Starting { .. })
                    })
                {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let access = Arc::clone(&access);
                    facts = tokio::task::spawn_blocking(move || access.facts()).await;
                }
            }
            let Ok(facts) = facts else { return };
            tokio::task::spawn_blocking(move || this.after_facts(facts));
        });
    }

    fn after_facts(self: &Arc<Self>, facts: SteamFacts) {
        let this = self;
        match facts {
            SteamFacts::Ready { plan, runner } => this.run(
                plan,
                runner,
                Some(Command::SignInQr),
                SIGN_IN_LIMIT,
                |account| matches!(account, Account::SignedIn { .. } | Account::Problem { .. }),
            ),
            SteamFacts::Unavailable(reason) => {
                eprintln!("korrid steam account: sign-in not started: {reason:?}");
                this.set(Some(Account::Problem {
                    problem: account::Problem::Unavailable(reason),
                }));
                if let Some(pending) = this.lock().pending.take() {
                    (pending.abandon)();
                }
            }
            SteamFacts::Absent => {
                eprintln!("korrid steam account: sign-in not started: no Steam engine runner");
                this.set(None)
            }
        }
    }

    fn sign_out(self: &Arc<Self>) {
        let Some(access) = self.access.clone() else {
            return;
        };
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            if let SteamFacts::Ready { plan, runner } = access.facts() {
                this.run(
                    plan,
                    runner,
                    Some(Command::SignOut),
                    STATUS_LIMIT,
                    |account| matches!(account, Account::SignedOut),
                );
            }
        });
    }

    /// A launch waits here when its route is the engine runner and nobody
    /// is signed in. Any other launch goes straight on.
    pub fn gate(
        self: &Arc<Self>,
        game_id: String,
        title: String,
        runner_id: Option<String>,
        proceed: Step,
        abandon: Step,
    ) {
        let Some(access) = self.access.clone() else {
            return proceed();
        };
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let engine = {
                let access = Arc::clone(&access);
                tokio::task::spawn_blocking(move || {
                    access.runs_on_engine(&game_id, runner_id.as_deref())
                })
                .await
                .unwrap_or(false)
            };
            // The account a job reports is final only once its host ended;
            // the ending job would also take this launch's pending steps.
            if engine && !this.job_ended().await {
                eprintln!("korrid steam account: launch not gated: a job host still runs");
                return abandon();
            }
            let wait = engine
                && matches!(
                    this.lock().account,
                    Some(
                        Account::SignedOut
                            | Account::Checking
                            | Account::Problem {
                                problem: account::Problem::SignIn { .. }
                            }
                    )
                );
            if !wait {
                // Signed in, or unavailable: korrid's launch says which.
                return proceed();
            }
            this.lock().pending = Some(Pending { proceed, abandon });
            this.sign_in(Some(title));
        });
    }

    /// A published `steam-account:` action. True when it is one.
    pub fn act(self: &Arc<Self>, id: &str) -> bool {
        match id {
            account::SIGN_IN | account::RETRY => {
                let for_game = self
                    .lock()
                    .account
                    .as_ref()
                    .and_then(Account::for_game)
                    .map(str::to_owned);
                self.sign_in(for_game);
            }
            account::CANCEL => {
                let (job, pending) = {
                    let mut inner = self.lock();
                    (inner.job.clone(), inner.pending.take())
                };
                match job {
                    Some(stop) => stop.store(true, Ordering::SeqCst),
                    None => self.set(Some(Account::SignedOut)),
                }
                if let Some(pending) = pending {
                    (pending.abandon)();
                }
            }
            account::SIGN_OUT => self.sign_out(),
            account::USE_CLIENT => {
                let Some(access) = self.access.clone() else {
                    return true;
                };
                let this = Arc::clone(self);
                tokio::spawn(async move {
                    match access.use_client_runner().await {
                        Ok(()) => this.set(Some(Account::Problem {
                            problem: account::Problem::ClientChosen,
                        })),
                        Err(error) => {
                            eprintln!("korrid steam account: client runner not chosen: {error}")
                        }
                    }
                });
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam_engine::host::{process_units::ProcessHostUnits, tests::script_host};

    struct Access {
        plan: HostPlan,
        runner: JobRunner,
        engine: bool,
        chosen: Arc<AtomicBool>,
        presence: Arc<Mutex<String>>,
    }

    impl SteamEngineAccess for Access {
        fn facts(&self) -> SteamFacts {
            if self.presence.lock().unwrap().is_empty() {
                return SteamFacts::Absent;
            }
            SteamFacts::Ready {
                plan: self.plan.clone(),
                runner: self.runner.clone(),
            }
        }
        fn runs_on_engine(&self, _: &str, _: Option<&str>) -> bool {
            self.engine
        }
        fn use_client_runner(
            &self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>
        {
            self.chosen.store(true, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        }
        fn presence(&self) -> String {
            self.presence.lock().unwrap().clone()
        }
        fn take_conflict(&self, _: &str) -> bool {
            false
        }
        fn engine_artifacts(&self) -> Vec<String> {
            Vec::new()
        }
    }

    fn account(temp: &std::path::Path, body: &str, engine: bool) -> Arc<SteamAccount> {
        account_with(temp, body, engine).0
    }

    type Handles = (Arc<SteamAccount>, Arc<AtomicBool>, Arc<Mutex<String>>);

    fn account_with(temp: &std::path::Path, body: &str, engine: bool) -> Handles {
        let chosen: Arc<AtomicBool> = Arc::default();
        let presence = Arc::new(Mutex::new(String::from("steam|true")));
        let launches = temp.join("launches");
        let access = Access {
            plan: HostPlan {
                host_program: script_host(temp, body),
                engine: temp.join("steamclient.so"),
                proton: None,
                account_root: temp.into(),
            },
            runner: JobRunner {
                units: Arc::new(ProcessHostUnits::new(&launches)),
                launches_root: launches,
                runtime_uid: unsafe { libc::geteuid() },
                live: Arc::default(),
                health: Arc::default(),
            },
            engine,
            chosen: Arc::clone(&chosen),
            presence: Arc::clone(&presence),
        };
        let korrid = super::super::korrid::Korrid::new(Arc::new(|_| {
            Box::pin(async { Err(super::super::korrid::Refusal::Unreachable("test".into())) })
        }))
        .with_steam_engine(Arc::new(access));
        (SteamAccount::new(korrid, Arc::new(|| {})), chosen, presence)
    }

    /// Steam refused the engine: the fallback chooses the client runner and
    /// says so; nothing offers to switch back by itself.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_fallback_chooses_the_client_runner() {
        let temp = tempfile::tempdir().unwrap();
        let (account, chosen, _) = account_with(temp.path(), QR_HOST, true);
        account.set(Some(Account::Problem {
            problem: account::Problem::Unavailable(Unavailable::EngineRefused { eresult: 7 }),
        }));
        assert!(account.act(account::USE_CLIENT));
        until(&account, |s| {
            matches!(s, Some(SurfaceSteamAccount::Problem { kicker, actions, .. })
                if kicker == "Steam games use the Steam client" && actions.is_empty())
        })
        .await;
        assert!(chosen.load(Ordering::SeqCst));
    }

    /// The account is read again when the Steam package or its engine runner
    /// comes or goes, not only at start.
    #[tokio::test(flavor = "multi_thread")]
    async fn installing_or_disabling_the_engine_runner_reads_the_account_again() {
        let temp = tempfile::tempdir().unwrap();
        let (account, _, presence) = account_with(temp.path(), QR_HOST, true);
        *presence.lock().unwrap() = String::new();
        account.start();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(account.snapshot(), None, "no engine runner, no Steam entry");
        *presence.lock().unwrap() = "steam|true".into();
        until(&account, |s| {
            matches!(s, Some(SurfaceSteamAccount::SignedOut { .. }))
        })
        .await;
        *presence.lock().unwrap() = String::new();
        until(&account, Option::is_none).await;
    }

    async fn until(account: &SteamAccount, check: impl Fn(&Option<SurfaceSteamAccount>) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !check(&account.snapshot()) {
            assert!(Instant::now() < deadline, "{:?}", account.snapshot());
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// A host with no saved login (it says `signed_out` at start); `signin-qr`
    /// shows two codes, then signs in.
    const QR_HOST: &str = r#"echo '{"event":"signed_out"}'
exec 3<>"$fifo"
while read -r line <&3; do
  case "$line" in
    signin-qr) echo '{"event":"qr_code","challenge_url":"https://s.team/q/1/1"}'
               echo '{"event":"qr_refreshed","challenge_url":"https://s.team/q/1/2"}'
               sleep 1
               echo '{"event":"logon_state","state":4,"logged_on":true,"connected":true,"offline":false}'
               echo '{"event":"signed_in","account_name":"korri_player"}' ;;
    shutdown) exit 0 ;;
  esac
done"#;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_launch_without_a_sign_in_waits_for_the_qr_and_then_goes_on() {
        let temp = tempfile::tempdir().unwrap();
        let account = account(temp.path(), QR_HOST, true);
        account.refresh();
        until(&account, |s| {
            matches!(s, Some(SurfaceSteamAccount::SignedOut { .. }))
        })
        .await;
        let proceeded = Arc::new(AtomicBool::new(false));
        let abandoned = Arc::new(AtomicBool::new(false));
        let (p, a) = (Arc::clone(&proceeded), Arc::clone(&abandoned));
        account.gate(
            "celeste".into(),
            "Celeste".into(),
            None,
            Arc::new(move || p.store(true, Ordering::SeqCst)),
            Arc::new(move || a.store(true, Ordering::SeqCst)),
        );
        until(&account, |s| {
            matches!(s, Some(SurfaceSteamAccount::Code { for_game: Some(game), .. }) if game == "Celeste")
        })
        .await;
        assert!(
            !proceeded.load(Ordering::SeqCst),
            "the launch waits for the sign-in"
        );
        until(&account, |s| {
            matches!(s, Some(SurfaceSteamAccount::SignedIn { account_name, .. }) if account_name == "korri_player")
        })
        .await;
        // The launch goes on once the job host has shut down, so the engine
        // has written the saved login before the game's own host reads it.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !proceeded.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "the launch did not go on");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!abandoned.load(Ordering::SeqCst));
        assert_eq!(
            std::fs::read_dir(temp.path().join("launches"))
                .unwrap()
                .count(),
            0,
            "the job host is gone"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancel_stops_the_sign_in_and_the_launch_that_waited() {
        let temp = tempfile::tempdir().unwrap();
        let account = account(
            temp.path(),
            r#"exec 3<>"$fifo"
while read -r line <&3; do
  case "$line" in
    signin-qr) echo '{"event":"qr_code","challenge_url":"https://s.team/q/1/1"}' ;;
    shutdown) exit 0 ;;
  esac
done"#,
            true,
        );
        account.set(Some(Account::SignedOut));
        let abandoned = Arc::new(AtomicBool::new(false));
        let a = Arc::clone(&abandoned);
        account.gate(
            "g".into(),
            "Game".into(),
            None,
            Arc::new(|| panic!("a cancelled sign-in never launches")),
            Arc::new(move || a.store(true, Ordering::SeqCst)),
        );
        until(&account, |s| {
            matches!(s, Some(SurfaceSteamAccount::Code { .. }))
        })
        .await;
        assert!(account.act(account::CANCEL));
        until(&account, |s| {
            matches!(s, Some(SurfaceSteamAccount::SignedOut { .. }))
        })
        .await;
        assert!(abandoned.load(Ordering::SeqCst));
        assert!(!account.act("other"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_game_on_the_client_runner_never_waits() {
        let temp = tempfile::tempdir().unwrap();
        let account = account(temp.path(), QR_HOST, false);
        account.set(Some(Account::SignedOut));
        let proceeded = Arc::new(AtomicBool::new(false));
        let p = Arc::clone(&proceeded);
        account.gate(
            "g".into(),
            "Game".into(),
            None,
            Arc::new(move || p.store(true, Ordering::SeqCst)),
            Arc::new(|| {}),
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(proceeded.load(Ordering::SeqCst));
        assert!(matches!(
            account.snapshot(),
            Some(SurfaceSteamAccount::SignedOut { .. })
        ));
    }
}
