//! `SurfaceModel.accountSignIns` for plugins that declare an `account` (ADR
//! 0089): the generic account host. Steam's entry comes from
//! `steam_account.rs`.
//!
//! The list is the plugin host's projection of every installed plugin, on or
//! off (`PluginRepositories`): its title, setup offer and logo. A plugin that
//! is off is signed out; Sign in turns it on (`set_plugin_enabled`) and then
//! asks its program for a code. korrid talks to the program only while the
//! plugin's first service runs, so the program always runs as that
//! service's user. Every action runs through `RunAction`.
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use korri_surface_treaty::{SurfaceAccountSignIn, SurfaceMedia, SurfaceMediaKind};

use super::Notify;
use crate::{
    account_sign_in::{
        job::{AccountHosts, Job},
        protocol::{self, Command, Event},
        state::{self, Account, Problem},
    },
    plugin_admin::PluginAccount,
    PluginChangeOutcome, PluginRepositoriesOutcome,
};

/// korrid's side, read in process.
pub trait PluginAccountAccess: Send + Sync {
    /// Cheap: changes whenever the plugin host publishes a new selection.
    fn presence(&self) -> String;
    /// Blocking: the unit name of the enabled plugin's first declared
    /// service, beside which the plugin host runs its account program.
    fn service(&self, plugin: &str) -> Option<String>;
    /// Blocking: whether that unit is active.
    fn active(&self, service: &str) -> bool;
    fn hosts(&self) -> Arc<dyn AccountHosts>;
}

/// A QR session lasts at most this long.
const SIGN_IN_LIMIT: Duration = Duration::from_secs(15 * 60);
/// A status job and sign-out wait at most this long for the program.
const STATUS_LIMIT: Duration = Duration::from_secs(60);
/// After the plugin is turned on, its service gets this long to start.
const SERVICE_WAIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(200);
const PRESENCE_POLL: Duration = Duration::from_secs(2);
/// A signed-in account is asked again this often, so a sign-in that the
/// service ended shows as needing sign-in (`signed_out{reason: "expired"}`).
const RECHECK: Duration = Duration::from_secs(10 * 60);

struct Entry {
    title: String,
    offer: Option<String>,
    logo: Option<SurfaceMedia>,
    enabled: bool,
    account: Account,
    /// Set while a job runs; storing true stops it.
    job: Option<Arc<AtomicBool>>,
}

pub struct PluginAccounts {
    korrid: super::korrid::Korrid,
    access: Option<Arc<dyn PluginAccountAccess>>,
    entries: Mutex<BTreeMap<String, Entry>>,
    notify: Notify,
    /// Reads of the projection run one at a time.
    reading: tokio::sync::Mutex<()>,
    service_wait: Duration,
}

impl PluginAccounts {
    pub fn new(korrid: super::korrid::Korrid, notify: Notify) -> Arc<Self> {
        Arc::new(Self {
            access: korrid.plugin_accounts.clone(),
            korrid,
            entries: Mutex::new(BTreeMap::new()),
            notify,
            reading: tokio::sync::Mutex::new(()),
            service_wait: SERVICE_WAIT,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Entry>> {
        self.entries.lock().expect("plugin accounts poisoned")
    }

    /// One entry per plugin, in plugin id order.
    pub fn snapshot(&self) -> Vec<SurfaceAccountSignIn> {
        self.lock()
            .iter()
            .map(|(id, entry)| SurfaceAccountSignIn {
                id: id.clone(),
                title: entry.title.clone(),
                logo: entry.logo.clone(),
                offer: entry.offer.clone(),
                state: entry.account.surface(id, &entry.title),
            })
            .collect()
    }

    fn set(&self, plugin: &str, account: Account) {
        if let Some(entry) = self.lock().get_mut(plugin) {
            entry.account = account;
        }
        (self.notify)();
    }

    fn account(&self, plugin: &str) -> Option<Account> {
        self.lock().get(plugin).map(|entry| entry.account.clone())
    }

    /// Read the projection now, and again whenever the plugin host
    /// publishes a new selection. Ask each signed-in account again every
    /// `RECHECK`.
    pub fn start(self: &Arc<Self>) {
        let Some(access) = self.access.clone() else {
            return;
        };
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut seen: Option<String> = None;
            let mut checked = Instant::now();
            loop {
                let reader = Arc::clone(&access);
                if let Ok(now) = tokio::task::spawn_blocking(move || reader.presence()).await {
                    if seen.as_ref() != Some(&now) {
                        seen = Some(now);
                        Arc::clone(&this).refresh().await;
                    }
                }
                if checked.elapsed() >= RECHECK {
                    checked = Instant::now();
                    this.recheck();
                }
                tokio::time::sleep(PRESENCE_POLL).await;
            }
        });
    }

    /// Ask every signed-in account who is signed in, without changing what
    /// the surface shows until the program answers.
    fn recheck(self: &Arc<Self>) {
        let signed_in = self
            .lock()
            .iter()
            .filter(|(_, entry)| entry.enabled && entry.job.is_none() && entry.account.signed_in())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for plugin in signed_in {
            let this = Arc::clone(self);
            tokio::task::spawn_blocking(move || {
                // The first answer ends it; no answer keeps the account as
                // it is.
                this.run(&plugin, Command::Status, STATUS_LIMIT, |_| true)
            });
        }
    }

    /// Read the installed plugins' account sign-ins. A plugin that is new here, or
    /// was turned on, is asked who is signed in.
    pub async fn refresh(self: Arc<Self>) {
        if self.access.is_none() {
            return;
        }
        let _reading = self.reading.lock().await;
        let PluginRepositoriesOutcome::Ok(repositories) = self.korrid.plugin_repositories().await
        else {
            // Nothing is known: the list stays as it was.
            return;
        };
        let installs = repositories
            .repository_installs
            .into_iter()
            .map(|install| (install.id, install.enabled, install.account))
            .chain(
                repositories
                    .other_installs
                    .into_iter()
                    .map(|install| (install.id, install.enabled, install.account)),
            )
            .filter_map(|(id, enabled, account)| Some((id, enabled, account?)))
            // Steam has its own entry.
            .filter(|(id, _, _)| !crate::steam_engine::is_steam_plugin(id))
            .collect::<Vec<_>>();
        let mut check = Vec::new();
        {
            let mut entries = self.lock();
            entries.retain(|id, _| installs.iter().any(|(installed, _, _)| installed == id));
            for (id, enabled, declared) in installs {
                let PluginAccount { title, offer, logo } = declared;
                let logo = logo.as_deref().and_then(logo_media);
                match entries.get_mut(&id) {
                    Some(entry) => {
                        let turned_on = enabled && !entry.enabled;
                        entry.title = title;
                        entry.offer = offer;
                        entry.logo = logo;
                        entry.enabled = enabled;
                        // A sign-in that is turning the plugin on owns it.
                        if entry.job.is_none() && entry.account != Account::Starting {
                            if !enabled {
                                entry.account = Account::Idle;
                            } else if turned_on {
                                entry.account = Account::Checking;
                                check.push(id);
                            }
                        }
                    }
                    None => {
                        if enabled {
                            check.push(id.clone());
                        }
                        entries.insert(
                            id,
                            Entry {
                                title,
                                offer,
                                logo,
                                enabled,
                                account: if enabled {
                                    Account::Checking
                                } else {
                                    Account::Idle
                                },
                                job: None,
                            },
                        );
                    }
                }
            }
        }
        (self.notify)();
        for plugin in check {
            let this = Arc::clone(&self);
            tokio::task::spawn_blocking(move || {
                this.run(&plugin, Command::Status, STATUS_LIMIT, |account| {
                    !matches!(account, Account::Checking)
                })
            });
        }
    }

    /// Run one job for `plugin` until `done` holds, the limit, a stop or the
    /// program's end. Blocking. Publishes each state on the way.
    fn run(
        self: &Arc<Self>,
        plugin: &str,
        command: Command,
        limit: Duration,
        done: impl Fn(&Account) -> bool,
    ) {
        let Some(access) = self.access.clone() else {
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut entries = self.lock();
            let Some(entry) = entries.get_mut(plugin) else {
                return;
            };
            if entry.job.is_some() {
                return;
            }
            entry.job = Some(Arc::clone(&stop));
        }
        let outcome = (|| -> Result<(), Problem> {
            // The program runs as the service's own user: it must be up. The
            // service is looked up once the registry has the plugin (a read
            // evaluates every plugin), then watched.
            let deadline = Instant::now() + self.service_wait;
            let mut service: Option<String> = None;
            loop {
                if service.is_none() {
                    service = access.service(plugin);
                }
                if service.as_deref().is_some_and(|unit| access.active(unit)) {
                    break;
                }
                if stop.load(Ordering::SeqCst) {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    eprintln!("korrid account sign-in: {plugin}: its service is not running");
                    return Err(Problem::NotRunning);
                }
                std::thread::sleep(if service.is_none() {
                    Duration::from_secs(1)
                } else {
                    POLL
                });
            }
            let mut job = Job::start(access.hosts().as_ref(), plugin).map_err(|error| {
                eprintln!("korrid account sign-in: {plugin}: no account host: {error}");
                Problem::SignIn
            })?;
            job.send(command).map_err(|error| {
                eprintln!(
                    "korrid account sign-in: {plugin}: the account host took no command: {error}"
                );
                Problem::SignIn
            })?;
            let deadline = Instant::now() + limit;
            loop {
                for event in job.poll() {
                    let current = self.account(plugin).unwrap_or(Account::Checking);
                    let next = current.apply(&event);
                    match &event {
                        // The account name and the link stay out of the journal.
                        Event::SignedIn { .. } => {
                            eprintln!("korrid account sign-in: {plugin}: signed in")
                        }
                        Event::SignedOut { reason } => eprintln!(
                            "korrid account sign-in: {plugin}: signed out: {}",
                            reason
                                .as_deref()
                                .map(protocol::loggable)
                                .unwrap_or_else(|| "no reason given".into())
                        ),
                        Event::SigninFailed { reason } => eprintln!(
                            "korrid account sign-in: {plugin}: sign-in failed: {}",
                            protocol::loggable(reason)
                        ),
                        _ => {}
                    }
                    let finished = done(&next);
                    self.set(plugin, next);
                    if finished {
                        return Ok(());
                    }
                }
                if stop.load(Ordering::SeqCst) {
                    return Ok(());
                }
                if job.overflowed() {
                    eprintln!(
                        "korrid account sign-in: {plugin}: the account host wrote a line longer than korrid reads; the job ended"
                    );
                    return Err(Problem::BadOutput);
                }
                if Instant::now() >= deadline || !job.running() {
                    eprintln!(
                        "korrid account sign-in: {plugin}: the account host {} before it answered",
                        if job.running() {
                            "ran out of time"
                        } else {
                            "ended"
                        }
                    );
                    return Err(Problem::SignIn);
                }
            }
        })();
        let cancelled = stop.load(Ordering::SeqCst);
        {
            let mut entries = self.lock();
            let Some(entry) = entries.get_mut(plugin) else {
                return;
            };
            entry.job = None;
            let current = entry.account.clone();
            entry.account = match (outcome, cancelled) {
                (_, true) => Account::SignedOut { expired: false },
                (Err(problem), false) if !done(&current) => match current {
                    // A status job that never answered: nobody is known to be
                    // signed in, and Sign in stays on offer.
                    Account::Checking if problem == Problem::SignIn => {
                        Account::SignedOut { expired: false }
                    }
                    _ => Account::Problem { problem },
                },
                _ => current,
            };
        }
        (self.notify)();
    }

    fn sign_in(self: &Arc<Self>, plugin: String) {
        let enabled = {
            let mut entries = self.lock();
            let Some(entry) = entries.get_mut(&plugin) else {
                return;
            };
            if entry.job.is_some() {
                return;
            }
            entry.account = Account::Starting;
            entry.enabled
        };
        (self.notify)();
        let this = Arc::clone(self);
        tokio::spawn(async move {
            if !enabled {
                match this.korrid.set_plugin_enabled(plugin.clone(), true).await {
                    PluginChangeOutcome::Ok(_) => {
                        if let Some(entry) = this.lock().get_mut(&plugin) {
                            entry.enabled = true;
                        }
                    }
                    // The same request and approval checks as the Settings
                    // switch (`plugins.rs` toggle): the plugin host enables
                    // only an approved receipt whose package still matches.
                    PluginChangeOutcome::Err(failure) => {
                        eprintln!(
                            "korrid account sign-in: {plugin}: not turned on: {}",
                            failure.message
                        );
                        let problem = if failure.code == super::plugins::NEEDS_REVIEW_CODE {
                            Problem::NeedsReview
                        } else {
                            Problem::NotEnabled
                        };
                        this.set(&plugin, Account::Problem { problem });
                        return;
                    }
                }
            }
            // A Cancel while the plugin was turned on ends the sign-in here.
            if this.account(&plugin) != Some(Account::Starting) {
                return;
            }
            tokio::task::spawn_blocking(move || {
                this.run(&plugin, Command::SignInQr, SIGN_IN_LIMIT, |account| {
                    matches!(account, Account::SignedIn { .. } | Account::Problem { .. })
                })
            });
        });
    }

    fn sign_out(self: &Arc<Self>, plugin: String) {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            this.run(&plugin, Command::SignOut, STATUS_LIMIT, |account| {
                matches!(account, Account::SignedOut { .. })
            })
        });
    }

    /// A published `account-sign-in:` action. True when it is one.
    pub fn act(self: &Arc<Self>, id: &str) -> bool {
        let Some((verb, plugin)) = state::parse_action(id) else {
            return false;
        };
        let plugin = plugin.to_owned();
        match verb {
            state::SIGN_IN | state::RETRY => self.sign_in(plugin),
            state::CANCEL => {
                let job = {
                    let mut entries = self.lock();
                    let Some(entry) = entries.get_mut(&plugin) else {
                        return true;
                    };
                    if entry.job.is_none() {
                        entry.account = Account::SignedOut { expired: false };
                    }
                    entry.job.clone()
                };
                match job {
                    Some(stop) => stop.store(true, Ordering::SeqCst),
                    None => (self.notify)(),
                }
            }
            state::SIGN_OUT => self.sign_out(plugin),
            _ => {}
        }
        true
    }
}

/// The plugin's logo as the model carries it: an SVG `data:` URL, which the
/// shell renders itself, with the size the SVG gives. `None` for a file
/// outside the store, a large file, or one whose size cannot be read.
fn logo_media(path: &str) -> Option<SurfaceMedia> {
    use base64::Engine as _;
    let path = std::path::Path::new(path);
    if !path.starts_with("/nix/store") {
        return None;
    }
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > crate::plugin_references::MAX_LOGO_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let (width, height) = svg_size(std::str::from_utf8(&bytes).ok()?)?;
    Some(SurfaceMedia {
        kind: SurfaceMediaKind::Logo,
        url: format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ),
        width,
        height,
        color: None,
    })
}

/// The root `<svg>` element's `width` and `height`, or its `viewBox` size.
fn svg_size(text: &str) -> Option<(u32, u32)> {
    let start = text.find("<svg")?;
    let tag = &text[start..start + text[start..].find('>')?];
    let attribute = |name: &str| -> Option<&str> {
        let mut rest = tag;
        loop {
            let at = rest.find(name)?;
            let before = rest[..at].chars().last();
            rest = &rest[at + name.len()..];
            let after = rest.trim_start();
            if before.is_some_and(char::is_whitespace) {
                if let Some(value) = after.strip_prefix('=') {
                    let value = value.trim_start();
                    let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
                    let value = &value[1..];
                    return Some(&value[..value.find(quote)?]);
                }
            }
        }
    };
    let number = |value: &str| -> Option<f64> {
        value
            .trim()
            .trim_end_matches("px")
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0)
    };
    let (width, height) = match (
        attribute("width").and_then(number),
        attribute("height").and_then(number),
    ) {
        (Some(width), Some(height)) => (width, height),
        _ => {
            let parts = attribute("viewBox")?
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|part| !part.is_empty())
                .map(|part| part.parse::<f64>().ok())
                .collect::<Option<Vec<_>>>()?;
            let [_, _, width, height] = parts[..] else {
                return None;
            };
            if !(width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite()) {
                return None;
            }
            (width, height)
        }
    };
    let side = |n: f64| (n.ceil() as u32).clamp(1, 4096);
    Some((side(width), side(height)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        account_sign_in::job::process::{script, ProcessHosts},
        plugin_admin::{PluginChangeApplied, PluginOtherInstall, PluginRepositories},
        RpcFailure, RpcRequest, RpcResponse,
    };
    use korri_surface_treaty::SurfaceAccountState;

    const ID: &str = "@korri:tailscale";

    struct Access {
        hosts: Arc<ProcessHosts>,
        active: Arc<AtomicBool>,
        presence: Arc<Mutex<String>>,
    }

    impl PluginAccountAccess for Access {
        fn presence(&self) -> String {
            self.presence.lock().unwrap().clone()
        }
        fn service(&self, _: &str) -> Option<String> {
            Some("tailscaled.service".into())
        }
        fn active(&self, service: &str) -> bool {
            assert_eq!(service, "tailscaled.service");
            self.active.load(Ordering::SeqCst)
        }
        fn hosts(&self) -> Arc<dyn AccountHosts> {
            self.hosts.clone()
        }
    }

    /// The fake plugin host: one install of `ID`, on or off; enabling it
    /// succeeds unless `refuse` is set.
    struct Fake {
        enabled: Arc<AtomicBool>,
        refuse: Arc<AtomicBool>,
        active: Arc<AtomicBool>,
        presence: Arc<Mutex<String>>,
        accounts: Arc<PluginAccounts>,
    }

    fn fake(temp: &std::path::Path, body: &str, enabled: bool) -> Fake {
        let enabled = Arc::new(AtomicBool::new(enabled));
        let refuse = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicBool::new(enabled.load(Ordering::SeqCst)));
        let presence = Arc::new(Mutex::new("v1".into()));
        let (on, no, up) = (
            Arc::clone(&enabled),
            Arc::clone(&refuse),
            Arc::clone(&active),
        );
        let korrid = super::super::korrid::Korrid::new(Arc::new(move |request| {
            let (on, no, up) = (Arc::clone(&on), Arc::clone(&no), Arc::clone(&up));
            Box::pin(async move {
                Ok(match request {
                    RpcRequest::PluginRepositories(_) => RpcResponse::PluginRepositories(
                        PluginRepositoriesOutcome::Ok(PluginRepositories {
                            official: None,
                            user_added: vec![],
                            repository_installs: vec![],
                            other_installs: vec![PluginOtherInstall {
                                id: ID.into(),
                                title: Some("Tailscale".into()),
                                enabled: on.load(Ordering::SeqCst),
                                account: Some(PluginAccount {
                                    title: "Tailscale".into(),
                                    offer: Some(
                                        "Private network: reach your devices from anywhere".into(),
                                    ),
                                    logo: None,
                                }),
                            }],
                        }),
                    ),
                    RpcRequest::PluginSetEnabled(request) => {
                        assert_eq!(request.id, ID);
                        if no.load(Ordering::SeqCst) {
                            RpcResponse::PluginSetEnabled(PluginChangeOutcome::Err(RpcFailure {
                                code: super::super::plugins::NEEDS_REVIEW_CODE.into(),
                                message: "changed since approval".into(),
                            }))
                        } else {
                            on.store(request.enabled, Ordering::SeqCst);
                            // The service comes up a little after the enable.
                            let up = Arc::clone(&up);
                            tokio::spawn(async move {
                                tokio::time::sleep(Duration::from_millis(300)).await;
                                up.store(true, Ordering::SeqCst);
                            });
                            RpcResponse::PluginSetEnabled(PluginChangeOutcome::Ok(
                                PluginChangeApplied {},
                            ))
                        }
                    }
                    _ => return Err(super::super::korrid::Refusal::Unreachable("test".into())),
                })
            })
        }))
        .with_plugin_accounts(Arc::new(Access {
            hosts: Arc::new(ProcessHosts {
                program: script(temp, body),
            }),
            active: Arc::clone(&active),
            presence: Arc::clone(&presence),
        }));
        let mut accounts = PluginAccounts::new(korrid, Arc::new(|| {}));
        Arc::get_mut(&mut accounts).unwrap().service_wait = Duration::from_secs(2);
        Fake {
            enabled,
            refuse,
            active,
            presence,
            accounts,
        }
    }

    async fn until(accounts: &PluginAccounts, check: impl Fn(&SurfaceAccountState) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = accounts.snapshot();
            if snapshot.first().is_some_and(|entry| check(&entry.state)) {
                return;
            }
            assert!(Instant::now() < deadline, "{snapshot:?}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Tailscale's program, as korri-plugins ships it: `status` answers at
    /// once; `signin-qr` shows a link, a new one, then the sign-in.
    const HOST: &str = r#"while read -r line; do
  case "$line" in
    status) if [ -e "$0.in" ]; then echo '{"event":"signed_in","account_name":"p@example.com"}'
            else echo '{"event":"signed_out","reason":"expired"}'; fi ;;
    signin-qr) echo '{"event":"qr_code","challenge_url":"https://login.example/a/1"}'
               sleep 0.3
               echo '{"event":"qr_refreshed","challenge_url":"https://login.example/a/2"}'
               sleep 0.3
               touch "$0.in"
               echo '{"event":"signed_in","account_name":"p@example.com"}' ;;
    signout) rm -f "$0.in"; echo '{"event":"signed_out"}' ;;
  esac
done"#;

    #[tokio::test(flavor = "multi_thread")]
    async fn an_idle_plugin_is_signed_out_and_sign_in_turns_it_on_then_shows_its_code() {
        let temp = tempfile::tempdir().unwrap();
        let fake = fake(temp.path(), HOST, false);
        Arc::clone(&fake.accounts).refresh().await;
        let entry = &fake.accounts.snapshot()[0];
        assert_eq!(entry.id, ID);
        assert_eq!(entry.title, "Tailscale");
        assert_eq!(
            entry.offer.as_deref(),
            Some("Private network: reach your devices from anywhere")
        );
        assert_eq!(
            entry.state,
            SurfaceAccountState::SignedOut {
                kicker: None,
                actions: vec![korri_surface_treaty::SurfaceAction::new(
                    state::action_id(state::SIGN_IN, ID),
                    "Sign in",
                    true
                )],
            }
        );
        assert!(fake.accounts.act(&state::action_id(state::SIGN_IN, ID)));
        until(&fake.accounts, |s| {
            matches!(s, SurfaceAccountState::Code { .. })
        })
        .await;
        assert!(fake.enabled.load(Ordering::SeqCst), "Sign in turned it on");
        assert!(fake.active.load(Ordering::SeqCst));
        until(&fake.accounts, |s| {
            matches!(s, SurfaceAccountState::SignedIn { account_name, .. } if account_name == "p@example.com")
        })
        .await;
        // A new selection is read again; the plugin stays on and signed in.
        *fake.presence.lock().unwrap() = "v2".into();
        Arc::clone(&fake.accounts).refresh().await;
        until(&fake.accounts, |s| {
            matches!(s, SurfaceAccountState::SignedIn { .. })
        })
        .await;
        assert!(fake.accounts.act(&state::action_id(state::SIGN_OUT, ID)));
        until(&fake.accounts, |s| {
            matches!(s, SurfaceAccountState::SignedOut { kicker: None, .. })
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_enabled_plugin_whose_sign_in_expired_needs_sign_in() {
        let temp = tempfile::tempdir().unwrap();
        let fake = fake(temp.path(), HOST, true);
        Arc::clone(&fake.accounts).refresh().await;
        until(&fake.accounts, |s| {
            matches!(s, SurfaceAccountState::SignedOut { kicker: Some(kicker), .. }
                if kicker == "Tailscale needs you to sign in again")
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_service_that_is_not_running_is_a_problem_with_retry() {
        let temp = tempfile::tempdir().unwrap();
        let fake = fake(temp.path(), HOST, true);
        fake.active.store(false, Ordering::SeqCst);
        // The status job waits `service_wait` (2 s here), then gives up.
        let accounts = Arc::clone(&fake.accounts);
        Arc::clone(&accounts).refresh().await;
        assert!(matches!(
            accounts.snapshot()[0].state,
            SurfaceAccountState::Working { .. }
        ));
        until(&accounts, |s| {
            matches!(s, SurfaceAccountState::Problem { kicker, actions, .. }
                if kicker == "Tailscale is starting" && actions[0].id == state::action_id(state::RETRY, ID))
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_enable_is_a_problem_and_cancel_ends_a_code() {
        let temp = tempfile::tempdir().unwrap();
        let fake = fake(temp.path(), HOST, false);
        fake.refuse.store(true, Ordering::SeqCst);
        Arc::clone(&fake.accounts).refresh().await;
        fake.accounts.act(&state::action_id(state::SIGN_IN, ID));
        until(&fake.accounts, |s| {
            matches!(s, SurfaceAccountState::Problem { kicker, .. } if kicker == "Tailscale needs your review")
        })
        .await;
        fake.refuse.store(false, Ordering::SeqCst);
        let slow = fake_slow(temp.path());
        Arc::clone(&slow.accounts).refresh().await;
        until(&slow.accounts, |s| {
            matches!(s, SurfaceAccountState::SignedOut { .. })
        })
        .await;
        slow.accounts.act(&state::action_id(state::RETRY, ID));
        until(&slow.accounts, |s| {
            matches!(s, SurfaceAccountState::Code { .. })
        })
        .await;
        assert!(slow.accounts.act(&state::action_id(state::CANCEL, ID)));
        until(&slow.accounts, |s| {
            matches!(s, SurfaceAccountState::SignedOut { kicker: None, .. })
        })
        .await;
        assert!(!slow.accounts.act("steam-account:cancel"));
    }

    /// A program that floods its output: the job ends, and the sign-in is a
    /// Problem, not a guess that nobody is signed in.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_program_that_floods_its_output_is_a_problem() {
        let temp = tempfile::tempdir().unwrap();
        let fake = fake(temp.path(), "exec tr -d '\\n' < /dev/zero", true);
        Arc::clone(&fake.accounts).refresh().await;
        until(&fake.accounts, |s| {
            matches!(s, SurfaceAccountState::Problem { reason, actions, .. }
                if reason == "Tailscale answered in a way Korri cannot read."
                    && actions[0].id == state::action_id(state::RETRY, ID))
        })
        .await;
    }

    /// A program that shows a code and never finishes.
    fn fake_slow(temp: &std::path::Path) -> Fake {
        fake(
            temp,
            r#"while read -r line; do
  case "$line" in
    status) echo '{"event":"signed_out"}' ;;
    signin-qr) echo '{"event":"qr_code","challenge_url":"https://login.example/a/1"}' ;;
  esac
done"#,
            true,
        )
    }

    #[test]
    fn a_logo_is_an_svg_data_url_with_the_svgs_own_size() {
        assert_eq!(
            svg_size(
                r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="48px" height='24'>"#
            ),
            Some((48, 24))
        );
        assert_eq!(
            svg_size(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120.5 60">"#),
            Some((121, 60))
        );
        assert_eq!(svg_size("<svg>"), None);
        assert_eq!(svg_size("not svg"), None);
        // Only the store's files are read.
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("logo.svg");
        std::fs::write(&file, r#"<svg width="1" height="1"/>"#).unwrap();
        assert_eq!(logo_media(file.to_str().unwrap()), None);
    }
}
