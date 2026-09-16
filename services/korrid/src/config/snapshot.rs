use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use proseql_engine::{
    descriptor::{IdStrategy, SchemaNode},
    errors::EngineError,
};
use proseql_formats::FormatRegistry;
use proseql_storage::{
    document_graph::{
        load_document_graph_sources, DocumentGraphTransformContext, DocumentGraphTransformHost,
        LoadedDocumentGraph,
    },
    fs::FsStorageHost,
    host::StorageHost,
    memory::MemoryStorageHost,
    persistence::CollectionStorageConfig,
    source_config::{
        normalize_source_config, DatabaseSourceConfig, DocumentGraphFragmentErrorPolicy,
        DocumentGraphRootConfig, DocumentGraphSourceConfig, SourceCollectionSelection,
        SourceConfigInput,
    },
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{
    classify_snapshot_support, decode_config_documents, ConfigSchemaError, ConfigSnapshot,
    HostPayload,
};

pub const DEVICE_FILE_NAME: &str = "device.yaml";
pub const GAMES_FILE_NAME: &str = "catalog/games.yaml";
pub const RELEASES_FILE_NAME: &str = "catalog/releases.yaml";
pub const FILE_NAMES: [&str; 3] = [DEVICE_FILE_NAME, GAMES_FILE_NAME, RELEASES_FILE_NAME];
pub const EMPTY_DOCUMENT_BYTES: &[u8] = b"{}\n";

const TRANSFORM_CALLBACK_ID: &str = "korri.strict-readable";
use super::{DEVICE_SECTIONS, GAMES_SECTIONS, RELEASES_SECTIONS};
const ALL_SECTIONS: &[&str] = &[
    "host",
    "storage",
    "providers",
    "systems",
    "families",
    "runners",
    "profiles",
    "hooks",
    "locations",
    "games",
    "releases",
];

#[derive(Clone, Debug)]
pub struct ConfigSnapshotState {
    pub snapshot: Arc<ConfigSnapshot>,
    pub generation: u64,
    pub diagnostic: Option<SnapshotDiagnostic>,
    pub authorization: SnapshotAuthorization,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SnapshotAuthorization {
    Authorized,
    Unauthorized,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotDiagnostic {
    pub code: SnapshotDiagnosticCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SnapshotDiagnosticCode {
    LocalConfigReloadFailed,
    LocalConfigUnsupported,
    LocalConfigUnauthorized,
}

#[derive(Clone)]
pub struct ConfigSnapshotCoordinator {
    root: Arc<PathBuf>,
    storage: Arc<dyn StorageHost>,
    state: Arc<RwLock<ConfigSnapshotState>>,
    reload_lock: Arc<Mutex<()>>,
}

impl ConfigSnapshotCoordinator {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let storage = FsStorageHost::new_polling(Duration::from_secs(60))
            .expect("create proseQL filesystem storage host");
        Self::with_storage(root, Arc::new(storage))
    }

    pub fn with_storage(root: impl AsRef<Path>, storage: Arc<dyn StorageHost>) -> Self {
        Self {
            root: Arc::new(root.as_ref().to_owned()),
            storage,
            state: Arc::new(RwLock::new(ConfigSnapshotState {
                snapshot: Arc::new(ConfigSnapshot::default()),
                generation: 0,
                diagnostic: None,
                authorization: SnapshotAuthorization::Authorized,
            })),
            reload_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn current(&self) -> ConfigSnapshotState {
        self.state
            .read()
            .expect("config snapshot state poisoned")
            .clone()
    }

    pub fn current_snapshot(&self) -> Arc<ConfigSnapshot> {
        self.current().snapshot
    }

    pub fn current_diagnostic(&self) -> Option<SnapshotDiagnostic> {
        self.current().diagnostic
    }

    pub fn reload(&self) -> ConfigSnapshotState {
        let _reload = self
            .reload_lock
            .lock()
            .expect("config snapshot reload lock poisoned");

        match self.load_candidate() {
            Ok(snapshot) => {
                let mut state = self.state.write().expect("config snapshot state poisoned");
                let next = ConfigSnapshotState {
                    snapshot: Arc::new(snapshot),
                    generation: state.generation + 1,
                    diagnostic: None,
                    authorization: SnapshotAuthorization::Authorized,
                };
                *state = next.clone();
                next
            }
            Err(CandidateLoadError::Storage { message }) => {
                let mut state = self.state.write().expect("config snapshot state poisoned");
                let next = ConfigSnapshotState {
                    snapshot: Arc::clone(&state.snapshot),
                    generation: state.generation,
                    diagnostic: Some(SnapshotDiagnostic {
                        code: SnapshotDiagnosticCode::LocalConfigUnauthorized,
                        message,
                    }),
                    authorization: SnapshotAuthorization::Unauthorized,
                };
                *state = next.clone();
                next
            }
            Err(CandidateLoadError::Unsupported { message }) => {
                let mut state = self.state.write().expect("config snapshot state poisoned");
                let next = ConfigSnapshotState {
                    snapshot: Arc::clone(&state.snapshot),
                    generation: state.generation,
                    diagnostic: Some(SnapshotDiagnostic {
                        code: SnapshotDiagnosticCode::LocalConfigUnsupported,
                        message,
                    }),
                    authorization: state.authorization,
                };
                *state = next.clone();
                next
            }
            Err(CandidateLoadError::Content { message }) => {
                let mut state = self.state.write().expect("config snapshot state poisoned");
                let next = ConfigSnapshotState {
                    snapshot: Arc::clone(&state.snapshot),
                    generation: state.generation,
                    diagnostic: Some(SnapshotDiagnostic {
                        code: SnapshotDiagnosticCode::LocalConfigReloadFailed,
                        message,
                    }),
                    authorization: state.authorization,
                };
                *state = next.clone();
                next
            }
        }
    }

    fn load_candidate(&self) -> Result<ConfigSnapshot, CandidateLoadError> {
        let root = root_string(&self.root);
        let root_dir = format!("{}/", root.trim_end_matches('/'));
        self.storage
            .ensure_dir(&root_dir)
            .map_err(|error| storage_error(&root, error))?;
        self.storage
            .ensure_dir(&fixed_path(&root, "catalog/"))
            .map_err(|error| storage_error(&root, error))?;
        let graph_storage = MemoryStorageHost::default();
        graph_storage
            .ensure_dir(&root_dir)
            .map_err(|error| storage_error(&root, error))?;
        graph_storage
            .ensure_dir(&fixed_path(&root, "catalog/"))
            .map_err(|error| storage_error(&root, error))?;
        let mut documents = Vec::new();
        for name in FILE_NAMES {
            ensure_fixed_file(self.storage.as_ref(), &root, name)?;
            let path = fixed_path(&root, name);
            let yaml = self
                .storage
                .read(&path)
                .map_err(|error| storage_error(&path, error))?;
            graph_storage
                .write(&path, &yaml)
                .map_err(|error| storage_error(&path, error))?;
            documents.push(yaml);
        }
        decode_config_documents(&documents[0], &documents[1], &documents[2])
            .map_err(|error| content_error(&root, error))?;

        let formats = FormatRegistry::with_builtins();
        let source_config =
            graph_source_config(&root).map_err(|error| CandidateLoadError::Content {
                message: sanitize_message(&root, &error.to_string()),
            })?;
        let transform = StrictReadableTransform;
        let graph = load_document_graph_sources(
            &graph_storage,
            &formats,
            &source_config,
            None,
            Some(&transform),
        )
        .map_err(|error| CandidateLoadError::Content {
            message: sanitize_message(&root, &error.to_string()),
        })?;
        let snapshot = snapshot_from_graph(&graph).map_err(|error| content_error(&root, error))?;
        classify_snapshot_support(&snapshot).map_err(|error| CandidateLoadError::Unsupported {
            message: sanitize_message(&root, &error.to_string()),
        })?;

        Ok(snapshot)
    }
}

fn snapshot_from_graph(graph: &LoadedDocumentGraph) -> Result<ConfigSnapshot, ConfigSchemaError> {
    Ok(ConfigSnapshot {
        host: decode_graph_host(graph)?,
        storage: decode_graph_collection(graph, "storage")?,
        providers: decode_graph_collection(graph, "providers")?,
        systems: decode_graph_collection(graph, "systems")?,
        families: decode_graph_collection(graph, "families")?,
        runners: decode_graph_collection(graph, "runners")?,
        profiles: decode_graph_collection(graph, "profiles")?,
        hooks: decode_graph_collection(graph, "hooks")?,
        games: decode_graph_collection(graph, "games")?,
        releases: decode_graph_collection(graph, "releases")?,
        locations: decode_graph_collection(graph, "locations")?,
    })
}

fn decode_graph_host(
    graph: &LoadedDocumentGraph,
) -> Result<Option<HostPayload>, ConfigSchemaError> {
    let Some(records) = graph.collections.get("host") else {
        return Ok(None);
    };
    if records.is_empty() {
        return Ok(None);
    }
    if records.len() != 1 || !records.contains_key("host") {
        return Err(ConfigSchemaError::Invalid {
            path: "document graph.host".to_owned(),
            message: "host collection must contain only the host record".to_owned(),
        });
    }
    decode_graph_record("host", "host", records.get("host").expect("host record")).map(Some)
}

fn decode_graph_collection<T>(
    graph: &LoadedDocumentGraph,
    collection: &str,
) -> Result<BTreeMap<String, T>, ConfigSchemaError>
where
    T: DeserializeOwned,
{
    let Some(records) = graph.collections.get(collection) else {
        return Ok(BTreeMap::new());
    };
    records
        .iter()
        .map(|(id, value)| Ok((id.clone(), decode_graph_record(collection, id, value)?)))
        .collect()
}

fn decode_graph_record<T>(collection: &str, id: &str, value: &Value) -> Result<T, ConfigSchemaError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value.clone()).map_err(|source| ConfigSchemaError::Invalid {
        path: format!("document graph.{collection}[{id}]"),
        message: source.to_string(),
    })
}

fn ensure_fixed_file(
    storage: &dyn StorageHost,
    root: &str,
    file_name: &str,
) -> Result<(), CandidateLoadError> {
    let path = fixed_path(root, file_name);
    if storage
        .exists(&path)
        .map_err(|error| storage_error(&path, error))?
    {
        return Ok(());
    }
    let canonical = std::str::from_utf8(EMPTY_DOCUMENT_BYTES).expect("canonical UTF-8 document");
    storage
        .write(&path, canonical)
        .map_err(|error| storage_error(&path, error))
}

fn graph_source_config(
    root: &str,
) -> Result<proseql_storage::source_config::NormalizedSourceConfig, EngineError> {
    let mut collections = BTreeMap::new();
    for section in ALL_SECTIONS {
        collections.insert(
            (*section).to_owned(),
            CollectionStorageConfig {
                name: (*section).to_owned(),
                schema: SchemaNode::Unknown,
                id_strategy: IdStrategy::Provided,
                version: None,
                migrations: Vec::new(),
            },
        );
    }
    normalize_source_config(SourceConfigInput {
        collections: collections.into_iter().collect(),
        sources: vec![DatabaseSourceConfig::DocumentGraph(
            DocumentGraphSourceConfig {
                id: "korri-readable".to_owned(),
                roots: vec![DocumentGraphRootConfig {
                    id: Some("fixed-local-config".to_owned()),
                    root: root.to_owned(),
                    optional: false,
                    include: Some(vec![
                        DEVICE_FILE_NAME.to_owned(),
                        GAMES_FILE_NAME.to_owned(),
                        RELEASES_FILE_NAME.to_owned(),
                    ]),
                    exclude: Vec::new(),
                    collections: Some(SourceCollectionSelection::Named(
                        ALL_SECTIONS
                            .iter()
                            .map(|section| (*section).to_owned())
                            .collect(),
                    )),
                }],
                collections: Some(SourceCollectionSelection::Named(
                    ALL_SECTIONS
                        .iter()
                        .map(|section| (*section).to_owned())
                        .collect(),
                )),
                include: None,
                exclude: Vec::new(),
                transform_callback_id: Some(TRANSFORM_CALLBACK_ID.to_owned()),
                on_fragment_error: DocumentGraphFragmentErrorPolicy::Error,
            },
        )],
    })
}

struct StrictReadableTransform;

impl DocumentGraphTransformHost for StrictReadableTransform {
    fn run_transform(
        &self,
        callback_id: &str,
        document: &Value,
        context: &DocumentGraphTransformContext,
    ) -> Result<Value, Value> {
        if callback_id != TRANSFORM_CALLBACK_ID {
            return Err(json!({ "message": format!("unknown transform callback {callback_id}") }));
        }
        let file_name = Path::new(&context.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(context.path.as_str());
        readable_document_to_graph(file_name, document)
            .map_err(|message| json!({ "message": message }))
    }
}

fn readable_document_to_graph(file_name: &str, document: &Value) -> Result<Value, String> {
    let Value::Object(input) = document else {
        return Err(format!("{file_name}: readable document must be an object"));
    };
    let allowed = match file_name {
        DEVICE_FILE_NAME => DEVICE_SECTIONS,
        "games.yaml" => GAMES_SECTIONS,
        "releases.yaml" => RELEASES_SECTIONS,
        other => return Err(format!("unexpected readable document {other}")),
    };

    let mut output = Map::new();
    for section in allowed {
        let Some(section_value) = input.get(*section) else {
            continue;
        };
        if *section == "host" {
            output.insert(
                (*section).to_owned(),
                Value::Object(Map::from_iter([("host".to_owned(), section_value.clone())])),
            );
        } else {
            output.insert((*section).to_owned(), section_value.clone());
        }
    }
    Ok(Value::Object(output))
}

#[derive(Debug)]
enum CandidateLoadError {
    Storage { message: String },
    Content { message: String },
    Unsupported { message: String },
}

fn storage_error(path: &str, error: EngineError) -> CandidateLoadError {
    CandidateLoadError::Storage {
        message: format!(
            "local configuration storage is unavailable while accessing {}: {}",
            fixed_file_label(path),
            sanitize_message(path, &error.to_string())
        ),
    }
}

fn content_error(root: &str, error: ConfigSchemaError) -> CandidateLoadError {
    CandidateLoadError::Content {
        message: sanitize_message(root, &error.to_string()),
    }
}

fn sanitize_message(root: &str, message: &str) -> String {
    message.replace(root, "<storage-root>")
}

fn fixed_file_label(path: &str) -> &str {
    let name = Path::new(path).file_name().and_then(|value| value.to_str());
    match name {
        Some(DEVICE_FILE_NAME) => DEVICE_FILE_NAME,
        Some("games.yaml") => GAMES_FILE_NAME,
        Some("releases.yaml") => RELEASES_FILE_NAME,
        _ => "fixed local configuration",
    }
}

fn fixed_path(root: &str, file_name: &str) -> String {
    format!("{}/{}", root.trim_end_matches('/'), file_name)
}

fn root_string(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}
