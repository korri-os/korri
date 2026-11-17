//! The local signer (CONTEXT.md): a separate service that makes recovery
//! words and signs with the person key they make (ADRs 0072 and 0082).
//!
//! It keeps no person key on disk. It holds a person key only in memory, in
//! two cases. A device in setup holds its new words until the person
//! confirms them; then the signer signs the device's owner statement and
//! erases them. Words that a person types for one action make a key for one
//! signature, and the signer erases it at once.
//!
//! korrid carries the words between the person and this service in memory
//! only (owner decision, 2026-11-16). korrid never makes the key and never
//! signs with it.

use crate::{
    identity::validate_signable_owner_statement_template,
    recovery_words::{Confirmation, RecoveryWords, CONFIRMATION_WORDS},
    remote_signer::{PersonSigner, PersonSignerRequest, PersonSignerState},
};
use futures::future::BoxFuture;
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::{Keys, PublicKey},
    types::Timestamp,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zeroize::{Zeroize, Zeroizing};

const MAX_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum LocalSignerError {
    #[error("local signer key is invalid")]
    InvalidKey,
    #[error("{0}")]
    Words(#[from] crate::recovery_words::RecoveryWordsError),
}

/// A person key in memory, made from recovery words, for the signatures of
/// one action. Dropping it erases the key.
pub struct LocalPersonSigner {
    keys: Keys,
    expected_device_public_key: String,
    state: Mutex<PersonSignerState>,
}

impl LocalPersonSigner {
    /// The person key of `words`. It signs owner statements only for
    /// `expected_device_public_key`.
    pub fn from_words(
        words: &RecoveryWords,
        expected_device_public_key: &str,
    ) -> Result<Self, LocalSignerError> {
        Ok(Self {
            keys: words.person_keys()?,
            expected_device_public_key: parse_public_key_text(expected_device_public_key)?,
            state: Mutex::new(PersonSignerState::Unavailable {
                message: "Local signer is ready".into(),
            }),
        })
    }

    pub fn public_key(&self) -> String {
        self.keys.public_key().to_hex()
    }

    fn approve(&self, request: &PersonSignerRequest) -> PersonSignerState {
        match self.sign(&request.unsigned_event_template) {
            Ok((owner_public_key, signed_event_json)) => PersonSignerState::Approved {
                owner_public_key,
                unsigned_event_template: request.unsigned_event_template.clone(),
                signed_event_json,
            },
            Err(()) => PersonSignerState::InvalidResponse {
                message: "Local signer accepts only a valid owner-binding or person-name template"
                    .into(),
            },
        }
    }

    fn sign(&self, template: &str) -> Result<(String, String), ()> {
        if template.len() > MAX_MESSAGE_BYTES {
            return Err(());
        }
        if let Ok(person_name) = crate::stream_play::person_name::validate_template(template) {
            let tags = person_name
                .tags
                .into_iter()
                .map(|tag| Tag::parse(tag).map_err(|_| ()))
                .collect::<Result<Vec<_>, _>>()?;
            let event = EventBuilder::new(Kind::Custom(person_name.kind), person_name.content)
                .tags(tags)
                .custom_created_at(Timestamp::from(person_name.created_at))
                .finalize(&self.keys)
                .map_err(|_| ())?;
            return Ok((self.keys.public_key().to_hex(), event.as_json()));
        }
        let template = validate_signable_owner_statement_template(template).map_err(|_| ())?;
        if template.device_public_key != self.expected_device_public_key {
            return Err(());
        }
        let tags = template
            .tags
            .into_iter()
            .map(|tag| Tag::parse(tag).map_err(|_| ()))
            .collect::<Result<Vec<_>, _>>()?;
        let event = EventBuilder::new(
            Kind::Custom(crate::identity::OWNER_EVENT_KIND),
            template.content,
        )
        .tags(tags)
        .custom_created_at(Timestamp::from(template.created_at))
        .finalize(&self.keys)
        .map_err(|_| ())?;
        Ok((self.keys.public_key().to_hex(), event.as_json()))
    }
}

impl PersonSigner for LocalPersonSigner {
    fn state(&self) -> PersonSignerState {
        self.state
            .lock()
            .expect("local signer state poisoned")
            .clone()
    }

    fn request<'a>(&'a self, request: PersonSignerRequest) -> BoxFuture<'a, PersonSignerState> {
        Box::pin(async move {
            let next = self.approve(&request);
            *self.state.lock().expect("local signer state poisoned") = next.clone();
            next
        })
    }
}

/// New words that the person must write down and confirm.
pub struct ShownWords {
    /// The 12 words, lowercase, one space between words.
    pub words: Zeroizing<String>,
    /// The places, from 1 to 12, of the words the person types back.
    pub positions: [usize; CONFIRMATION_WORDS],
}

impl std::fmt::Debug for ShownWords {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShownWords")
            .field("positions", &self.positions)
            .finish_non_exhaustive()
    }
}

/// How a confirmation ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfirmedWords {
    /// The words matched. The state holds the signed owner statement. The
    /// signer erased the words once it approved the statement.
    Signed(PersonSignerState),
    /// The words did not match. The signer keeps them for another try.
    Mismatch { message: String },
}

/// The words of a device in setup, held until the person confirms them.
struct PendingWords {
    words: RecoveryWords,
    confirmation: Confirmation,
}

/// The local signer's service state: the device it signs for in setup, and
/// the words that wait for confirmation. Nothing here is ever written.
pub struct LocalSigner {
    expected_device_public_key: String,
    pending: Mutex<Option<PendingWords>>,
}

impl LocalSigner {
    pub fn new(expected_device_public_key: &str) -> Result<Self, LocalSignerError> {
        Ok(Self {
            expected_device_public_key: parse_public_key_text(expected_device_public_key)?,
            pending: Mutex::new(None),
        })
    }

    /// Make new words for this device's setup. They replace any words that
    /// still wait for confirmation.
    pub fn make_words(&self) -> ShownWords {
        let words = RecoveryWords::generate();
        let confirmation = Confirmation::choose();
        let shown = ShownWords {
            words: words.phrase(),
            positions: confirmation.positions(),
        };
        *self.pending() = Some(PendingWords {
            words,
            confirmation,
        });
        shown
    }

    /// Check the person's answers. When they match, sign this device's
    /// owner statement with the words' person key and erase the words.
    pub fn confirm_words(
        &self,
        answers: &[&str],
        request: &PersonSignerRequest,
    ) -> Result<ConfirmedWords, String> {
        let mut pending = self.pending();
        let Some(waiting) = pending.as_ref() else {
            return Err("No recovery words wait for confirmation. Show new words.".into());
        };
        if let Err(error) = waiting.confirmation.check(&waiting.words, answers) {
            return Ok(ConfirmedWords::Mismatch {
                message: error.to_string(),
            });
        }
        let signer =
            LocalPersonSigner::from_words(&waiting.words, &self.expected_device_public_key)
                .map_err(|error| error.to_string())?;
        let state = signer.approve(request);
        if matches!(state, PersonSignerState::Approved { .. }) {
            *pending = None;
        }
        Ok(ConfirmedWords::Signed(state))
    }

    /// Erase any words that wait for confirmation.
    pub fn forget_words(&self) {
        *self.pending() = None;
    }

    /// Whether words wait for confirmation.
    pub fn holds_words(&self) -> bool {
        self.pending().is_some()
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, Option<PendingWords>> {
        self.pending.lock().expect("local signer words poisoned")
    }
}

/// The local signer's setup requests, in process or over its socket. Setup's
/// end uses this; the service and korrid's socket client both answer it.
pub trait SetupWords: Send + Sync {
    fn make_words(&self) -> BoxFuture<'_, Result<ShownWords, String>>;
    fn confirm_words<'a>(
        &'a self,
        answers: &'a [String],
        request: PersonSignerRequest,
    ) -> BoxFuture<'a, Result<ConfirmedWords, String>>;
    fn forget_words(&self) -> BoxFuture<'_, Result<(), String>>;
}

impl SetupWords for LocalSigner {
    fn make_words(&self) -> BoxFuture<'_, Result<ShownWords, String>> {
        Box::pin(async move { Ok(LocalSigner::make_words(self)) })
    }

    fn confirm_words<'a>(
        &'a self,
        answers: &'a [String],
        request: PersonSignerRequest,
    ) -> BoxFuture<'a, Result<ConfirmedWords, String>> {
        Box::pin(async move {
            let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
            LocalSigner::confirm_words(self, &answers, &request)
        })
    }

    fn forget_words(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            LocalSigner::forget_words(self);
            Ok(())
        })
    }
}

impl SetupWords for UnixLocalSigner {
    fn make_words(&self) -> BoxFuture<'_, Result<ShownWords, String>> {
        Box::pin(UnixLocalSigner::make_words(self))
    }

    fn confirm_words<'a>(
        &'a self,
        answers: &'a [String],
        request: PersonSignerRequest,
    ) -> BoxFuture<'a, Result<ConfirmedWords, String>> {
        Box::pin(UnixLocalSigner::confirm_words(self, answers, request))
    }

    fn forget_words(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(UnixLocalSigner::forget_words(self))
    }
}

/// Sign one template with the person key of words the person typed. The key
/// lives only during this call.
pub fn sign_with_words(
    words: &str,
    expected_device_public_key: &str,
    request: &PersonSignerRequest,
) -> Result<PersonSignerState, LocalSignerError> {
    let words = RecoveryWords::parse(words)?;
    let signer = LocalPersonSigner::from_words(&words, expected_device_public_key)?;
    Ok(signer.approve(request))
}

/// One request to the local signer. Debug shows only its tag, and the words
/// it carries are erased when it is dropped.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
pub enum LocalSignerWireRequest {
    MakeWords,
    ConfirmWords {
        answers: Vec<String>,
        request: PersonSignerRequest,
    },
    ForgetWords,
    SignWithWords {
        words: String,
        expected_device_public_key: String,
        request: PersonSignerRequest,
    },
}

impl LocalSignerWireRequest {
    fn tag(&self) -> &'static str {
        match self {
            Self::MakeWords => "MakeWords",
            Self::ConfirmWords { .. } => "ConfirmWords",
            Self::ForgetWords => "ForgetWords",
            Self::SignWithWords { .. } => "SignWithWords",
        }
    }
}

impl std::fmt::Debug for LocalSignerWireRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.tag())
    }
}

impl LocalSignerWireRequest {
    /// Erase the words this request carries. Drop calls it.
    fn erase(&mut self) {
        match self {
            Self::ConfirmWords { answers, .. } => answers.iter_mut().for_each(Zeroize::zeroize),
            Self::SignWithWords { words, .. } => words.zeroize(),
            Self::MakeWords | Self::ForgetWords => {}
        }
    }
}

impl Drop for LocalSignerWireRequest {
    fn drop(&mut self) {
        self.erase();
    }
}

/// One answer from the local signer. Debug never shows words.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
pub enum LocalSignerWireResponse {
    Words {
        words: String,
        positions: [usize; CONFIRMATION_WORDS],
    },
    WordsMismatch {
        message: String,
    },
    Sign(PersonSignerState),
    Ok,
    Error {
        message: String,
    },
}

impl std::fmt::Debug for LocalSignerWireResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Words { positions, .. } => formatter
                .debug_struct("Words")
                .field("positions", positions)
                .finish_non_exhaustive(),
            Self::WordsMismatch { message } | Self::Error { message } => {
                formatter.debug_tuple("Error").field(message).finish()
            }
            Self::Sign(state) => formatter.debug_tuple("Sign").field(state).finish(),
            Self::Ok => formatter.write_str("Ok"),
        }
    }
}

impl LocalSignerWireResponse {
    /// Erase the words this response carries. Drop calls it.
    fn erase(&mut self) {
        if let Self::Words { words, .. } = self {
            words.zeroize();
        }
    }
}

impl Drop for LocalSignerWireResponse {
    fn drop(&mut self) {
        self.erase();
    }
}

/// korrid's side of the local signer socket.
#[derive(Clone)]
pub struct UnixLocalSigner {
    socket: PathBuf,
}

impl UnixLocalSigner {
    pub fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    pub async fn make_words(&self) -> Result<ShownWords, String> {
        match &request_over_unix(&self.socket, &LocalSignerWireRequest::MakeWords).await? {
            LocalSignerWireResponse::Words { words, positions } => Ok(ShownWords {
                words: Zeroizing::new(words.clone()),
                positions: *positions,
            }),
            LocalSignerWireResponse::Error { message } => Err(message.clone()),
            _ => Err(WRONG_RESPONSE.into()),
        }
    }

    pub async fn confirm_words(
        &self,
        answers: &[String],
        request: PersonSignerRequest,
    ) -> Result<ConfirmedWords, String> {
        let request = LocalSignerWireRequest::ConfirmWords {
            answers: answers.to_vec(),
            request,
        };
        match &request_over_unix(&self.socket, &request).await? {
            LocalSignerWireResponse::Sign(state) => Ok(ConfirmedWords::Signed(state.clone())),
            LocalSignerWireResponse::WordsMismatch { message } => Ok(ConfirmedWords::Mismatch {
                message: message.clone(),
            }),
            LocalSignerWireResponse::Error { message } => Err(message.clone()),
            _ => Err(WRONG_RESPONSE.into()),
        }
    }

    pub async fn forget_words(&self) -> Result<(), String> {
        match &request_over_unix(&self.socket, &LocalSignerWireRequest::ForgetWords).await? {
            LocalSignerWireResponse::Ok => Ok(()),
            LocalSignerWireResponse::Error { message } => Err(message.clone()),
            _ => Err(WRONG_RESPONSE.into()),
        }
    }

    pub async fn sign_with_words(
        &self,
        words: &str,
        expected_device_public_key: &str,
        request: PersonSignerRequest,
    ) -> Result<PersonSignerState, String> {
        let request = LocalSignerWireRequest::SignWithWords {
            words: words.to_owned(),
            expected_device_public_key: expected_device_public_key.to_owned(),
            request,
        };
        match &request_over_unix(&self.socket, &request).await? {
            LocalSignerWireResponse::Sign(state) => Ok(state.clone()),
            LocalSignerWireResponse::Error { message } => Err(message.clone()),
            _ => Err(WRONG_RESPONSE.into()),
        }
    }
}

const WRONG_RESPONSE: &str = "Local signer returned the wrong response";

/// Answer one request on the local signer's socket.
pub async fn serve_connection(
    mut stream: tokio::net::UnixStream,
    signer: Arc<LocalSigner>,
) -> Result<(), std::io::Error> {
    let mut bytes = Zeroizing::new(Vec::new());
    (&mut stream)
        .take((MAX_MESSAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    let response = if bytes.len() > MAX_MESSAGE_BYTES {
        LocalSignerWireResponse::Error {
            message: "Local signer request is too large".into(),
        }
    } else {
        match serde_json::from_slice::<LocalSignerWireRequest>(&bytes) {
            Ok(request) => handle(&signer, &request),
            Err(_) => LocalSignerWireResponse::Error {
                message: "Local signer request is invalid".into(),
            },
        }
    };
    let response = Zeroizing::new(serde_json::to_vec(&response).map_err(std::io::Error::other)?);
    stream.write_all(&response).await?;
    stream.shutdown().await
}

fn handle(signer: &LocalSigner, request: &LocalSignerWireRequest) -> LocalSignerWireResponse {
    match request {
        LocalSignerWireRequest::MakeWords => {
            let shown = signer.make_words();
            LocalSignerWireResponse::Words {
                words: shown.words.as_str().to_owned(),
                positions: shown.positions,
            }
        }
        LocalSignerWireRequest::ConfirmWords { answers, request } => {
            let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
            match signer.confirm_words(&answers, request) {
                Ok(ConfirmedWords::Signed(state)) => LocalSignerWireResponse::Sign(state),
                Ok(ConfirmedWords::Mismatch { message }) => {
                    LocalSignerWireResponse::WordsMismatch { message }
                }
                Err(message) => LocalSignerWireResponse::Error { message },
            }
        }
        LocalSignerWireRequest::ForgetWords => {
            signer.forget_words();
            LocalSignerWireResponse::Ok
        }
        LocalSignerWireRequest::SignWithWords {
            words,
            expected_device_public_key,
            request,
        } => match sign_with_words(words, expected_device_public_key, request) {
            Ok(state) => LocalSignerWireResponse::Sign(state),
            Err(error) => LocalSignerWireResponse::Error {
                message: error.to_string(),
            },
        },
    }
}

async fn request_over_unix(
    socket: &Path,
    request: &LocalSignerWireRequest,
) -> Result<LocalSignerWireResponse, String> {
    let bytes = Zeroizing::new(
        serde_json::to_vec(request).map_err(|_| "local signer request failed".to_owned())?,
    );
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("local signer request is too large".into());
    }
    let mut stream = tokio::net::UnixStream::connect(socket)
        .await
        .map_err(|_| "local signer is unavailable".to_owned())?;
    stream
        .write_all(&bytes)
        .await
        .map_err(|_| "local signer request failed".to_owned())?;
    stream
        .shutdown()
        .await
        .map_err(|_| "local signer request failed".to_owned())?;
    let mut response = Zeroizing::new(Vec::new());
    stream
        .take((MAX_MESSAGE_BYTES + 1) as u64)
        .read_to_end(&mut response)
        .await
        .map_err(|_| "local signer response failed".to_owned())?;
    if response.len() > MAX_MESSAGE_BYTES {
        return Err("local signer response is too large".into());
    }
    serde_json::from_slice(&response).map_err(|_| "local signer response is invalid".into())
}

fn parse_public_key_text(value: &str) -> Result<String, LocalSignerError> {
    let value = value.trim();
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(LocalSignerError::InvalidKey);
    }
    let key = PublicKey::from_hex(value).map_err(|_| LocalSignerError::InvalidKey)?;
    key.xonly().map_err(|_| LocalSignerError::InvalidKey)?;
    Ok(value.to_owned())
}

#[cfg(test)]
#[path = "local_signer_tests.rs"]
mod tests;
