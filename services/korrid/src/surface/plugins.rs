//! Ports of `clients/portal/src/surface/plugin-repositories.ts` (pure: the
//! rows and their words) and `plugin-repositories-controller.ts` (the
//! effects): plugin repositories, what each offers, what is installed from
//! it, the trusted publishers, and the two-step approval a first "on" needs.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use korri_surface_treaty::{
    SurfaceSettingChoice, SurfaceSettingConfirmation, SurfaceSettingGroup,
    SurfaceSettingInteraction, SurfaceSettingItem, SurfaceSettingsStatus,
};

use super::korrid::{ask, Korrid};
use crate::{
    plugin_admin::{
        PluginCatalogEntry, PluginInspectRequest, PluginInspection, PluginInstallRequest,
        PluginInstalledState, PluginPublisher, PluginPublisherEntry, PluginPublisherState,
        PluginPublishers, PluginPublishersRequest, PluginRepositories, PluginRepositoriesRequest,
        PluginRepositoryAddResult, PluginRepositoryInstall, PluginRepositoryRequest,
        PluginRevokePublisherRequest, PluginSetEnabledRequest, PluginTrustPublisherRequest,
    },
    PluginCatalogOutcome, PluginChangeOutcome, PluginInspectOutcome, PluginPublishersOutcome,
    PluginRepositoriesOutcome, PluginRepositoryAddOutcome, PluginRepositoryRemoveOutcome,
    PluginSetting, RpcFailure,
};

pub const UNSIGNED_DISCLOSURE: &str = "This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from.";
/// The host refuses an "on" whose stored approval no longer matches.
pub const NEEDS_REVIEW_CODE: &str = "PluginNeedsReview";

fn warning_text(warning: &str) -> Option<String> {
    let text = warning.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

// ---------------------------------------------------------------- state

#[derive(Clone, Debug, PartialEq)]
pub enum CatalogView {
    Checking,
    Ready(Vec<PluginCatalogEntry>),
    Unreachable(String),
}

/// One release a person may install: where it comes from, and which.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseTarget {
    pub source_url: String,
    pub plugin_id: String,
    pub release_version: String,
}

#[derive(Clone, Debug)]
pub struct PluginReview {
    pub target: ReleaseTarget,
    pub report: PluginInspection,
}

#[derive(Clone, Debug)]
pub enum Read<T> {
    Loading,
    Failed(String),
    Ready(T),
}

#[derive(Clone, Debug)]
pub struct PluginFacts {
    pub repositories: Read<PluginRepositories>,
    /// By repository URL.
    pub catalogs: BTreeMap<String, CatalogView>,
    /// Inspection reports waiting for the person's yes, by release key.
    pub reviews: BTreeMap<String, PluginReview>,
    /// Installed plugins whose stored approval the host called stale, with
    /// the state the person asked for when it refused.
    pub needs_review: BTreeMap<String, bool>,
    pub publishers: Read<PluginPublishers>,
}

impl Default for PluginFacts {
    fn default() -> Self {
        Self {
            repositories: Read::Loading,
            catalogs: BTreeMap::new(),
            reviews: BTreeMap::new(),
            needs_review: BTreeMap::new(),
            publishers: Read::Loading,
        }
    }
}

// ---------------------------------------------------------------- ids

/// JavaScript `encodeURIComponent`.
pub fn enc(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// JavaScript `decodeURIComponent`; None where it would throw.
pub fn dec(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub const ADD_REPOSITORY_SETTING: &str = "plugin-repository-add";
pub const OFFICIAL_REPOSITORY_ROW: &str = "plugin-repository:official";
const REPOSITORY_ROW: &str = "plugin-repository:";
const REMOVE_ACTION: &str = "plugin-repository-remove:";
const RELEASE_ROW: &str = "plugin-release:";
const INSPECT_ACTION: &str = "plugin-inspect:";
const REVIEW_ACTION: &str = "plugin-review:";
const TOGGLE_SETTING: &str = "plugin-toggle:";
const CREDENTIAL_SETTING: &str = "plugin-credential:";
const TRUST_ACTION: &str = "plugin-trust:";
const REVIEW_PUBLISHER_ROW: &str = "plugin-review-publisher:";
const PUBLISHER_ROW: &str = "plugin-publisher:";
const REVOKE_ACTION: &str = "plugin-publisher-revoke:";
pub const PUBLISHERS_STATUS_ROW: &str = "plugin-publishers:status";

pub fn release_key(target: &ReleaseTarget) -> String {
    [
        &target.source_url,
        &target.plugin_id,
        &target.release_version,
    ]
    .iter()
    .map(|part| enc(part))
    .collect::<Vec<_>>()
    .join(":")
}

fn target_of(key: &str) -> Option<ReleaseTarget> {
    let parts: Vec<Option<String>> = key.split(':').map(dec).collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.as_deref().is_none_or(str::is_empty))
    {
        return None;
    }
    let mut parts = parts.into_iter().flatten();
    Some(ReleaseTarget {
        source_url: parts.next()?,
        plugin_id: parts.next()?,
        release_version: parts.next()?,
    })
}

pub fn repository_row_id(url: &str) -> String {
    format!("{REPOSITORY_ROW}{}", enc(url))
}
pub fn remove_repository_action_id(url: &str) -> String {
    format!("{REMOVE_ACTION}{}", enc(url))
}
pub fn release_row_id(target: &ReleaseTarget) -> String {
    format!("{RELEASE_ROW}{}", release_key(target))
}
pub fn inspect_action_id(target: &ReleaseTarget) -> String {
    format!("{INSPECT_ACTION}{}", release_key(target))
}
pub fn review_action_id(target: &ReleaseTarget) -> String {
    format!("{REVIEW_ACTION}{}", release_key(target))
}
pub fn toggle_setting_id(plugin_id: &str) -> String {
    format!("{TOGGLE_SETTING}{}", enc(plugin_id))
}
/// The row where a person enters a plugin's secret (ADR 0034, "Entry").
pub fn credential_setting_id(plugin_id: &str) -> String {
    format!("{CREDENTIAL_SETTING}{}", enc(plugin_id))
}
/// The plugin a secret row belongs to.
pub fn credential_plugin_from(setting_id: &str) -> Option<String> {
    dec(setting_id.strip_prefix(CREDENTIAL_SETTING)?).filter(|id| !id.is_empty())
}
pub fn trust_action_id(target: &ReleaseTarget, namespace: &str) -> String {
    format!("{TRUST_ACTION}{}:{}", release_key(target), enc(namespace))
}
pub fn review_publisher_row_id(target: &ReleaseTarget, namespace: &str) -> String {
    format!(
        "{REVIEW_PUBLISHER_ROW}{}:{}",
        release_key(target),
        enc(namespace)
    )
}
pub fn publisher_row_id(namespace: &str) -> String {
    format!("{PUBLISHER_ROW}{}", enc(namespace))
}
pub fn revoke_action_id(namespace: &str) -> String {
    format!("{REVOKE_ACTION}{}", enc(namespace))
}

#[derive(Clone, Debug, PartialEq)]
pub enum PluginAction {
    Remove(String),
    Inspect(ReleaseTarget),
    Review(ReleaseTarget),
    Trust(ReleaseTarget, String),
    Revoke(String),
}

pub fn plugin_action_from(action_id: &str) -> Option<PluginAction> {
    if let Some(url) = action_id.strip_prefix(REMOVE_ACTION) {
        return dec(url)
            .filter(|url| !url.is_empty())
            .map(PluginAction::Remove);
    }
    if let Some(namespace) = action_id.strip_prefix(REVOKE_ACTION) {
        return dec(namespace)
            .filter(|namespace| !namespace.is_empty())
            .map(PluginAction::Revoke);
    }
    if let Some(key) = action_id.strip_prefix(TRUST_ACTION) {
        let split = key.rfind(':').filter(|split| *split > 0)?;
        let target = target_of(&key[..split])?;
        let namespace = dec(&key[split + 1..]).filter(|namespace| !namespace.is_empty())?;
        return Some(PluginAction::Trust(target, namespace));
    }
    if let Some(key) = action_id.strip_prefix(INSPECT_ACTION) {
        return target_of(key).map(PluginAction::Inspect);
    }
    if let Some(key) = action_id.strip_prefix(REVIEW_ACTION) {
        return target_of(key).map(PluginAction::Review);
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
pub enum PluginChange {
    AddRepository(String),
    Toggle(String, bool),
}

pub fn plugin_change_from(setting_id: &str, value: &str) -> Option<PluginChange> {
    if setting_id == ADD_REPOSITORY_SETTING {
        return Some(PluginChange::AddRepository(value.into()));
    }
    let plugin_id = dec(setting_id.strip_prefix(TOGGLE_SETTING)?).filter(|id| !id.is_empty())?;
    match value {
        "true" => Some(PluginChange::Toggle(plugin_id, true)),
        "false" => Some(PluginChange::Toggle(plugin_id, false)),
        _ => None,
    }
}

pub fn is_plugin_setting_id(setting_id: &str) -> bool {
    setting_id == ADD_REPOSITORY_SETTING || setting_id.starts_with(TOGGLE_SETTING)
}

// ---------------------------------------------------------------- reducers

/// The address the person typed, if it is one Korri may fetch.
pub fn https_address(typed: &str) -> Option<String> {
    let text = typed.trim();
    if !text
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
    {
        return None;
    }
    let url = url::Url::parse(text).ok()?;
    (url.scheme() == "https" && url.host_str().is_some_and(|host| !host.is_empty()))
        .then(|| text.to_owned())
}

pub const INVALID_ADDRESS: &str = "Use an https:// address";
pub const ALREADY_ADDED: &str = "Already added";

pub fn with_publishers(mut facts: PluginFacts, outcome: PluginPublishersOutcome) -> PluginFacts {
    match outcome {
        PluginPublishersOutcome::Ok(value) => facts.publishers = Read::Ready(value),
        // A failed re-read keeps what was last read.
        PluginPublishersOutcome::Err(failure) => {
            if !matches!(facts.publishers, Read::Ready(_)) {
                facts.publishers = Read::Failed(failure.message);
            }
        }
    }
    facts
}

pub fn with_repositories(
    mut facts: PluginFacts,
    outcome: PluginRepositoriesOutcome,
) -> PluginFacts {
    match outcome {
        PluginRepositoriesOutcome::Ok(value) => facts.repositories = Read::Ready(value),
        PluginRepositoriesOutcome::Err(failure) => {
            if !matches!(facts.repositories, Read::Ready(_)) {
                facts.repositories = Read::Failed(failure.message);
            }
        }
    }
    facts
}

pub fn configured_repositories(repositories: &PluginRepositories) -> Vec<String> {
    repositories
        .official
        .iter()
        .cloned()
        .chain(repositories.user_added.iter().cloned())
        .collect()
}

/// Mark catalogs being read. A catalog already read keeps its rows meanwhile.
pub fn checking_catalogs(mut facts: PluginFacts, urls: &[String]) -> PluginFacts {
    facts.catalogs = urls
        .iter()
        .map(|url| {
            (
                url.clone(),
                facts
                    .catalogs
                    .get(url)
                    .cloned()
                    .unwrap_or(CatalogView::Checking),
            )
        })
        .collect();
    facts
}

pub fn with_catalog(
    mut facts: PluginFacts,
    url: &str,
    outcome: PluginCatalogOutcome,
) -> PluginFacts {
    facts.catalogs.insert(
        url.into(),
        match outcome {
            PluginCatalogOutcome::Ok(catalog) => CatalogView::Ready(catalog.entries),
            PluginCatalogOutcome::Err(failure) => CatalogView::Unreachable(failure.message),
        },
    );
    facts
}

pub fn without_repository(mut facts: PluginFacts, url: &str) -> PluginFacts {
    facts.catalogs.remove(url);
    facts
        .reviews
        .retain(|_, review| review.target.source_url != url);
    facts
}

pub fn with_review(
    mut facts: PluginFacts,
    target: &ReleaseTarget,
    report: PluginInspection,
) -> PluginFacts {
    facts.reviews.insert(
        release_key(target),
        PluginReview {
            target: target.clone(),
            report,
        },
    );
    facts
}

pub fn review_for<'a>(facts: &'a PluginFacts, target: &ReleaseTarget) -> Option<&'a PluginReview> {
    facts.reviews.get(&release_key(target))
}

pub fn without_report(mut facts: PluginFacts, target: &ReleaseTarget) -> PluginFacts {
    facts.reviews.remove(&release_key(target));
    facts
}

pub fn without_review(mut facts: PluginFacts, target: &ReleaseTarget) -> PluginFacts {
    facts.reviews.remove(&release_key(target));
    facts.needs_review.remove(&target.plugin_id);
    facts
}

pub fn with_needs_review(mut facts: PluginFacts, plugin_id: &str, wanted: bool) -> PluginFacts {
    facts.needs_review.insert(plugin_id.into(), wanted);
    facts
}

pub fn wanted_after_review(facts: &PluginFacts, plugin_id: &str) -> bool {
    facts.needs_review.get(plugin_id).copied().unwrap_or(true)
}

pub fn install_of<'a>(
    facts: &'a PluginFacts,
    plugin_id: &str,
) -> Option<&'a PluginRepositoryInstall> {
    match &facts.repositories {
        Read::Ready(value) => value
            .repository_installs
            .iter()
            .find(|install| install.id == plugin_id),
        _ => None,
    }
}

pub fn row_id_for_release(facts: &PluginFacts, target: &ReleaseTarget) -> String {
    match install_of(facts, &target.plugin_id) {
        Some(install) if install.source_url == target.source_url => {
            toggle_setting_id(&target.plugin_id)
        }
        _ => release_row_id(target),
    }
}

// ---------------------------------------------------------------- words

pub fn repository_name(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    let without = if lower.starts_with("https://") {
        &url[8..]
    } else {
        url
    };
    without.trim_end_matches('/').to_owned()
}

pub fn publisher_of(plugin_id: &str) -> &str {
    match plugin_id.find(':') {
        Some(colon) if colon > 0 => &plugin_id[..colon],
        _ => plugin_id,
    }
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

pub fn is_trusted_publisher(publisher: &PluginPublisher) -> bool {
    matches!(
        publisher.state,
        PluginPublisherState::Bound | PluginPublisherState::Trusted
    )
}

fn awaits_trust(publisher: &PluginPublisher) -> bool {
    matches!(
        publisher.state,
        PluginPublisherState::New | PluginPublisherState::KeyChanged
    )
}

pub fn publisher_state_label(state: PluginPublisherState) -> &'static str {
    match state {
        PluginPublisherState::Bound => "Configured",
        PluginPublisherState::Trusted => "Trusted",
        PluginPublisherState::New => "Not trusted yet",
        PluginPublisherState::KeyChanged => "Key changed",
        PluginPublisherState::Unsigned => "Not signed",
    }
}

fn state_in_a_line(state: PluginPublisherState) -> &'static str {
    match state {
        PluginPublisherState::Bound => "configured",
        PluginPublisherState::Trusted => "trusted",
        PluginPublisherState::New => "not trusted yet",
        PluginPublisherState::KeyChanged => "key changed",
        PluginPublisherState::Unsigned => "not signed",
    }
}

fn key_part(publisher: &PluginPublisher) -> String {
    publisher
        .fingerprint
        .as_ref()
        .filter(|key| !key.is_empty())
        .map_or(String::new(), |key| format!(" Key {key}."))
}

/// Every publisher an inspected install brings, the plugin's own first, once
/// each. When one namespace comes with two states, the one that asks wins.
pub fn publishers_of(report: &PluginInspection) -> Vec<PluginPublisher> {
    let mut out: Vec<PluginPublisher> = Vec::new();
    for publisher in
        std::iter::once(&report.publisher).chain(report.brings.iter().map(|bring| &bring.publisher))
    {
        match out
            .iter_mut()
            .find(|known| known.namespace == publisher.namespace)
        {
            None => out.push(publisher.clone()),
            Some(known) => {
                if !awaits_trust(known) && awaits_trust(publisher) {
                    *known = publisher.clone();
                }
            }
        }
    }
    out
}

pub fn publishers_to_trust(report: &PluginInspection) -> Vec<PluginPublisher> {
    publishers_of(report)
        .into_iter()
        .filter(awaits_trust)
        .collect()
}

fn brings_unsigned(report: &PluginInspection) -> bool {
    std::iter::once(&report.publisher)
        .chain(report.brings.iter().map(|bring| &bring.publisher))
        .any(|publisher| publisher.state == PluginPublisherState::Unsigned)
}

pub fn key_change_lines(publisher: &PluginPublisher) -> Vec<String> {
    vec![
        format!("The publisher key for {} changed.", publisher.namespace),
        format!(
            "Before: {}.",
            publisher
                .previous_fingerprint
                .as_deref()
                .unwrap_or("unknown")
        ),
        format!(
            "Now: {}.",
            publisher.fingerprint.as_deref().unwrap_or("unknown")
        ),
        "This can mean the author replaced their key, or that someone else is publishing as them."
            .into(),
        "Trust the new key only if the author told you about the change.".into(),
    ]
}

fn disclosure_of(publisher: &PluginPublisher) -> String {
    publisher
        .disclosure
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or(UNSIGNED_DISCLOSURE)
        .to_owned()
}

fn publisher_line(publisher: &PluginPublisher) -> String {
    let name = &publisher.namespace;
    match publisher.state {
        PluginPublisherState::Bound => format!(
            "Publisher {name}, trusted by this device's configuration.{}",
            key_part(publisher)
        ),
        PluginPublisherState::Trusted => format!(
            "Publisher {name}, trusted on this device.{}",
            key_part(publisher)
        ),
        PluginPublisherState::New => {
            format!("Publisher {name}, not trusted yet.{}", key_part(publisher))
        }
        PluginPublisherState::KeyChanged => format!(
            "Publisher {name}: key changed. Before {}. Now {}.",
            publisher
                .previous_fingerprint
                .as_deref()
                .unwrap_or("unknown"),
            publisher.fingerprint.as_deref().unwrap_or("unknown")
        ),
        PluginPublisherState::Unsigned => format!("Publisher {name}, not signed."),
    }
}

/// The approval report as plain lines, one fact per line.
pub fn review_message(report: &PluginInspection) -> String {
    let mut lines = Vec::new();
    if report.publisher.state == PluginPublisherState::Unsigned {
        lines.push(disclosure_of(&report.publisher));
    }
    for bring in &report.brings {
        if bring.publisher.state == PluginPublisherState::Unsigned {
            lines.push(format!(
                "{}: {}",
                bring.title.as_deref().unwrap_or(&bring.id),
                disclosure_of(&bring.publisher)
            ));
        }
    }
    lines.push(publisher_line(&report.publisher));
    let mut asks = Vec::new();
    if let Some(warning) = warning_text(&report.warning) {
        asks.push(format!("Warning: {warning}"));
    }
    let list = |ports: &[u16]| {
        ports
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut ports = Vec::new();
    if !report.ports.allowed_tcp_ports.is_empty() {
        ports.push(format!("TCP {}", list(&report.ports.allowed_tcp_ports)));
    }
    if !report.ports.allowed_udp_ports.is_empty() {
        ports.push(format!("UDP {}", list(&report.ports.allowed_udp_ports)));
    }
    if !ports.is_empty() {
        asks.push(format!("Opens network ports: {}.", ports.join("; ")));
    }
    if !report.native_units.is_empty() {
        asks.push(format!(
            "Runs in the background: {}.",
            report.native_units.join(", ")
        ));
    }
    for bring in &report.brings {
        let name = bring.title.as_deref().unwrap_or(&bring.id);
        let from = format!(
            "from {} ({})",
            bring.publisher.namespace,
            state_in_a_line(bring.publisher.state)
        );
        if bring.already_approved {
            asks.push(format!("Also uses {name} {from}. You approved it before."));
        } else {
            let warning = warning_text(&bring.warning)
                .map_or(String::new(), |warning| format!(" Warning: {warning}"));
            asks.push(format!("Also installs {name} {from}.{warning}"));
        }
    }
    if asks.is_empty() {
        asks.push("It asks for no network ports and runs nothing in the background.".into());
    }
    lines.extend(asks);
    lines.join("\n")
}

fn publisher_description(publisher: &PluginPublisher) -> String {
    let key = publisher
        .fingerprint
        .as_ref()
        .filter(|key| !key.is_empty())
        .map_or(String::new(), |key| format!("Key {key}."));
    match publisher.state {
        PluginPublisherState::Bound => format!("{key} This device's configuration trusts it.")
            .trim()
            .to_owned(),
        PluginPublisherState::Trusted => format!("{key} You trusted it on this device.")
            .trim()
            .to_owned(),
        PluginPublisherState::New => {
            format!("{key} Check it with the plugin maker before you trust it.")
                .trim()
                .to_owned()
        }
        PluginPublisherState::KeyChanged => format!(
            "Before {}. Now {}.",
            publisher
                .previous_fingerprint
                .as_deref()
                .unwrap_or("unknown"),
            publisher.fingerprint.as_deref().unwrap_or("unknown")
        ),
        PluginPublisherState::Unsigned => disclosure_of(publisher),
    }
}

// ---------------------------------------------------------------- rows

fn item(id: impl Into<String>, label: impl Into<String>) -> SurfaceSettingItem {
    SurfaceSettingItem::new(id, label)
}

fn action(
    action_id: String,
    destructive: Option<bool>,
    confirmation: Option<(String, String, String)>,
) -> SurfaceSettingInteraction {
    SurfaceSettingInteraction::Action {
        action_id,
        destructive,
        confirmation: confirmation.map(|(title, message, confirm_label)| {
            SurfaceSettingConfirmation {
                title,
                message,
                confirm_label,
            }
        }),
    }
}

fn trust_row(
    id: String,
    label: &str,
    review: &PluginReview,
    publisher: &PluginPublisher,
    waiting: &[PluginPublisher],
) -> SurfaceSettingItem {
    let plugin = review
        .report
        .title
        .clone()
        .unwrap_or_else(|| label.to_owned());
    let later: Vec<String> = waiting
        .iter()
        .filter(|other| other.namespace != publisher.namespace)
        .map(|other| other.namespace.clone())
        .collect();
    let then: Vec<String> = if later.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "After this, {plugin} also needs you to trust {}.",
            later.join(", ")
        )]
    };
    if publisher.public_key.is_none() {
        // Nothing to send: the host must report the key before anyone trusts it.
        return SurfaceSettingItem {
            value: Some("Check again".into()),
            description: Some(format!(
                "Korri did not get the key of {}. Check the plugin again.",
                publisher.namespace
            )),
            interaction: Some(action(inspect_action_id(&review.target), None, None)),
            ..item(id, label)
        };
    }
    if publisher.state == PluginPublisherState::KeyChanged {
        let mut message = key_change_lines(publisher);
        message.extend(then);
        return SurfaceSettingItem {
            value: Some("Trust the new key".into()),
            description: Some(format!(
                "The publisher key for {} changed.",
                publisher.namespace
            )),
            interaction: Some(action(
                trust_action_id(&review.target, &publisher.namespace),
                Some(true),
                Some((
                    format!("Trust the new key for {}?", publisher.namespace),
                    message.join("\n"),
                    "Trust the new key".into(),
                )),
            )),
            ..item(id, label)
        };
    }
    let own = publisher.namespace == review.report.publisher.namespace;
    let needs = review
        .report
        .brings
        .iter()
        .find(|bring| bring.publisher.namespace == publisher.namespace);
    let first = match (own, needs) {
        (false, Some(needs)) => format!(
            "{plugin} needs {} from {}. This device has not trusted it before.",
            needs.title.as_deref().unwrap_or(&needs.id),
            publisher.namespace
        ),
        _ => format!(
            "{plugin} comes from {}. This device has not trusted it before.",
            publisher.namespace
        ),
    };
    let mut message = vec![
        first,
        format!(
            "Key {}.",
            publisher.fingerprint.as_deref().unwrap_or("unknown")
        ),
        "Trust it only if the plugin maker gave you this key.".into(),
        format!(
            "Later plugins from {} must have the same key.",
            publisher.namespace
        ),
    ];
    message.extend(then);
    SurfaceSettingItem {
        value: Some(format!("Trust {}", publisher.namespace)),
        description: Some(format!(
            "{} is not trusted yet. Check its key, then trust it.",
            publisher.namespace
        )),
        interaction: Some(action(
            trust_action_id(&review.target, &publisher.namespace),
            None,
            Some((
                format!("Trust {}?", publisher.namespace),
                message.join("\n"),
                "Trust".into(),
            )),
        )),
        ..item(id, label)
    }
}

fn review_rows(
    id: String,
    label: &str,
    review: &PluginReview,
    description: Option<String>,
    wanted: bool,
) -> Vec<SurfaceSettingItem> {
    let waiting = publishers_to_trust(&review.report);
    let verb = if wanted { "Turn on" } else { "Turn off" };
    let step = match waiting.first() {
        Some(next) => trust_row(id, label, review, next, &waiting),
        None => SurfaceSettingItem {
            value: Some(
                if wanted {
                    "Review and turn on"
                } else {
                    "Review and turn off"
                }
                .into(),
            ),
            description,
            interaction: Some(action(
                review_action_id(&review.target),
                // An unsigned plugin is the owner's own risk: the question warns.
                brings_unsigned(&review.report).then_some(true),
                Some((
                    format!(
                        "{verb} {}?",
                        review.report.title.as_deref().unwrap_or(label)
                    ),
                    review_message(&review.report),
                    verb.into(),
                )),
            )),
            ..item(id, label)
        },
    };
    let mut rows = vec![step];
    rows.extend(
        publishers_of(&review.report)
            .iter()
            .map(|publisher| SurfaceSettingItem {
                value: Some(publisher_state_label(publisher.state).into()),
                description: Some(publisher_description(publisher)),
                ..item(
                    review_publisher_row_id(&review.target, &publisher.namespace),
                    format!("Publisher {}", publisher.namespace),
                )
            }),
    );
    rows
}

fn on_off() -> Vec<SurfaceSettingChoice> {
    vec![
        SurfaceSettingChoice {
            value: "true".into(),
            label: "On".into(),
        },
        SurfaceSettingChoice {
            value: "false".into(),
            label: "Off".into(),
        },
    ]
}

fn toggle_rows(
    plugin_id: &str,
    label: &str,
    enabled: bool,
    facts: &PluginFacts,
    description: Option<String>,
) -> Vec<SurfaceSettingItem> {
    if let (Some(wanted), Some(install)) = (
        facts.needs_review.get(plugin_id),
        install_of(facts, plugin_id),
    ) {
        let target = ReleaseTarget {
            source_url: install.source_url.clone(),
            plugin_id: plugin_id.into(),
            release_version: install.release_version.clone(),
        };
        if let Some(review) = review_for(facts, &target) {
            return review_rows(
                toggle_setting_id(plugin_id),
                label,
                review,
                description,
                *wanted,
            );
        }
        return vec![SurfaceSettingItem {
            value: Some("Needs review".into()),
            description: Some(format!(
                "It changed since you approved it. Check it again to turn it {}.",
                if *wanted { "on" } else { "off" }
            )),
            interaction: Some(action(inspect_action_id(&target), None, None)),
            ..item(toggle_setting_id(plugin_id), label)
        }];
    }
    vec![SurfaceSettingItem {
        value: Some(if enabled { "On" } else { "Off" }.into()),
        description,
        interaction: Some(SurfaceSettingInteraction::Choice {
            choices: on_off(),
            selected: if enabled { "true" } else { "false" }.into(),
        }),
        ..item(toggle_setting_id(plugin_id), label)
    }]
}

fn untrusted_words(publisher: &PluginPublisher) -> Option<String> {
    match publisher.state {
        PluginPublisherState::Bound | PluginPublisherState::Trusted => None,
        PluginPublisherState::New => Some(format!(
            "Publisher {} is not trusted yet.",
            publisher.namespace
        )),
        PluginPublisherState::KeyChanged => Some(format!(
            "The publisher key for {} changed.",
            publisher.namespace
        )),
        PluginPublisherState::Unsigned => {
            Some("Not signed. Korri cannot confirm who made it.".into())
        }
    }
}

fn release_rows(
    target: ReleaseTarget,
    label: &str,
    facts: &PluginFacts,
    several: bool,
    description: Option<String>,
) -> Vec<SurfaceSettingItem> {
    if let Some(review) = review_for(facts, &target) {
        return review_rows(
            release_row_id(&target),
            label,
            review,
            several.then(|| format!("Version {}", target.release_version)),
            true,
        );
    }
    vec![SurfaceSettingItem {
        value: Some(if several {
            format!("Install {}", target.release_version)
        } else {
            "Install".into()
        }),
        description,
        interaction: Some(action(inspect_action_id(&target), None, None)),
        ..item(release_row_id(&target), label)
    }]
}

fn catalog_rows(
    url: &str,
    catalog: Option<&CatalogView>,
    installs: &[PluginRepositoryInstall],
    titles: &BTreeMap<String, String>,
    facts: &PluginFacts,
) -> Vec<SurfaceSettingItem> {
    let from_here: Vec<&PluginRepositoryInstall> = installs
        .iter()
        .filter(|install| install.source_url == url)
        .collect();
    let title_of = |id: &str| titles.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let installed_rows = || -> Vec<SurfaceSettingItem> {
        from_here
            .iter()
            .flat_map(|install| {
                toggle_rows(
                    &install.id,
                    &title_of(&install.id),
                    install.enabled,
                    facts,
                    None,
                )
            })
            .collect()
    };
    let status_id = format!("{}:status", repository_row_id(url));
    let entries = match catalog {
        None | Some(CatalogView::Checking) => {
            let mut rows = vec![item(status_id, "Checking for plugins…")];
            rows.extend(installed_rows());
            return rows;
        }
        Some(CatalogView::Unreachable(message)) => {
            let mut rows = vec![SurfaceSettingItem {
                description: Some(message.clone()),
                ..item(status_id, "Can't reach this repository")
            }];
            rows.extend(installed_rows());
            return rows;
        }
        Some(CatalogView::Ready(entries)) => entries,
    };
    let mut order: Vec<String> = Vec::new();
    let mut releases: BTreeMap<String, Vec<&PluginCatalogEntry>> = BTreeMap::new();
    for entry in entries {
        if !releases.contains_key(&entry.plugin_id) {
            order.push(entry.plugin_id.clone());
        }
        releases
            .entry(entry.plugin_id.clone())
            .or_default()
            .push(entry);
    }
    let mut rows = Vec::new();
    for plugin_id in &order {
        let entries = &releases[plugin_id];
        let first = entries[0];
        let label = first
            .title
            .clone()
            .or_else(|| titles.get(plugin_id).cloned())
            .unwrap_or_else(|| plugin_id.clone());
        let install = from_here
            .iter()
            .find(|candidate| &candidate.id == plugin_id);
        let installed_entry = entries.iter().find(|entry| entry.installed.is_some());
        if install.is_some() || installed_entry.is_some() {
            let enabled = install.map_or(
                installed_entry.and_then(|entry| entry.installed)
                    == Some(PluginInstalledState::Enabled),
                |install| install.enabled,
            );
            rows.extend(toggle_rows(
                plugin_id,
                &label,
                enabled,
                facts,
                first.description.clone(),
            ));
            continue;
        }
        // Install inspects first; trust, when the publisher needs it, comes after.
        let untrusted = untrusted_words(&first.publisher);
        let several = entries.len() > 1;
        for (index, entry) in entries.iter().enumerate() {
            let about = if several {
                Some("Pick the version to install.".to_owned())
            } else {
                first.description.clone()
            };
            let description = if index == 0 {
                match &untrusted {
                    None => about,
                    Some(untrusted) if several => {
                        Some(format!("{untrusted} {}", about.unwrap_or_default()))
                    }
                    Some(untrusted) => Some(untrusted.clone()),
                }
            } else {
                None
            };
            rows.extend(release_rows(
                ReleaseTarget {
                    source_url: url.into(),
                    plugin_id: plugin_id.clone(),
                    release_version: entry.release_version.clone(),
                },
                &label,
                facts,
                several,
                description,
            ));
        }
    }
    // An install the catalog no longer lists still turns on and off.
    for install in &from_here {
        if !releases.contains_key(&install.id) {
            rows.extend(toggle_rows(
                &install.id,
                &title_of(&install.id),
                install.enabled,
                facts,
                None,
            ));
        }
    }
    if rows.is_empty() {
        rows.push(item(
            format!("{}:status", repository_row_id(url)),
            "No plugins for this device",
        ));
    }
    rows
}

fn catalog_status(catalog: Option<&CatalogView>) -> String {
    match catalog {
        None | Some(CatalogView::Checking) => "Checking…".into(),
        Some(CatalogView::Unreachable(_)) => "Can't reach it".into(),
        Some(CatalogView::Ready(entries)) => {
            let mut ids: Vec<&str> = entries
                .iter()
                .map(|entry| entry.plugin_id.as_str())
                .collect();
            ids.sort();
            ids.dedup();
            count(ids.len(), "plugin")
        }
    }
}

fn publisher_row(
    entry: &PluginPublisherEntry,
    title_of: &dyn Fn(&str) -> String,
) -> SurfaceSettingItem {
    let installed = if entry.installed.is_empty() {
        "No plugins installed from it.".to_owned()
    } else {
        format!(
            "{} installed from it.",
            count(entry.installed.len(), "plugin")
        )
    };
    if entry.state == PluginPublisherState::Bound {
        return SurfaceSettingItem {
            value: Some(publisher_state_label(PluginPublisherState::Bound).into()),
            description: Some(format!(
                "This device's configuration trusts it. Key {}. {installed}",
                entry.fingerprint
            )),
            ..item(publisher_row_id(&entry.namespace), entry.namespace.clone())
        };
    }
    let names: Vec<String> = entry.installed.iter().map(|id| title_of(id)).collect();
    let message = if names.is_empty() {
        vec![
            format!("This device stops trusting {}.", entry.namespace),
            format!(
                "To install from {} again, you check and trust its key again.",
                entry.namespace
            ),
        ]
    } else {
        vec![
            format!(
                "Plugins from {} stop starting on this device: {}.",
                entry.namespace,
                names.join(", ")
            ),
            "You can still turn them off and remove them.".into(),
            format!(
                "To use them again, you check and trust the key of {} again.",
                entry.namespace
            ),
        ]
    };
    SurfaceSettingItem {
        value: Some("Revoke".into()),
        description: Some(format!(
            "Trusted on this device. Key {}. {installed}",
            entry.fingerprint
        )),
        interaction: Some(action(
            revoke_action_id(&entry.namespace),
            Some(true),
            Some((
                format!("Revoke {}?", entry.namespace),
                message.join("\n"),
                "Revoke".into(),
            )),
        )),
        ..item(publisher_row_id(&entry.namespace), entry.namespace.clone())
    }
}

pub const PLUGINS_GROUP: &str = "Plugins";
pub const OFFICIAL_GROUP: &str = "Official plugins";
pub const REMOVED_GROUP: &str = "Installed from removed repositories";
pub const OTHER_GROUP: &str = "Other installed plugins";
pub const PUBLISHERS_GROUP: &str = "Publishers";

fn publishers_group(
    facts: &PluginFacts,
    title_of: &dyn Fn(&str) -> String,
) -> Option<SurfaceSettingGroup> {
    let items = match &facts.publishers {
        Read::Loading => return None,
        Read::Failed(message) => vec![SurfaceSettingItem {
            description: Some(message.clone()),
            ..item(PUBLISHERS_STATUS_ROW, "Can't read the publishers")
        }],
        Read::Ready(value) if value.entries.is_empty() => vec![SurfaceSettingItem {
            description: Some(
                "Korri asks you to trust a publisher when you install its first plugin.".into(),
            ),
            ..item(PUBLISHERS_STATUS_ROW, "No trusted publishers")
        }],
        Read::Ready(value) => value
            .entries
            .iter()
            .map(|entry| publisher_row(entry, title_of))
            .collect(),
    };
    Some(SurfaceSettingGroup {
        title: PUBLISHERS_GROUP.into(),
        items,
    })
}

/// The row where a person enters or clears the plugin's secret. The surface
/// gets the status only, never the value.
fn credential_row(plugin: &PluginSetting, editable: bool) -> Option<SurfaceSettingItem> {
    let configured = plugin.credential? == crate::config::settings::SecretSettingStatus::Configured;
    Some(SurfaceSettingItem {
        value: Some(
            if configured {
                "Configured"
            } else {
                "Not configured"
            }
            .into(),
        ),
        description: Some(
            "Korri keeps it on this device and sends it only to the host you approved.".into(),
        ),
        interaction: editable.then(|| SurfaceSettingInteraction::SensitiveText {
            placeholder: Some("Paste API key".into()),
            max_length: Some(256),
            clear_label: configured.then(|| "Clear saved key".into()),
        }),
        ..item(
            credential_setting_id(&plugin.id),
            format!("{} API key", plugin.title),
        )
    })
}

/// Settings groups for plugins: Plugins, Official plugins, each added
/// repository, removed repositories' installs, other installs, Publishers.
/// `installed` is `SettingsSnapshot.plugins`: titles, and for a plugin that
/// declares a secret, its secret row after the plugin's own rows. `editable`
/// is `SettingsSnapshot.editableSettingIds`.
pub fn plugin_setting_groups(
    facts: &PluginFacts,
    installed: Option<&[PluginSetting]>,
    editable: &[String],
) -> Vec<SurfaceSettingGroup> {
    let mut groups = plugin_groups(facts, installed);
    for plugin in installed.unwrap_or(&[]) {
        let id = credential_setting_id(&plugin.id);
        let Some(row) = credential_row(plugin, editable.contains(&id)) else {
            continue;
        };
        let toggle = toggle_setting_id(&plugin.id);
        // After the last row of this plugin: its toggle, or its review rows.
        let place = groups.iter_mut().find_map(|group| {
            let first = group.items.iter().position(|item| item.id == toggle)?;
            let rows = 1 + group.items[first + 1..]
                .iter()
                .take_while(|item| item.id.starts_with(REVIEW_PUBLISHER_ROW))
                .count();
            Some((group, first + rows))
        });
        match place {
            Some((group, at)) => group.items.insert(at, row),
            // The plugin's own rows cannot be listed now (the repository
            // list is still being read, or cannot be read). Its secret does
            // not depend on that list: the row ends the first group.
            None => {
                if let Some(group) = groups.first_mut() {
                    group.items.push(row);
                }
            }
        }
    }
    groups
}

fn plugin_groups(
    facts: &PluginFacts,
    installed: Option<&[PluginSetting]>,
) -> Vec<SurfaceSettingGroup> {
    let titles: BTreeMap<String, String> = installed
        .unwrap_or(&[])
        .iter()
        .map(|plugin| (plugin.id.clone(), plugin.title.clone()))
        .collect();
    let mut known = titles.clone();
    if let Read::Ready(value) = &facts.repositories {
        for plugin in &value.other_installs {
            if let Some(title) = plugin.title.as_ref().filter(|title| !title.is_empty()) {
                known.insert(plugin.id.clone(), title.clone());
            }
        }
    }
    for catalog in facts.catalogs.values() {
        if let CatalogView::Ready(entries) = catalog {
            for entry in entries {
                if let Some(title) = entry.title.as_ref().filter(|title| !title.is_empty()) {
                    known.insert(entry.plugin_id.clone(), title.clone());
                }
            }
        }
    }
    let title_of = |id: &str| known.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let publishers = publishers_group(facts, &title_of);
    let add_row = SurfaceSettingItem {
        description: Some("Type the https:// address the plugin maker gave you.".into()),
        interaction: Some(SurfaceSettingInteraction::Text {
            placeholder: Some("https://…".into()),
            max_length: Some(512),
            allow_empty: None,
        }),
        ..item(ADD_REPOSITORY_SETTING, "Add a repository")
    };
    let value = match &facts.repositories {
        Read::Ready(value) => value,
        other => {
            // The official row keeps its place and says where the list stands.
            let hub = SurfaceSettingGroup {
                title: PLUGINS_GROUP.into(),
                items: vec![
                    SurfaceSettingItem {
                        value: Some(
                            if matches!(other, Read::Loading) {
                                "Checking…"
                            } else {
                                "Can't read the list"
                            }
                            .into(),
                        ),
                        description: match other {
                            Read::Failed(message) => Some(message.clone()),
                            _ => None,
                        },
                        ..item(OFFICIAL_REPOSITORY_ROW, "Official plugins")
                    },
                    add_row,
                ],
            };
            return std::iter::once(hub).chain(publishers).collect();
        }
    };
    let mut hub_items = vec![match &value.official {
        None => SurfaceSettingItem {
            value: Some("Not set up yet".into()),
            description: Some("Korri's own plugins will appear here.".into()),
            ..item(OFFICIAL_REPOSITORY_ROW, "Official plugins")
        },
        Some(official) => SurfaceSettingItem {
            value: Some(catalog_status(facts.catalogs.get(official))),
            description: Some("Always here. It can't be removed.".into()),
            ..item(OFFICIAL_REPOSITORY_ROW, "Official plugins")
        },
    }];
    hub_items.extend(value.user_added.iter().map(|url| SurfaceSettingItem {
        value: Some(catalog_status(facts.catalogs.get(url))),
        ..item(repository_row_id(url), repository_name(url))
    }));
    hub_items.push(add_row);
    let mut groups = vec![SurfaceSettingGroup {
        title: PLUGINS_GROUP.into(),
        items: hub_items,
    }];
    if let Some(official) = &value.official {
        groups.push(SurfaceSettingGroup {
            title: OFFICIAL_GROUP.into(),
            items: catalog_rows(
                official,
                facts.catalogs.get(official),
                &value.repository_installs,
                &titles,
                facts,
            ),
        });
    }
    for url in &value.user_added {
        let from_here = value
            .repository_installs
            .iter()
            .filter(|install| &install.source_url == url)
            .count();
        let name = repository_name(url);
        let message = match from_here {
            0 => format!("Korri stops showing plugins from {name}. You can add it again later."),
            1 => format!("Korri stops showing plugins from {name}. The plugin you installed from it stays, and still turns on and off under \"{REMOVED_GROUP}\"."),
            n => format!("Korri stops showing plugins from {name}. The {n} plugins you installed from it stay, and still turn on and off under \"{REMOVED_GROUP}\"."),
        };
        let mut items = catalog_rows(
            url,
            facts.catalogs.get(url),
            &value.repository_installs,
            &titles,
            facts,
        );
        items.push(SurfaceSettingItem {
            interaction: Some(action(
                remove_repository_action_id(url),
                Some(true),
                Some(("Remove this repository?".into(), message, "Remove".into())),
            )),
            ..item(remove_repository_action_id(url), "Remove this repository")
        });
        groups.push(SurfaceSettingGroup { title: name, items });
    }
    let configured = configured_repositories(value);
    let removed: Vec<&PluginRepositoryInstall> = value
        .repository_installs
        .iter()
        .filter(|install| !configured.contains(&install.source_url))
        .collect();
    if !removed.is_empty() {
        groups.push(SurfaceSettingGroup {
            title: REMOVED_GROUP.into(),
            items: removed
                .iter()
                .flat_map(|install| {
                    toggle_rows(
                        &install.id,
                        &titles
                            .get(&install.id)
                            .cloned()
                            .unwrap_or_else(|| install.id.clone()),
                        install.enabled,
                        facts,
                        Some(format!("From {}", repository_name(&install.source_url))),
                    )
                })
                .collect(),
        });
    }
    if !value.other_installs.is_empty() {
        groups.push(SurfaceSettingGroup {
            title: OTHER_GROUP.into(),
            items: value
                .other_installs
                .iter()
                .flat_map(|plugin| {
                    toggle_rows(
                        &plugin.id,
                        plugin.title.as_deref().unwrap_or(&plugin.id),
                        plugin.enabled,
                        facts,
                        None,
                    )
                })
                .collect(),
        });
    }
    groups.extend(publishers);
    groups
}

/// JavaScript `localeCompare(..., { numeric: true })` for version labels.
pub fn natural_compare(left: &str, right: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (left.chars().peekable(), right.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(digit) = a.peek().copied().filter(char::is_ascii_digit) {
                    na.push(digit);
                    a.next();
                }
                let mut nb = String::new();
                while let Some(digit) = b.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(digit);
                    b.next();
                }
                let (na, nb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ordering = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
            (Some(x), Some(y)) => {
                let ordering = x.to_lowercase().cmp(y.to_lowercase());
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
                a.next();
                b.next();
            }
        }
    }
}

// ---------------------------------------------------------------- controller

/// What the controller reads and changes: its facts, the settings status,
/// the shared settings lock, and who re-reads after a change.
pub struct PluginsHost {
    pub facts: Box<dyn Fn() -> PluginFacts + Send + Sync>,
    pub publish: Box<dyn Fn(PluginFacts) + Send + Sync>,
    pub status: Box<dyn Fn(SurfaceSettingsStatus) + Send + Sync>,
    pub busy_get: Box<dyn Fn() -> bool + Send + Sync>,
    pub busy_set: Box<dyn Fn(bool) + Send + Sync>,
    pub changed: Box<dyn Fn() + Send + Sync>,
}

pub struct PluginsController {
    korrid: Korrid,
    host: PluginsHost,
    read_seq: Mutex<u64>,
}

type Done<T> =
    Box<dyn FnOnce(Arc<PluginsController>, T) -> futures::future::BoxFuture<'static, ()> + Send>;
type Refused = Box<dyn FnOnce(Arc<PluginsController>, &RpcFailure) -> bool + Send>;

impl PluginsController {
    pub fn new(korrid: Korrid, host: PluginsHost) -> Arc<Self> {
        Arc::new(Self {
            korrid,
            host,
            read_seq: Mutex::new(0),
        })
    }

    fn update(&self, change: impl FnOnce(PluginFacts) -> PluginFacts) {
        (self.host.publish)(change((self.host.facts)()));
    }

    fn problem(&self, setting_id: &str, message: &str) {
        (self.host.status)(SurfaceSettingsStatus::Problem {
            setting_id: setting_id.into(),
            message: message.into(),
        });
    }

    fn idle(&self) {
        (self.host.status)(SurfaceSettingsStatus::Idle);
    }

    /// Start one locked operation on `setting_id`; false when another runs.
    fn begin(&self, setting_id: &str) -> bool {
        if (self.host.busy_get)() {
            return false;
        }
        (self.host.busy_set)(true);
        (self.host.status)(SurfaceSettingsStatus::Saving {
            setting_id: setting_id.into(),
        });
        true
    }

    /// One locked operation: the row saves while `call` runs.
    fn locked<T: Send + 'static>(
        self: &Arc<Self>,
        setting_id: String,
        call: futures::future::BoxFuture<'static, Result<T, RpcFailure>>,
        done: Done<T>,
        refused: Option<Refused>,
    ) {
        if !self.begin(&setting_id) {
            return;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = call.await;
            (this.host.busy_set)(false);
            match outcome {
                Err(failure) => {
                    let handled =
                        refused.is_some_and(|refused| refused(Arc::clone(&this), &failure));
                    if !handled {
                        this.problem(&setting_id, &failure.message);
                    }
                }
                Ok(payload) => done(Arc::clone(&this), payload).await,
            }
        });
    }

    /// Read repositories, the publishers, then every configured catalog.
    pub async fn refresh(&self) {
        let seq = {
            let mut seq = self.read_seq.lock().expect("plugins poisoned");
            *seq += 1;
            *seq
        };
        let current = || *self.read_seq.lock().expect("plugins poisoned") == seq;
        let outcome = ask!(
            self.korrid,
            Route,
            PluginRepositories,
            PluginRepositoriesRequest {},
            PluginRepositoriesOutcome
        );
        if !current() {
            return;
        }
        let repositories = match &outcome {
            PluginRepositoriesOutcome::Ok(value) => Some(value.clone()),
            PluginRepositoriesOutcome::Err(_) => None,
        };
        self.update(|facts| with_repositories(facts, outcome));
        let Some(repositories) = repositories else {
            return;
        };
        let publishers = ask!(
            self.korrid,
            Route,
            PluginPublishers,
            PluginPublishersRequest {},
            PluginPublishersOutcome
        );
        if !current() {
            return;
        }
        self.update(|facts| with_publishers(facts, publishers));
        let urls = configured_repositories(&repositories);
        self.update(|facts| checking_catalogs(facts, &urls));
        // One at a time: korrid serves plugin requests in turn.
        for url in urls {
            if !current() {
                return;
            }
            let outcome = ask!(
                self.korrid,
                Route,
                PluginCatalog,
                PluginRepositoryRequest { url: url.clone() },
                PluginCatalogOutcome
            );
            if !current() {
                return;
            }
            self.update(|facts| with_catalog(facts, &url, outcome));
        }
    }

    fn add_repository(self: &Arc<Self>, typed: &str) {
        let Some(url) = https_address(typed) else {
            if !(self.host.busy_get)() {
                self.problem(ADD_REPOSITORY_SETTING, INVALID_ADDRESS);
            }
            return;
        };
        let korrid = self.korrid.clone();
        self.locked(
            ADD_REPOSITORY_SETTING.into(),
            Box::pin(async move {
                match ask!(
                    korrid,
                    Route,
                    PluginRepositoryAdd,
                    PluginRepositoryRequest { url },
                    PluginRepositoryAddOutcome
                ) {
                    PluginRepositoryAddOutcome::Ok(result) => Ok(result),
                    PluginRepositoryAddOutcome::Err(failure) => Err(failure),
                }
            }),
            Box::new(|this, result| {
                Box::pin(async move {
                    if result == PluginRepositoryAddResult::AlreadyPresent {
                        this.problem(ADD_REPOSITORY_SETTING, ALREADY_ADDED);
                        return;
                    }
                    this.idle();
                    this.refresh().await;
                })
            }),
            None,
        );
    }

    fn remove_repository(self: &Arc<Self>, url: String) {
        let korrid = self.korrid.clone();
        let removed = url.clone();
        // NotFound: it is already gone, which is what the person asked for.
        self.locked(
            remove_repository_action_id(&url),
            Box::pin(async move {
                match ask!(
                    korrid,
                    Route,
                    PluginRepositoryRemove,
                    PluginRepositoryRequest { url },
                    PluginRepositoryRemoveOutcome
                ) {
                    PluginRepositoryRemoveOutcome::Ok(_) => Ok(()),
                    PluginRepositoryRemoveOutcome::Err(failure) => Err(failure),
                }
            }),
            Box::new(move |this, ()| {
                Box::pin(async move {
                    this.idle();
                    this.update(|facts| without_repository(facts, &removed));
                    this.refresh().await;
                })
            }),
            None,
        );
    }

    async fn inspect_now(
        korrid: Korrid,
        target: ReleaseTarget,
    ) -> Result<PluginInspection, RpcFailure> {
        match ask!(
            korrid,
            Route,
            PluginInspect,
            PluginInspectRequest {
                url: target.source_url,
                plugin_id: target.plugin_id,
                release_version: target.release_version
            },
            PluginInspectOutcome
        ) {
            PluginInspectOutcome::Ok(report) => Ok(report),
            PluginInspectOutcome::Err(failure) => Err(failure),
        }
    }

    async fn set_enabled(korrid: &Korrid, id: String, enabled: bool) -> Result<(), RpcFailure> {
        match ask!(
            korrid,
            Route,
            PluginSetEnabled,
            PluginSetEnabledRequest { id, enabled },
            PluginChangeOutcome
        ) {
            PluginChangeOutcome::Ok(_) => Ok(()),
            PluginChangeOutcome::Err(failure) => Err(failure),
        }
    }

    fn inspect(self: &Arc<Self>, target: ReleaseTarget) {
        // korrid downloads and verifies the release before it reports.
        let row = row_id_for_release(&(self.host.facts)(), &target);
        let reviewed = target.clone();
        self.locked(
            row,
            Box::pin(Self::inspect_now(self.korrid.clone(), target)),
            Box::new(move |this, report| {
                Box::pin(async move {
                    this.update(|facts| with_review(facts, &reviewed, report));
                    this.idle();
                })
            }),
            None,
        );
    }

    /// Install exactly the reviewed release, then set the state the person
    /// asked for.
    fn approve(self: &Arc<Self>, target: ReleaseTarget) {
        let facts = (self.host.facts)();
        // Nothing was inspected for this release, so nothing was shown.
        let Some(review) = review_for(&facts, &target).cloned() else {
            return;
        };
        let wanted = wanted_after_review(&facts, &review.target.plugin_id);
        let installed = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&installed);
        let korrid = self.korrid.clone();
        let this = Arc::clone(self);
        let plugin_id = review.target.plugin_id.clone();
        self.locked(
            row_id_for_release(&facts, &target),
            Box::pin(async move {
                match ask!(
                    korrid,
                    Route,
                    PluginInstall,
                    PluginInstallRequest {
                        url: review.target.source_url.clone(),
                        plugin_id: review.target.plugin_id.clone(),
                        release_version: review.target.release_version.clone(),
                        approval: review.report.approval.clone(),
                    },
                    PluginChangeOutcome
                ) {
                    PluginChangeOutcome::Err(failure) => return Err(failure),
                    PluginChangeOutcome::Ok(_) => {}
                }
                *flag.lock().expect("flag poisoned") = true;
                this.update(|facts| without_review(facts, &target));
                Self::set_enabled(&korrid, review.target.plugin_id.clone(), wanted).await
            }),
            Box::new(|this, ()| {
                Box::pin(async move {
                    this.idle();
                    (this.host.changed)();
                })
            }),
            Some(Box::new(move |this, failure| {
                if !*installed.lock().expect("flag poisoned") {
                    return false;
                }
                // Installed, but not switched: the plugin's own row says why.
                this.problem(&toggle_setting_id(&plugin_id), &failure.message);
                (this.host.changed)();
                true
            })),
        );
    }

    /// Trust one publisher the kept report named, with exactly the key it
    /// reported, then check the release again.
    fn trust(self: &Arc<Self>, target: ReleaseTarget, namespace: String) {
        let facts = (self.host.facts)();
        let Some(review) = review_for(&facts, &target) else {
            return;
        };
        // Only a key the person was shown can be trusted.
        let Some(public_key) = publishers_to_trust(&review.report)
            .into_iter()
            .find(|candidate| candidate.namespace == namespace)
            .and_then(|publisher| publisher.public_key)
        else {
            return;
        };
        let row = row_id_for_release(&facts, &target);
        let trusted = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&trusted);
        let korrid = self.korrid.clone();
        let checked = target.clone();
        let dropped = target.clone();
        let refused_row = row.clone();
        self.locked(
            row,
            Box::pin(async move {
                match ask!(
                    korrid,
                    Route,
                    PluginTrustPublisher,
                    PluginTrustPublisherRequest {
                        namespace,
                        public_key
                    },
                    PluginChangeOutcome
                ) {
                    PluginChangeOutcome::Err(failure) => return Err(failure),
                    PluginChangeOutcome::Ok(_) => {}
                }
                *flag.lock().expect("flag poisoned") = true;
                Self::inspect_now(korrid, target).await
            }),
            Box::new(move |this, report| {
                Box::pin(async move {
                    this.update(|facts| with_review(facts, &checked, report));
                    this.idle();
                    this.refresh().await;
                })
            }),
            Some(Box::new(move |this, failure| {
                if !*trusted.lock().expect("flag poisoned") {
                    return false;
                }
                this.update(|facts| without_report(facts, &dropped));
                this.problem(&refused_row, &failure.message);
                let again = Arc::clone(&this);
                tokio::spawn(async move { again.refresh().await });
                true
            })),
        );
    }

    fn revoke(self: &Arc<Self>, namespace: String) {
        let korrid = self.korrid.clone();
        self.locked(
            publisher_row_id(&namespace),
            Box::pin(async move {
                match ask!(
                    korrid,
                    Route,
                    PluginRevokePublisher,
                    PluginRevokePublisherRequest { namespace },
                    PluginChangeOutcome
                ) {
                    PluginChangeOutcome::Ok(_) => Ok(()),
                    PluginChangeOutcome::Err(failure) => Err(failure),
                }
            }),
            Box::new(|this, ()| {
                Box::pin(async move {
                    this.idle();
                    (this.host.changed)();
                })
            }),
            None,
        );
    }

    fn toggle(self: &Arc<Self>, plugin_id: String, enabled: bool) {
        let korrid = self.korrid.clone();
        let id = plugin_id.clone();
        self.locked(
            toggle_setting_id(&plugin_id),
            Box::pin(async move { Self::set_enabled(&korrid, id, enabled).await }),
            Box::new(|this, ()| {
                Box::pin(async move {
                    this.idle();
                    (this.host.changed)();
                })
            }),
            Some(Box::new(move |this, failure| {
                if failure.code != NEEDS_REVIEW_CODE
                    || install_of(&(this.host.facts)(), &plugin_id).is_none()
                {
                    return false;
                }
                // The row turns into Needs review; its next press inspects again.
                this.update(|facts| with_needs_review(facts, &plugin_id, enabled));
                this.idle();
                true
            })),
        );
    }

    /// A setting-local action. False when the id is not a plugin action.
    pub fn run(self: &Arc<Self>, action_id: &str) -> bool {
        match plugin_action_from(action_id) {
            None => false,
            Some(action) => {
                match action {
                    PluginAction::Remove(url) => self.remove_repository(url),
                    PluginAction::Inspect(target) => self.inspect(target),
                    PluginAction::Review(target) => self.approve(target),
                    PluginAction::Trust(target, namespace) => self.trust(target, namespace),
                    PluginAction::Revoke(namespace) => self.revoke(namespace),
                }
                true
            }
        }
    }

    /// A setting change. False when the id is not a plugin setting.
    pub fn change(self: &Arc<Self>, setting_id: &str, value: &str) -> bool {
        if !is_plugin_setting_id(setting_id) {
            return false;
        }
        // A plugin row with a value it never offered: nothing to do.
        match plugin_change_from(setting_id, value) {
            None => {}
            Some(PluginChange::AddRepository(url)) => self.add_repository(&url),
            Some(PluginChange::Toggle(plugin_id, enabled)) => self.toggle(plugin_id, enabled),
        }
        true
    }
}

/// The release of `plugin_id` to install: the newest version any configured
/// repository offers from a trusted publisher.
pub fn newest_release(facts: &PluginFacts, plugin_id: &str) -> Option<ReleaseTarget> {
    let mut offers: Vec<ReleaseTarget> = Vec::new();
    for (source_url, catalog) in &facts.catalogs {
        if let CatalogView::Ready(entries) = catalog {
            for entry in entries {
                if entry.plugin_id == plugin_id && is_trusted_publisher(&entry.publisher) {
                    offers.push(ReleaseTarget {
                        source_url: source_url.clone(),
                        plugin_id: plugin_id.into(),
                        release_version: entry.release_version.clone(),
                    });
                }
            }
        }
    }
    offers.sort_by(|a, b| natural_compare(&b.release_version, &a.release_version));
    offers.into_iter().next()
}
