//! The Steam provider in korrid's store RPCs (ADR 0083, contract "Provider
//! and installs"). `@korri:steam` declares the provider; korrid serves its
//! search from the host's `list-apps` and runs Get as `install <appid>` in a
//! job-mode host: one job at a time, with progress and Cancel. A finished
//! install is found by discovery from its appmanifest; Remove download runs
//! `uninstall <appid>`.
use super::{
    host::HostPlan,
    jobs::JobRunner,
    protocol::{AppId, Command, Event},
    provider::{appid, AppList, Job},
};
use crate::{
    acquisition::{
        claims::{ProviderClaim, ProviderClaimDetails, ProviderClaimDetailsTag, ProviderClaimTag},
        AcquireJobState, AcquireJobStatus, SearchResponse,
    },
    RpcFailure,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// A listing is reused this long; Get and Remove download clear it.
const LIST_FRESH: Duration = Duration::from_secs(5 * 60);
const LIST_LIMIT: Duration = Duration::from_secs(120);
/// The host's own install limit is 3600 s by default; korrid allows more.
const INSTALL_LIMIT: Duration = Duration::from_secs(4 * 3600);
const POLL: Duration = Duration::from_millis(250);

fn failure(code: &str, message: &str) -> RpcFailure {
    RpcFailure {
        code: code.into(),
        message: message.into(),
    }
}

fn unavailable() -> RpcFailure {
    failure(
        super::availability::Unavailable::CODE,
        "Steam is not available on this device right now.",
    )
}

/// The store page Korri links for an app: Steam's own, which a person
/// opens on another device to buy a game.
fn store_url(app: AppId) -> String {
    format!("https://store.steampowered.com/app/{app}/")
}

struct StoreJob {
    status: Mutex<AcquireJobStatus>,
    stop: AtomicBool,
}

/// After an install: rescan the library and find the game's id by its
/// provider link `{@korri:steam, <appid>}`.
pub type Imported = Arc<dyn Fn(AppId) -> Option<String> + Send + Sync>;

#[derive(Default)]
pub struct SteamStore {
    list: Mutex<Option<(Instant, Arc<AppList>)>>,
    jobs: Mutex<BTreeMap<String, Arc<StoreJob>>>,
    /// One engine job at a time on this device.
    turn: tokio::sync::Mutex<()>,
}

impl SteamStore {
    async fn list(&self, plan: HostPlan, runner: JobRunner) -> Result<Arc<AppList>, RpcFailure> {
        if let Some((at, list)) = &*self.list.lock().expect("steam list poisoned") {
            if at.elapsed() < LIST_FRESH {
                return Ok(Arc::clone(list));
            }
        }
        let _turn = self.turn.lock().await;
        let list = tokio::task::spawn_blocking(move || {
            let mut host = runner.start(&plan).map_err(|_| unavailable())?;
            host.send(Command::ListApps).map_err(|_| unavailable())?;
            let mut list = AppList::default();
            let deadline = Instant::now() + LIST_LIMIT;
            loop {
                for event in host.poll() {
                    if let Event::SigninFailed { .. } | Event::SignedOut {} = event {
                        return Err(failure(
                            "SteamSignInNeeded",
                            "Sign in to Steam in Settings to see your games.",
                        ));
                    }
                    if list.apply(&event) {
                        return Ok(list);
                    }
                }
                if Instant::now() >= deadline || !host.running() {
                    return Err(failure(
                        "ProviderUnavailable",
                        "Steam did not send your games. Try again.",
                    ));
                }
                std::thread::sleep(POLL);
            }
        })
        .await
        .map_err(|_| unavailable())??;
        let list = Arc::new(list);
        *self.list.lock().expect("steam list poisoned") = Some((Instant::now(), Arc::clone(&list)));
        Ok(list)
    }

    pub async fn search(
        &self,
        plan: HostPlan,
        runner: JobRunner,
        query: &str,
    ) -> Result<SearchResponse, RpcFailure> {
        let list = self.list(plan, runner).await?;
        Ok(SearchResponse {
            claims: list
                .search(query)
                .into_iter()
                .map(|entry| ProviderClaim {
                    tag: ProviderClaimTag::ProviderClaim,
                    provider_id: super::provider::PROVIDER_ID.into(),
                    url: store_url(appid(&entry.id).unwrap_or_default()),
                    id: entry.id,
                    title: entry.title,
                    platform: entry.system,
                    thumbnail_url: None,
                    artifact: None,
                    fetched_at: None,
                })
                .collect(),
        })
    }

    pub async fn details(
        &self,
        plan: HostPlan,
        runner: JobRunner,
        id: &str,
    ) -> Result<ProviderClaimDetails, RpcFailure> {
        let app = appid(id)
            .ok_or_else(|| failure("UnknownClaim", "This game is not in your Steam library."))?;
        let list = self.list(plan, runner).await?;
        let found = list
            .apps()
            .find(|candidate| candidate.appid == app)
            .ok_or_else(|| failure("UnknownClaim", "This game is not in your Steam library."))?;
        Ok(ProviderClaimDetails {
            tag: ProviderClaimDetailsTag::ProviderClaimDetails,
            provider_id: super::provider::PROVIDER_ID.into(),
            id: id.into(),
            title: found.name.clone(),
            url: store_url(app),
            description: None,
            download_page_url: None,
            artifact: None,
            fetched_at: None,
        })
    }

    /// Get: queue `install <appid>` and return the job at once.
    pub fn acquire(
        self: &Arc<Self>,
        plan: HostPlan,
        runner: JobRunner,
        id: &str,
        imported: Imported,
    ) -> Result<AcquireJobStatus, RpcFailure> {
        let app = appid(id)
            .ok_or_else(|| failure("UnknownClaim", "This game is not in your Steam library."))?;
        let job_id = format!("{:032x}", rand::random::<u128>());
        let status = AcquireJobStatus {
            job_id: job_id.clone(),
            provider_id: super::provider::PROVIDER_ID.into(),
            id: id.into(),
            file_name: None,
            system: Some("steam".into()),
            message: None,
            state: AcquireJobState::Queued,
        };
        let job = Arc::new(StoreJob {
            status: Mutex::new(status.clone()),
            stop: AtomicBool::new(false),
        });
        self.jobs
            .lock()
            .expect("steam jobs poisoned")
            .insert(job_id, Arc::clone(&job));
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let _turn = this.turn.lock().await;
            *this.list.lock().expect("steam list poisoned") = None;
            if job.stop.load(Ordering::SeqCst) {
                return;
            }
            let worker = Arc::clone(&job);
            let _ = tokio::task::spawn_blocking(move || {
                run_app_job(
                    &worker,
                    plan,
                    runner,
                    Command::Install(app),
                    INSTALL_LIMIT,
                    &imported,
                )
            })
            .await;
        });
        Ok(status)
    }

    pub fn status(&self, job_id: &str) -> Option<AcquireJobStatus> {
        let jobs = self.jobs.lock().expect("steam jobs poisoned");
        let job = jobs.get(job_id)?;
        let status = job.status.lock().expect("steam job poisoned").clone();
        Some(status)
    }

    pub fn cancel(&self, job_id: &str) -> Option<AcquireJobStatus> {
        let job = self
            .jobs
            .lock()
            .expect("steam jobs poisoned")
            .get(job_id)
            .cloned()?;
        job.stop.store(true, Ordering::SeqCst);
        let mut status = job.status.lock().expect("steam job poisoned");
        if matches!(
            status.state,
            AcquireJobState::Queued
                | AcquireJobState::Resolving
                | AcquireJobState::Downloading { .. }
        ) {
            status.state = AcquireJobState::Cancelled;
        }
        Some(status.clone())
    }

    /// Remove download: `uninstall <appid>`, waited for. Saves stay: they
    /// are in Steam Cloud and in Korri's history, never in this job's way.
    pub async fn uninstall(
        &self,
        plan: HostPlan,
        runner: JobRunner,
        app: AppId,
    ) -> Result<(), RpcFailure> {
        let _turn = self.turn.lock().await;
        *self.list.lock().expect("steam list poisoned") = None;
        let job = Arc::new(StoreJob {
            status: Mutex::new(AcquireJobStatus {
                job_id: String::new(),
                provider_id: super::provider::PROVIDER_ID.into(),
                id: app.to_string(),
                file_name: None,
                system: None,
                message: None,
                state: AcquireJobState::Queued,
            }),
            stop: AtomicBool::new(false),
        });
        let worker = Arc::clone(&job);
        let none: Imported = Arc::new(|_| None);
        tokio::task::spawn_blocking(move || {
            run_app_job(
                &worker,
                plan,
                runner,
                Command::Uninstall(app),
                LIST_LIMIT,
                &none,
            )
        })
        .await
        .map_err(|_| unavailable())?;
        let state = job.status.lock().expect("steam job poisoned").state.clone();
        match state {
            AcquireJobState::Imported { .. } => Ok(()),
            AcquireJobState::Failed { reason } => Err(reason),
            _ => Err(failure(
                "RemoveFailed",
                "Steam did not remove this game. Try again.",
            )),
        }
    }
}

/// One install or uninstall in a job host, its state kept in `job`.
fn run_app_job(
    job: &StoreJob,
    plan: HostPlan,
    runner: JobRunner,
    command: Command,
    limit: Duration,
    imported: &Imported,
) {
    let app = match command {
        Command::Install(app) | Command::Uninstall(app) => app,
        _ => return,
    };
    let set = |state: AcquireJobState| {
        let mut status = job.status.lock().expect("steam job poisoned");
        if !matches!(status.state, AcquireJobState::Cancelled) {
            status.state = state;
        }
    };
    let failed = |message: &str| AcquireJobState::Failed {
        reason: failure("SteamJobFailed", message),
    };
    set(AcquireJobState::Resolving);
    let mut host = match runner.start(&plan) {
        Ok(host) => host,
        Err(_) => return set(failed("Steam could not start on this device. Try again.")),
    };
    if host.send(command).is_err() {
        return set(failed("Steam could not start on this device. Try again."));
    }
    let mut state = Job::Starting;
    let deadline = Instant::now() + limit;
    loop {
        for event in host.poll() {
            if let Event::SigninFailed { .. } | Event::SignedOut {} = event {
                return set(failed("Sign in to Steam in Settings first."));
            }
            state = state.apply(app, &event);
            match &state {
                Job::Moving { done, total } => set(AcquireJobState::Downloading {
                    bytes_done: *done,
                    bytes_total: (*total > 0).then_some(*total),
                    bytes_per_second: None,
                }),
                Job::Finished { ok: true, .. } => {
                    host.stop();
                    set(AcquireJobState::Placing);
                    let game_id = imported(app).unwrap_or_default();
                    return set(AcquireJobState::Imported { game_id });
                }
                Job::Finished { ok: false, result } => {
                    return set(failed(Job::failure(result)));
                }
                Job::Starting => {}
            }
        }
        if job.stop.load(Ordering::SeqCst) {
            // The host stops with SIGKILL; Steam keeps what it downloaded.
            return;
        }
        if Instant::now() >= deadline || !host.running() {
            return set(failed(Job::failure("timeout")));
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam_engine::host::{process_units::ProcessHostUnits, tests::script_host};

    const HOST: &str = r#"echo '{"event":"signed_in","account_name":"p"}'
exec 3<>"$fifo"
while read -r line <&3; do
  case "$line" in
    list-apps) echo '{"event":"app","appid":78000,"name":"Bejeweled 3","source":"owned"}'
               echo '{"event":"app","appid":440,"name":"Team Fortress 2","source":"free"}'
               echo '{"event":"apps_done"}' ;;
    "install 78000") echo '{"event":"app_job_started","appid":78000,"kind":"install"}'
               echo '{"event":"app_progress","appid":78000,"bytes_downloaded":40,"bytes_to_download":80,"bytes_staged":0,"bytes_to_stage":100}'
               sleep 0.6
               echo '{"event":"app_job_finished","appid":78000,"kind":"install","result":"installed"}' ;;
    "install 440") echo '{"event":"app_progress","appid":440,"bytes_downloaded":1,"bytes_to_download":9,"bytes_staged":0,"bytes_to_stage":9}'; sleep 30 ;;
    "uninstall 78000") echo '{"event":"app_job_finished","appid":78000,"kind":"uninstall","result":"uninstalled"}' ;;
  esac
done"#;

    fn setup(temp: &std::path::Path) -> (HostPlan, JobRunner) {
        let launches = temp.join("launches");
        (
            HostPlan {
                host_program: script_host(temp, HOST),
                engine: temp.join("steamclient.so"),
                proton: None,
                account_root: temp.into(),
            },
            JobRunner {
                units: Arc::new(ProcessHostUnits::new(&launches)),
                launches_root: launches,
                runtime_uid: unsafe { libc::geteuid() },
                live: Arc::default(),
            },
        )
    }

    async fn until(
        store: &SteamStore,
        job: &str,
        check: impl Fn(&AcquireJobState) -> bool,
    ) -> AcquireJobState {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let state = store.status(job).unwrap().state;
            if check(&state) {
                return state;
            }
            assert!(Instant::now() < deadline, "{state:?}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn search_lists_the_persons_games_and_get_installs_with_progress() {
        let temp = tempfile::tempdir().unwrap();
        let (plan, runner) = setup(temp.path());
        let store = Arc::new(SteamStore::default());
        let found = store
            .search(plan.clone(), runner.clone(), "bejeweled")
            .await
            .unwrap();
        assert_eq!(found.claims.len(), 1);
        assert_eq!(found.claims[0].id, "78000");
        assert_eq!(found.claims[0].provider_id, "@korri:steam");
        let details = store
            .details(plan.clone(), runner.clone(), "440")
            .await
            .unwrap();
        assert_eq!(details.title, "Team Fortress 2");
        let job = store
            .acquire(
                plan.clone(),
                runner.clone(),
                "78000",
                Arc::new(|app| Some(format!("game-{app}"))),
            )
            .unwrap();
        assert!(matches!(job.state, AcquireJobState::Queued));
        let moving = until(&store, &job.job_id, |s| {
            matches!(s, AcquireJobState::Downloading { .. })
        })
        .await;
        assert!(matches!(
            moving,
            AcquireJobState::Downloading {
                bytes_done: 40,
                bytes_total: Some(180),
                ..
            }
        ));
        let done = until(&store, &job.job_id, |s| {
            matches!(s, AcquireJobState::Imported { .. })
        })
        .await;
        assert!(matches!(done, AcquireJobState::Imported { game_id } if game_id == "game-78000"));
        store.uninstall(plan, runner, 78000).await.unwrap();
        assert_eq!(
            std::fs::read_dir(temp.path().join("launches"))
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancel_stops_the_install_and_its_host() {
        let temp = tempfile::tempdir().unwrap();
        let (plan, runner) = setup(temp.path());
        let store = Arc::new(SteamStore::default());
        let job = store
            .acquire(plan, runner, "440", Arc::new(|_| None))
            .unwrap();
        until(&store, &job.job_id, |s| {
            matches!(s, AcquireJobState::Downloading { .. })
        })
        .await;
        assert!(matches!(
            store.cancel(&job.job_id).unwrap().state,
            AcquireJobState::Cancelled
        ));
        let deadline = Instant::now() + Duration::from_secs(5);
        while std::fs::read_dir(temp.path().join("launches"))
            .unwrap()
            .count()
            > 0
        {
            assert!(Instant::now() < deadline, "the job host stops on cancel");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(store
            .acquire(
                setup(temp.path()).0,
                setup(temp.path()).1,
                "../1",
                Arc::new(|_| None)
            )
            .is_err());
    }
}

#[cfg(test)]
mod admission_tests {
    const PLUGIN: &str = include_str!("../../tests/fixtures/first-party/steam/plugin.ts");

    /// The Steam provider has no claims handlers: korrid serves it. Any other
    /// provider, or a second one beside it, still needs them.
    #[test]
    fn only_the_steam_provider_is_admitted_without_claims_handlers() {
        let declared = format!(
            "{PLUGIN}\nexport const providers = {{ \"@korri:steam\": {{ title: \"Steam\" }} }}\n"
        );
        // Admitted with no claims handlers; the base fixture has none.
        assert!(!PLUGIN.contains("claims.search"));
        crate::plugin::load_plugin_source("@korri", &declared).unwrap();
        let second = format!(
            "{PLUGIN}\nexport const providers = {{ \"@korri:steam\": {{ title: \"Steam\" }}, \"@korri:other\": {{ title: \"Other\" }} }}\n"
        );
        assert!(crate::plugin::load_plugin_source("@korri", &second).is_err());
        let renamed = declared.replace(
            "export const name = \"steam\"",
            "export const name = \"other\"",
        );
        assert!(crate::plugin::load_plugin_source("@korri", &renamed).is_err());
    }
}
