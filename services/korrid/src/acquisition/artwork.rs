//! The artwork operation (ADR 0032, with measured media from ADR 0091).
//!
//! A plugin that carries a `game-assets.candidates` handler finds images for
//! a game. It performs no I/O: it returns the HTTP requests it needs or its
//! candidates, as an acquisition provider does (ADR 0016). korrid makes every
//! request, adds the plugin's secret itself (ADR 0034), downloads the first
//! four candidates of each kind, checks the bytes and stores them beside
//! every other art plugin's. korrid does not know any artwork site.
use std::{
    collections::BTreeMap,
    fs::{self, DirBuilder},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use korri_surface_treaty::SurfaceMediaKind;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use typeshare::typeshare;

use super::{
    download::DownloadEngine,
    turns::{self, PluginHttpRequests, PluginHttpResponse, ProviderContext, ProviderError},
    Failure,
};
use crate::{
    config::library::ExternalIds,
    discovery::reconcile::{self, ArtworkGame},
    game_assets::{
        AssetCandidate, AssetError, AssetOwnerIdentity, GameAssetRepository, Replaced,
        MAX_ASSET_BYTES, MAX_IMAGES_PER_KIND,
    },
    plugin::{deserialize_optional_non_null as non_null, PluginRegistry},
    plugin_policy::RegistrySource,
    plugin_secrets::{PluginCredential, PluginSecrets},
};

pub use crate::script::GAME_ASSETS_CANDIDATES;

/// main's `ATTEMPTS_FILE`, now in the plugin's own folder.
const ATTEMPTS_FILE: &str = "attempts.json";
/// Games one plugin is asked about in one pass.
const BATCH_LIMIT: usize = 8;
/// A plugin's answer about a game is asked again after 30 days (the
/// coordinator's v1 choice for ADR 0091).
const REASK_AFTER_SECONDS: u64 = 30 * 24 * 60 * 60;

/// `game-assets.candidates` input (ADR 0090): the game's library title, its
/// system, the game's stored public ids, and every response so far.
/// `system` is absent for a release that names no system.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginGameAssetCandidatesInput {
    pub title: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub system: Option<PluginGameAssetSystem>,
    /// The game's public ids from earlier matches, by authority.
    #[typeshare(serialized_as = "HashMap<String, Vec<String>>")]
    pub external_ids: BTreeMap<String, Vec<String>>,
    pub responses: Vec<PluginHttpResponse>,
}

/// The game's system as art plugins see it: its id, its title when a
/// plugin gives one, and the union of its declared public ids.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginGameAssetSystem {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub title: Option<String>,
    #[typeshare(serialized_as = "HashMap<String, Vec<String>>")]
    pub external_ids: BTreeMap<String, Vec<String>>,
}

/// Legacy `GameAssetSource` without `provider`: korrid writes the owning
/// plugin's id itself. `id` is the site's own id of the image, as text.
/// `url` is where korrid downloads it: HTTPS, one of the plugin's `hosts`,
/// no credentials, query or fragment.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GameAssetCandidateSource {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub id: Option<String>,
    pub url: String,
}

/// Legacy `GameAssetCandidate` without `candidateId` and `gameId`, with
/// ADR 0091's `kind` in place of legacy `role`. `width` and `height` are
/// optional; when given, the image must have that size.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GameAssetCandidate {
    pub kind: SurfaceMediaKind,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub width: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    pub height: Option<u32>,
    pub source: GameAssetCandidateSource,
}

/// The handler's result (legacy `ListGameAssetCandidatesResponse`): the
/// candidates in the plugin's order of preference, kinds in any order.
/// korrid takes the first four candidates of each kind. Empty: no match.
/// `externalIds`: the public ids of the matched game, which korrid merges
/// into the game's record (ADR 0090).
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameAssetCandidates {
    pub candidates: Vec<GameAssetCandidate>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null"
    )]
    #[typeshare(serialized_as = "Option<HashMap<String, Vec<String>>>")]
    pub external_ids: Option<BTreeMap<String, Vec<String>>>,
}

#[typeshare(serialized_as = "PluginGameAssetCandidatesOutputWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PluginGameAssetCandidatesOutput {
    Requests(PluginHttpRequests),
    Result(GameAssetCandidates),
}

impl GameAssetCandidate {
    /// The rules a candidate must meet before korrid looks at any of them.
    fn admit(&self) -> Result<(), &'static str> {
        super::claims::safe_url(&self.source.url).map_err(|error| error.0)?;
        let url = url::Url::parse(&self.source.url).map_err(|_| "invalid URL")?;
        if url.query().is_some() || url.fragment().is_some() {
            return Err("an asset URL has no query and no fragment");
        }
        if self
            .source
            .id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.chars().count() > 2048)
        {
            return Err("an asset id is 1 to 2048 characters");
        }
        if self.width == Some(0) || self.height == Some(0) {
            return Err("an asset has a positive width and height");
        }
        Ok(())
    }
}

/// What the pass did, for the discovery snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ArtworkReport {
    /// Games a plugin was asked about.
    pub attempted: usize,
    /// Images stored.
    pub stored: usize,
    pub diagnostics: Vec<ArtworkDiagnostic>,
}

/// One line for the discovery snapshot. The message is fixed copy or a name
/// korrid chose: never a URL, a header value, plugin text or the secret.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArtworkDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub playable_id: Option<String>,
}

fn diagnostic(code: &'static str, message: &str, playable_id: Option<&str>) -> ArtworkDiagnostic {
    ArtworkDiagnostic {
        code,
        message: message.into(),
        playable_id: playable_id.map(str::to_owned),
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct AttemptState {
    #[serde(default)]
    attempts: BTreeMap<String, AttemptRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct AttemptRecord {
    outcome: AttemptOutcome,
    playable_id: String,
    release_id: String,
    release_fingerprint: String,
    rom_identity: String,
    /// When korrid recorded this attempt, in whole seconds since the Unix
    /// epoch.
    answered_at: u64,
    /// `external_ids_sha256` of the game's public ids once this attempt's
    /// own ids were merged in. The fingerprint leaves the ids out, so this
    /// is how a change of ids reaches the plugin again.
    external_ids_sha256: String,
}

/// What became of one plugin's lookup for one game (ADR 0032, section 2).
/// Whatever the outcome, `needs_asking` asks again once the record is 30
/// days old or the game's public ids changed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum AttemptOutcome {
    /// An image was stored, or the game holds images from the plugin and
    /// nothing failed. Not cleared by a rescan or a secret change.
    Assigned,
    /// The plugin found nothing, or a response repeated the plugin's secret
    /// and korrid kept it from the plugin. Cleared by a secret change.
    NoMatch,
    /// The site or the download failed. Cleared by a rescan.
    Transient,
    /// korrid refused the bytes. Cleared by a secret change.
    AssetRejected,
}

/// main's `attempt_key`: one game as the local catalog sees it now. A changed
/// ROM or library record is another key, so it is looked up again: the
/// fingerprint covers the whole record, title and releases' systems
/// included, and leaves out only the public ids.
fn attempt_key(game: &ArtworkGame) -> String {
    let mut hasher = Sha256::new();
    hasher.update(game.playable_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(game.release_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(game.release_fingerprint.as_bytes());
    hasher.update(b"\0");
    hasher.update(game.rom_identity.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn attempt_record(
    game: &ArtworkGame,
    outcome: AttemptOutcome,
    external_ids: &ExternalIds,
    answered_at: u64,
) -> AttemptRecord {
    AttemptRecord {
        outcome,
        playable_id: game.playable_id.clone(),
        release_id: game.release_id.clone(),
        release_fingerprint: game.release_fingerprint.clone(),
        rom_identity: game.rom_identity.clone(),
        answered_at,
        external_ids_sha256: external_ids_sha256(external_ids),
    }
}

/// A hash of a game's public ids, by authority and in their stored order.
fn external_ids_sha256(ids: &ExternalIds) -> String {
    let bytes = serde_json::to_vec(ids).expect("public ids serialize");
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

/// The staleness rule (ADR 0091): korrid asks a plugin about a game when
/// the plugin has no attempt on record for the game as it is now (a new
/// title, system, release or ROM is a new `attempt_key`), when the game's
/// public ids changed since that attempt, or when the attempt is 30 days
/// old. An attempt from a time after `now` counts as old: the clock moved
/// back, and its age is unknown.
fn needs_asking(record: Option<&AttemptRecord>, game: &ArtworkGame, now: u64) -> bool {
    let Some(record) = record else {
        return true;
    };
    record.external_ids_sha256 != external_ids_sha256(&game.external_ids)
        || record.answered_at > now
        || now - record.answered_at >= REASK_AFTER_SECONDS
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn owner_identity(game: &ArtworkGame) -> AssetOwnerIdentity {
    AssetOwnerIdentity {
        playable_id: game.playable_id.clone(),
        release_id: game.release_id.clone(),
        release_fingerprint: game.release_fingerprint.clone(),
        rom_identity: game.rom_identity.clone(),
    }
}

/// `<private>/<plugin id>/attempts.json`. The secret store checks the id
/// before it becomes a path part.
fn attempts_path(private_root: &Path, plugin_id: &str) -> std::io::Result<PathBuf> {
    PluginSecrets::new(private_root)
        .plugin_folder(plugin_id)
        .map(|folder| folder.join(ATTEMPTS_FILE))
        .map_err(std::io::Error::other)
}

fn read_attempts(private_root: &Path, plugin_id: &str) -> std::io::Result<AttemptState> {
    match fs::read(attempts_path(private_root, plugin_id)?) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(AttemptState::default()),
        Err(error) => Err(error),
    }
}

fn write_attempts(
    private_root: &Path,
    plugin_id: &str,
    state: &AttemptState,
) -> std::io::Result<()> {
    let path = attempts_path(private_root, plugin_id)?;
    let parent = path.parent().expect("attempt state has a parent");
    // The plugin's folder is private, as the secret store makes it.
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let temporary = parent.join(format!(".attempts.{}.tmp", rand::random::<u64>()));
    fs::write(&temporary, serde_json::to_vec_pretty(state)?)?;
    fs::rename(temporary, path)
}

/// Keep only the attempts that match `keep`. A plugin with no attempts file
/// gets none.
fn retain_attempts(
    private_root: &Path,
    plugin_id: &str,
    keep: impl Fn(AttemptOutcome) -> bool,
) -> std::io::Result<()> {
    if !attempts_path(private_root, plugin_id)?.exists() {
        return Ok(());
    }
    let mut state = read_attempts(private_root, plugin_id)?;
    let before = state.attempts.len();
    state.attempts.retain(|_, record| keep(record.outcome));
    if state.attempts.len() == before {
        return Ok(());
    }
    write_attempts(private_root, plugin_id, &state)
}

/// A set or a clear of a plugin's secret: every attempt except `assigned`
/// goes, so the next pass asks again with the new secret. The caller holds
/// the discovery write lock when a pass can run.
pub(crate) fn clear_attempts_after_secret_change(
    private_root: &Path,
    plugin_id: &str,
) -> std::io::Result<()> {
    retain_attempts(private_root, plugin_id, |outcome| {
        outcome == AttemptOutcome::Assigned
    })
}

/// One enabled plugin that carries the handler.
struct ArtPlugin {
    id: String,
    context: ProviderContext,
    credential_host: Option<String>,
    /// Where korrid exchanges a client pair for a token (ADR 0034 amendment).
    credential_token_url: Option<String>,
}

/// Every enabled, installed plugin with a `game-assets.candidates` handler,
/// in plugin id order. The handler is looked for, never called.
fn art_plugins(registry: &PluginRegistry) -> Vec<ArtPlugin> {
    registry
        .enabled_plugin_ids()
        .into_iter()
        .filter_map(|id| {
            let source = registry.plugin_source(id).ok()?;
            crate::script::has_plugin_operation_snapshot(&source, GAME_ASSETS_CANDIDATES)
                .ok()?
                .then(|| ArtPlugin {
                    id: id.to_owned(),
                    context: ProviderContext {
                        // The operation asks about no provider.
                        provider_id: id.to_owned(),
                        source: Arc::new(source),
                        hosts: Arc::new(registry.plugin_hosts(id).to_vec()),
                        credential: None,
                    },
                    credential_host: registry.plugin_credential_host(id).map(str::to_owned),
                    credential_token_url: registry
                        .plugin_credential_token_url(id)
                        .map(str::to_owned),
                })
        })
        .collect()
}

enum GameOutcome {
    /// Record the outcome, with the game's public ids once the plugin's own
    /// were merged in, and go on.
    Recorded(AttemptOutcome, ExternalIds),
    /// The game changed during the pass: nothing is recorded.
    Skipped,
    /// The site refused the secret: the pass stops for this plugin.
    Unauthorized,
}

/// The time an attempt is recorded at.
type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

/// The artwork pass korrid runs after each successful discovery pass.
#[derive(Clone)]
pub(crate) struct ArtworkPass {
    readable_root: PathBuf,
    private_root: PathBuf,
    write_lock: Arc<Mutex<()>>,
    registry_source: RegistrySource,
    engine: DownloadEngine,
    clock: Clock,
}

impl ArtworkPass {
    pub(crate) fn new(
        readable_root: impl AsRef<Path>,
        private_root: impl AsRef<Path>,
        write_lock: Arc<Mutex<()>>,
        registry_source: RegistrySource,
    ) -> Self {
        Self {
            readable_root: readable_root.as_ref().to_owned(),
            private_root: private_root.as_ref().to_owned(),
            write_lock,
            registry_source,
            engine: DownloadEngine::default(),
            clock: Arc::new(SystemTime::now),
        }
    }

    #[cfg(test)]
    pub(super) fn with_engine(mut self, engine: DownloadEngine) -> Self {
        self.engine = engine;
        self
    }

    #[cfg(test)]
    pub(super) fn with_clock(
        mut self,
        clock: impl Fn() -> SystemTime + Send + Sync + 'static,
    ) -> Self {
        self.clock = Arc::new(clock);
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.write_lock
            .lock()
            .expect("discovery write lock poisoned")
    }

    /// A rescan lets every plugin try its `transient` games again.
    pub(crate) fn clear_retryable_attempts(&self) {
        let Ok(registry) = self.registry_source.registry() else {
            return;
        };
        let _guard = self.lock();
        for plugin in art_plugins(&registry) {
            let _ = retain_attempts(&self.private_root, &plugin.id, |outcome| {
                outcome != AttemptOutcome::Transient
            });
        }
    }

    /// `clear_attempts_after_secret_change` under the discovery write lock,
    /// so a pass that is recording an attempt cannot write the old ones back.
    pub(crate) fn secret_changed(&self, plugin_id: &str) -> std::io::Result<()> {
        let _guard = self.lock();
        clear_attempts_after_secret_change(&self.private_root, plugin_id)
    }

    /// Run the pass on the calling thread, which must not be a runtime
    /// thread: discovery's own worker thread calls it.
    pub(crate) fn run(&self) -> ArtworkReport {
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(self.pass()),
            Err(_) => ArtworkReport {
                diagnostics: vec![diagnostic(
                    "EnrichmentStateUnavailable",
                    "the artwork pass could not start",
                    None,
                )],
                ..ArtworkReport::default()
            },
        }
    }

    async fn pass(&self) -> ArtworkReport {
        let mut report = ArtworkReport::default();
        // A device with no readable plugin set has no artwork plugin.
        let Ok(registry) = self.registry_source.registry() else {
            return report;
        };
        let plugins = art_plugins(&registry);
        if plugins.is_empty() {
            return report;
        }
        let games =
            match reconcile::artwork_games(&self.readable_root, &self.private_root, &registry) {
                Ok(games) => games,
                Err(error) => {
                    report.diagnostics.push(diagnostic(
                        "EnrichmentCatalogUnavailable",
                        &error.to_string(),
                        None,
                    ));
                    return report;
                }
            };
        for plugin in plugins {
            self.plugin_pass(plugin, &registry, &games, &mut report)
                .await;
        }
        report
    }

    /// The plugin's secret, when it declares one. `Err` carries the
    /// diagnostic that ends this plugin's pass: no secret, no call.
    fn credential(
        &self,
        plugin: &ArtPlugin,
    ) -> Result<Option<Arc<PluginCredential>>, ArtworkDiagnostic> {
        let Some(host) = &plugin.credential_host else {
            return Ok(None);
        };
        match PluginSecrets::new(&self.private_root).credential_declared(
            &plugin.id,
            host,
            plugin.credential_token_url.as_deref(),
        ) {
            Ok(Some(credential)) => Ok(Some(Arc::new(credential))),
            Ok(None) => Err(diagnostic(
                "CredentialMissing",
                &format!("{} needs its key. Enter it in Settings.", plugin.id),
                None,
            )),
            Err(_) => Err(diagnostic(
                "CredentialUnavailable",
                &format!("the key of {} cannot be read", plugin.id),
                None,
            )),
        }
    }

    async fn plugin_pass(
        &self,
        mut plugin: ArtPlugin,
        registry: &PluginRegistry,
        games: &[ArtworkGame],
        report: &mut ArtworkReport,
    ) {
        let credential = match self.credential(&plugin) {
            Ok(credential) => credential,
            Err(missing) => {
                report.diagnostics.push(missing);
                return;
            }
        };
        plugin.context.credential = credential.clone();
        let state_unavailable = || {
            diagnostic(
                "EnrichmentStateUnavailable",
                "enrichment state is unavailable",
                None,
            )
        };
        let attempts = {
            let _guard = self.lock();
            match read_attempts(&self.private_root, &plugin.id) {
                Ok(attempts) => attempts,
                Err(_) => {
                    report.diagnostics.push(state_unavailable());
                    return;
                }
            }
        };
        let repository = GameAssetRepository::new(&self.private_root);
        let now = unix_seconds((self.clock)());
        let mut asked = 0;
        for game in games {
            let key = attempt_key(game);
            if !needs_asking(attempts.attempts.get(&key), game, now) {
                continue;
            }
            let held = match repository.holds_from(&owner_identity(game), &plugin.id, None) {
                Ok(held) => held,
                Err(error) => {
                    // Unreadable records are not this game's fault: stop
                    // before any request and record nothing.
                    report
                        .diagnostics
                        .push(asset_diagnostic(&error, &game.playable_id));
                    return;
                }
            };
            if asked == BATCH_LIMIT {
                break;
            }
            asked += 1;
            report.attempted += 1;
            let (outcome, external_ids) =
                match self.one_game(&plugin, registry, game, held, report).await {
                    GameOutcome::Recorded(outcome, external_ids) => (outcome, external_ids),
                    GameOutcome::Skipped => continue,
                    GameOutcome::Unauthorized => {
                        report.diagnostics.push(diagnostic(
                            "CredentialUnauthorized",
                            &format!("the site of {} refused its key", plugin.id),
                            None,
                        ));
                        return;
                    }
                };
            let answered_at = unix_seconds((self.clock)());
            match self.record(
                &plugin,
                credential.as_deref(),
                key,
                attempt_record(game, outcome, &external_ids, answered_at),
            ) {
                Ok(true) => {}
                // The secret changed during the pass: what this pass learned
                // is about the old secret.
                Ok(false) => return,
                Err(()) => {
                    report.diagnostics.push(state_unavailable());
                    return;
                }
            }
        }
    }

    /// Record one attempt, unless the plugin's secret changed since the pass
    /// read it.
    fn record(
        &self,
        plugin: &ArtPlugin,
        credential: Option<&PluginCredential>,
        key: String,
        record: AttemptRecord,
    ) -> Result<bool, ()> {
        let _guard = self.lock();
        if let (Some(expected), Some(host)) = (credential, &plugin.credential_host) {
            match PluginSecrets::new(&self.private_root).credential_declared(
                &plugin.id,
                host,
                plugin.credential_token_url.as_deref(),
            ) {
                Ok(Some(current)) if current.same_secret(expected) => {}
                Ok(_) => return Ok(false),
                Err(_) => return Err(()),
            }
        }
        let mut attempts = read_attempts(&self.private_root, &plugin.id).map_err(|_| ())?;
        attempts.attempts.insert(key, record);
        write_attempts(&self.private_root, &plugin.id, &attempts).map_err(|_| ())?;
        Ok(true)
    }

    async fn one_game(
        &self,
        plugin: &ArtPlugin,
        registry: &PluginRegistry,
        game: &ArtworkGame,
        held: bool,
        report: &mut ArtworkReport,
    ) -> GameOutcome {
        let playable_id = game.playable_id.as_str();
        let input = asset_input(registry, game);
        let unanswered = |outcome| GameOutcome::Recorded(outcome, game.external_ids.clone());
        let mut external_ids = game.external_ids.clone();
        let candidates = match self.candidates(&plugin.context, &input).await {
            Ok(result) => {
                if let Some(ids) = result.external_ids.filter(|ids| !ids.is_empty()) {
                    // Under the write lock, as an image is stored.
                    let merged = {
                        let _guard = self.lock();
                        reconcile::merge_game_external_ids(
                            &self.readable_root,
                            &self.private_root,
                            registry,
                            game,
                            &ids,
                        )
                    };
                    match merged {
                        Ok(true) => {
                            crate::config::library::merge_external_ids(&mut external_ids, &ids);
                        }
                        Ok(false) => return GameOutcome::Skipped,
                        Err(error) => {
                            report.diagnostics.push(diagnostic(
                                "EnrichmentCatalogWriteFailed",
                                &error.to_string(),
                                Some(playable_id),
                            ));
                            return unanswered(AttemptOutcome::Transient);
                        }
                    }
                }
                result.candidates
            }
            Err(ProviderError::Unauthorized) => return GameOutcome::Unauthorized,
            Err(ProviderError::CredentialRepeated { host }) => {
                // The site, not the network, is at fault, and it will be at
                // the next rescan too. The game is on record until the
                // secret changes, so korrid does not send the secret to
                // that address again for this game at every rescan.
                report.diagnostics.push(diagnostic(
                    "ProviderFailed",
                    &format!(
                        "{host} repeated the key of {} in a response. Korri did not give that response to the plugin.",
                        plugin.id
                    ),
                    Some(playable_id),
                ));
                return unanswered(AttemptOutcome::NoMatch);
            }
            Err(error) => {
                // The fixed copy of the provider failure: no site text.
                let failure = error.rpc();
                report.diagnostics.push(ArtworkDiagnostic {
                    code: match error {
                        ProviderError::Unreachable(_) => "ProviderUnreachable",
                        _ => "ProviderFailed",
                    },
                    message: failure.message,
                    playable_id: Some(playable_id.to_owned()),
                });
                return unanswered(AttemptOutcome::Transient);
            }
        };
        if candidates.is_empty() {
            // No match now. The images an earlier answer stored stay: an
            // empty answer can also be a site that failed under the plugin.
            return GameOutcome::Recorded(AttemptOutcome::NoMatch, external_ids);
        }
        let (mut stored, mut transient, mut rejected) = (0, false, false);
        for kind in SurfaceMediaKind::ALL {
            // The plugin's order is its preference: the first four of a kind.
            let wanted: Vec<&GameAssetCandidate> = candidates
                .iter()
                .filter(|candidate| candidate.kind == kind)
                .take(MAX_IMAGES_PER_KIND)
                .collect();
            if wanted.is_empty() {
                continue;
            }
            let mut downloaded = Vec::new();
            let mut complete = true;
            for candidate in wanted {
                match self.download(plugin, candidate).await {
                    Ok(bytes) => downloaded.push(AssetCandidate {
                        bytes,
                        declared_width: candidate.width,
                        declared_height: candidate.height,
                        source_id: candidate.source.id.clone(),
                        source_url: Some(candidate.source.url.clone()),
                    }),
                    Err(failure) => {
                        report
                            .diagnostics
                            .push(download_diagnostic(failure, playable_id));
                        transient = true;
                        complete = false;
                    }
                }
            }
            match self.store(plugin, registry, game, kind, downloaded, complete) {
                Ok(Some(replaced)) => {
                    stored += replaced.stored.len();
                    for error in &replaced.rejected {
                        report
                            .diagnostics
                            .push(asset_diagnostic(error, playable_id));
                        rejected = true;
                    }
                }
                Ok(None) => return GameOutcome::Skipped,
                Err(Stored::Catalog(message)) => {
                    report.diagnostics.push(diagnostic(
                        "EnrichmentCatalogWriteFailed",
                        &message,
                        Some(playable_id),
                    ));
                    transient = true;
                }
                Err(Stored::Asset(error)) => {
                    report
                        .diagnostics
                        .push(asset_diagnostic(&error, playable_id));
                    transient = true;
                }
            }
        }
        report.stored += stored;
        let outcome = if transient {
            // A rescan asks again.
            AttemptOutcome::Transient
        } else if stored > 0 || held {
            AttemptOutcome::Assigned
        } else if rejected {
            AttemptOutcome::AssetRejected
        } else {
            AttemptOutcome::NoMatch
        };
        GameOutcome::Recorded(outcome, external_ids)
    }

    /// Run the handler's turns to its candidates and matched ids.
    async fn candidates(
        &self,
        context: &ProviderContext,
        input: &PluginGameAssetCandidatesInput,
    ) -> Result<GameAssetCandidates, ProviderError> {
        let result: GameAssetCandidates = turns::run(
            &self.engine,
            context,
            GAME_ASSETS_CANDIDATES,
            |responses| {
                serde_json::to_string(&PluginGameAssetCandidatesInput {
                    responses,
                    ..input.clone()
                })
            },
            turns::decode_turn,
        )
        .await?;
        for candidate in &result.candidates {
            candidate
                .admit()
                .map_err(|reason| ProviderError::Failed(reason.into()))?;
        }
        if let Some(ids) = &result.external_ids {
            crate::config::library::validate_external_ids(ids)
                .map_err(|reason| ProviderError::Failed(reason.into()))?;
        }
        Ok(result)
    }

    /// The image a candidate names. This download never carries the secret.
    async fn download(
        &self,
        plugin: &ArtPlugin,
        candidate: &GameAssetCandidate,
    ) -> Result<Vec<u8>, Failure> {
        let url = url::Url::parse(&candidate.source.url).map_err(|_| Failure::UnsafeUrl)?;
        self.engine
            .fetch_image(&url, &plugin.context.hosts, MAX_ASSET_BYTES)
            .await
    }

    /// Under the write lock: the game must still be the one the plugin was
    /// asked about. `Ok(None)`: it changed, and nothing was written.
    ///
    /// The images replace the plugin's earlier images of `kind` for this
    /// game. When a download of the kind failed (`complete` is false), the
    /// earlier images stay if the game holds any, and the images that did
    /// arrive are stored only if it holds none.
    fn store(
        &self,
        plugin: &ArtPlugin,
        registry: &PluginRegistry,
        game: &ArtworkGame,
        kind: SurfaceMediaKind,
        candidates: Vec<AssetCandidate>,
        complete: bool,
    ) -> Result<Option<Replaced>, Stored> {
        let _guard = self.lock();
        let Some(current) = reconcile::current_artwork_game(
            &self.readable_root,
            &self.private_root,
            registry,
            game,
        )
        .map_err(|error| Stored::Catalog(error.to_string()))?
        else {
            return Ok(None);
        };
        let repository = GameAssetRepository::new(&self.private_root);
        let owner = owner_identity(&current);
        if candidates.is_empty()
            || (!complete
                && repository
                    .holds_from(&owner, &plugin.id, Some(kind))
                    .map_err(Stored::Asset)?)
        {
            return Ok(Some(Replaced::default()));
        }
        repository
            .replace(owner, &plugin.id, kind, candidates)
            .map(Some)
            .map_err(Stored::Asset)
    }
}

/// The handler's first input: the game's title, system and stored ids
/// (ADR 0090). korrid passes the system's ids and reads none of them.
fn asset_input(registry: &PluginRegistry, game: &ArtworkGame) -> PluginGameAssetCandidatesInput {
    PluginGameAssetCandidatesInput {
        title: game.title.clone(),
        system: game.system.as_ref().map(|id| {
            let rules = registry.content_rules(id);
            PluginGameAssetSystem {
                id: id.clone(),
                title: rules.and_then(|rules| rules.title.clone()),
                external_ids: rules
                    .map(|rules| rules.external_ids.clone())
                    .unwrap_or_default(),
            }
        }),
        external_ids: game.external_ids.clone(),
        responses: Vec::new(),
    }
}

/// Why an image was not stored.
enum Stored {
    Catalog(String),
    Asset(AssetError),
}

fn asset_diagnostic(error: &AssetError, playable_id: &str) -> ArtworkDiagnostic {
    let code = match error {
        AssetError::Oversized => "AssetTooLarge",
        AssetError::Unsupported => "AssetUnsupported",
        AssetError::Malformed => "AssetMalformed",
        AssetError::UnsafeDimensions => "AssetUnsafeDimensions",
        AssetError::InconsistentDimensions => "AssetInconsistentDimensions",
        AssetError::Storage => "AssetStorageUnavailable",
    };
    diagnostic(code, &error.to_string(), Some(playable_id))
}

fn download_diagnostic(failure: Failure, playable_id: &str) -> ArtworkDiagnostic {
    let (code, message) = match failure {
        Failure::UnsafeUrl | Failure::UndeclaredHost => (
            "AssetUrlRejected",
            "the asset address is not HTTPS on a host the plugin declared",
        ),
        Failure::Redirect => ("AssetRedirectRejected", "asset redirects are not followed"),
        Failure::TooLarge => ("AssetTooLarge", "asset bytes exceed the configured limit"),
        _ => ("AssetDownloadFailed", "asset download failed"),
    };
    diagnostic(code, message, Some(playable_id))
}

#[cfg(test)]
#[path = "artwork_tests.rs"]
mod tests;
