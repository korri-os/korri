//! `korri-plugin serve-admin`: the root side of korrid's plugin admin socket.
//!
//! systemd owns the socket (root, korrid's group, 0660) and passes it in.
//! This server also checks the peer with SO_PEERCRED and serves only korrid's
//! uid. Each accepted request maps 1:1 to the `Host` method that the CLI
//! command named on its `PluginAdminRequest` variant already calls. The host
//! keeps its lock, catalog validation, archive hash, publisher signature and
//! approval-digest checks. Raw-cache install, update, switch, remove, purge,
//! restore and enabled-packages are not reachable from here.
pub use crate::plugin_admin::*;
use crate::{
    host::{Host, APPROVAL_STALE},
    package,
    package::Report,
    provenance::{current_platform, Provenance},
    repository::{Configuration, SourceUrl},
    selection::Desired,
    source_store::{AddOutcome, RemoveOutcome},
};
use std::{
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    time::Duration,
};

/// A connection must deliver its whole request within this time. It bounds
/// only the read; the host's own transfer bounds cover the work.
pub const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The kernel's record of the connecting process's uid.
pub fn peer_uid(stream: &UnixStream) -> Result<u32, String> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result != 0 || length as usize != std::mem::size_of::<libc::ucred>() {
        return Err("cannot read admin socket peer credentials".into());
    }
    Ok(credentials.uid)
}

/// Serve one connection. A peer other than `expected_uid` gets no response
/// and its request is never read or parsed. A request that does not parse as
/// one of the known requests is refused without calling `handle`.
pub fn serve_connection(
    mut stream: UnixStream,
    expected_uid: u32,
    handle: impl FnOnce(PluginAdminRequest) -> PluginAdminResponse,
) -> Result<(), String> {
    if peer_uid(&stream)? != expected_uid {
        return Err("admin socket peer is not the korrid account".into());
    }
    stream
        .set_read_timeout(Some(REQUEST_READ_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    (&mut stream)
        .take(MAX_MESSAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read admin request: {e}"))?;
    let response = if bytes.len() > MAX_MESSAGE_BYTES {
        refused(CODE_REFUSED, "admin request is too large".into())
    } else {
        match serde_json::from_slice::<PluginAdminRequest>(&bytes) {
            Ok(request) => handle(request),
            Err(_) => refused(CODE_REFUSED, "unknown or invalid admin request".into()),
        }
    };
    let bytes = serde_json::to_vec(&response).map_err(|e| e.to_string())?;
    let bytes = if bytes.len() > MAX_MESSAGE_BYTES {
        serde_json::to_vec(&refused(
            CODE_REFUSED,
            "admin response exceeds its size limit".into(),
        ))
        .map_err(|e| e.to_string())?
    } else {
        bytes
    };
    stream.write_all(&bytes).map_err(|e| e.to_string())
}

fn failure(code: &str, message: String) -> PluginAdminFailure {
    PluginAdminFailure {
        code: code.into(),
        message,
    }
}

fn refused(code: &str, message: String) -> PluginAdminResponse {
    PluginAdminResponse::Refused(failure(code, message))
}

fn source(raw: &str) -> Result<SourceUrl, PluginAdminFailure> {
    SourceUrl::parse(raw).map_err(|error| failure(CODE_URL_INVALID, error))
}

fn host_failure(error: String) -> PluginAdminFailure {
    failure(
        if error.contains("key changed: was") {
            CODE_PUBLISHER_KEY_CHANGED
        } else if error.contains("is not trusted on this device") {
            CODE_PUBLISHER_NOT_TRUSTED
        } else if error.contains("namespace") && error.contains("unsigned")
            || error.contains("remove the unsigned")
        {
            CODE_NAMESPACE_TAKEN
        } else if (error.contains(" needs ") && error.contains(" locked "))
            || error.contains("conflicting repository requirements")
        {
            CODE_REQUIREMENT_CONFLICT
        } else {
            CODE_REFUSED
        },
        error,
    )
}

pub fn host_refused(error: String) -> PluginAdminResponse {
    refused(CODE_REFUSED, error)
}

/// Run one request through the existing host boundary.
pub fn handle(
    host: &Host,
    config: &Configuration,
    request: PluginAdminRequest,
) -> PluginAdminResponse {
    let result = match request {
        PluginAdminRequest::Publishers(_) => host
            .publishers()
            .map(PluginAdminResponse::Publishers)
            .map_err(host_failure),
        PluginAdminRequest::TrustPublisher(request) => host
            .trust_publisher(&request.namespace, &request.public_key)
            .map(|()| PluginAdminResponse::Applied(PluginChangeApplied {}))
            .map_err(host_failure),
        PluginAdminRequest::RevokePublisher(request) => host
            .revoke_publisher(&request.namespace)
            .map(|()| PluginAdminResponse::Applied(PluginChangeApplied {}))
            .map_err(host_failure),
        PluginAdminRequest::InspectArchive(request) => host
            .inspect_archive(config, std::path::Path::new(&request.path))
            .and_then(|report| {
                host.release_download()?;
                inspection(host, &report)
            })
            .map(PluginAdminResponse::Inspection)
            .map_err(host_failure),
        PluginAdminRequest::InstallArchive(request) => host
            .inspect_archive(config, std::path::Path::new(&request.path))
            .and_then(|report| host.install_reviewed(report, &request.approval))
            .map(|()| PluginAdminResponse::Applied(PluginChangeApplied {}))
            .map_err(host_failure),
        PluginAdminRequest::Repositories(PluginRepositoriesRequest {}) => {
            repositories(host, config).map_err(host_failure)
        }
        PluginAdminRequest::RepositoryAdd(PluginRepositoryRequest { url }) => source(&url)
            .and_then(|url| host.add_source(config, &url).map_err(host_failure))
            .map(|outcome| {
                PluginAdminResponse::RepositoryAdded(match outcome {
                    AddOutcome::Added => PluginRepositoryAddResult::Added,
                    AddOutcome::AlreadyPresent => PluginRepositoryAddResult::AlreadyPresent,
                })
            }),
        PluginAdminRequest::RepositoryRemove(PluginRepositoryRequest { url }) => source(&url)
            .and_then(|url| host.remove_source(config, &url).map_err(host_failure))
            .map(|outcome| {
                PluginAdminResponse::RepositoryRemoved(match outcome {
                    RemoveOutcome::Removed => PluginRepositoryRemoveResult::Removed,
                    RemoveOutcome::NotFound => PluginRepositoryRemoveResult::NotFound,
                })
            }),
        PluginAdminRequest::Catalog(PluginRepositoryRequest { url }) => {
            source(&url).and_then(|url| catalog(host, config, &url).map_err(host_failure))
        }
        PluginAdminRequest::Inspect(request) => source(&request.url).and_then(|url| {
            let report = host
                .inspect_repository(config, &url, &request.plugin_id, &request.release_version)
                .map_err(host_failure)?;
            host.release_download().map_err(host_failure)?;
            Ok(PluginAdminResponse::Inspection(
                inspection(host, &report).map_err(host_failure)?,
            ))
        }),
        PluginAdminRequest::Install(request) => source(&request.url).and_then(|url| {
            let report = host
                .inspect_repository(config, &url, &request.plugin_id, &request.release_version)
                .map_err(host_failure)?;
            host.install_reviewed(report, &request.approval)
                .map_err(host_failure)?;
            Ok(PluginAdminResponse::Applied(PluginChangeApplied {}))
        }),
        PluginAdminRequest::SetEnabled(PluginSetEnabledRequest { id, enabled }) => host
            .set_enabled(&id, enabled)
            .map(|()| PluginAdminResponse::Applied(PluginChangeApplied {}))
            .map_err(|error| {
                if error == APPROVAL_STALE {
                    failure(CODE_NEEDS_REVIEW, error)
                } else {
                    host_failure(error)
                }
            }),
    };
    result.unwrap_or_else(PluginAdminResponse::Refused)
}

/// A receipt's on/off state; a receipt whose removal is unfinished has none.
fn installed_state(desired: &Desired) -> Option<PluginInstalledState> {
    match desired {
        Desired::Enabled => Some(PluginInstalledState::Enabled),
        Desired::Disabled => Some(PluginInstalledState::Disabled),
        Desired::Removed { .. } => None,
    }
}

fn repositories(host: &Host, config: &Configuration) -> Result<PluginAdminResponse, String> {
    let user_added = host
        .sources()?
        .into_iter()
        .filter(|source| config.official.as_ref() != Some(source))
        .map(|source| source.to_string())
        .collect();
    let mut repository_installs = Vec::new();
    let mut other_installs = Vec::new();
    for receipt in host.installed()? {
        let Some(state) = installed_state(&receipt.desired) else {
            continue;
        };
        let enabled = state == PluginInstalledState::Enabled;
        match receipt.provenance {
            Provenance::Repository {
                source_url,
                release_version,
                ..
            } => repository_installs.push(PluginRepositoryInstall {
                id: receipt.id,
                source_url,
                release_version,
                enabled,
            }),
            // Image-seeded and raw-cache installs. The title is the
            // installed declaration's; a failed read omits it.
            Provenance::RawCache { .. } | Provenance::Sideload { .. } => {
                other_installs.push(PluginOtherInstall {
                    title: package::load_declaration(&receipt.package)
                        .ok()
                        .and_then(|declaration| declaration.title),
                    id: receipt.id,
                    enabled,
                })
            }
        }
    }
    Ok(PluginAdminResponse::Repositories(PluginRepositories {
        official: config.official.as_ref().map(ToString::to_string),
        user_added,
        repository_installs,
        other_installs,
    }))
}

fn catalog(
    host: &Host,
    config: &Configuration,
    source: &SourceUrl,
) -> Result<PluginAdminResponse, String> {
    let catalog = host.catalog(config, source)?;
    let mut installed = std::collections::BTreeMap::new();
    let mut entries = Vec::new();
    for record in catalog
        .records
        .iter()
        .filter(|record| record.platform == current_platform() || record.platform == "any")
    {
        if !installed.contains_key(&record.plugin_id) {
            let state = host
                .status(&record.plugin_id)?
                .and_then(|receipt| installed_state(&receipt.desired));
            installed.insert(record.plugin_id.clone(), state);
        }
        // Catalog validation already proved the "@namespace:name" form.
        let namespace = record
            .plugin_id
            .split_once(':')
            .map_or("", |(namespace, _)| namespace);
        entries.push(PluginCatalogEntry {
            plugin_id: record.plugin_id.clone(),
            title: record.title.clone(),
            description: record.description.clone(),
            release_version: record.release_version.clone(),
            installed: installed[&record.plugin_id],
            publisher: host.catalog_publisher(namespace)?,
        });
    }
    Ok(PluginAdminResponse::Catalog(PluginCatalog { entries }))
}

pub fn inspection(host: &Host, report: &Report) -> Result<PluginInspection, String> {
    Ok(PluginInspection {
        publisher: report.publisher.clone().unwrap_or(
            host.catalog_publisher(report.id.split_once(':').ok_or("invalid plugin ID")?.0)?,
        ),
        id: report.id.clone(),
        title: report.declaration.title.clone(),
        approval: host.review_approval(report)?,
        warning: report.warning.clone(),
        ports: PluginPorts {
            allowed_tcp_ports: report.ports.allowed_tcp_ports.clone(),
            allowed_udp_ports: report.ports.allowed_udp_ports.clone(),
        },
        native_units: report.native_units.keys().cloned().collect(),
        brings: report
            .brings
            .iter()
            .map(|dependency| PluginDependency {
                publisher: dependency.report.publisher.clone().unwrap_or_else(|| {
                    crate::trust::publisher(
                        dependency.report.id.split_once(':').unwrap().0,
                        PluginPublisherState::New,
                        None,
                    )
                    .unwrap()
                }),
                id: dependency.report.id.clone(),
                title: dependency.report.declaration.title.clone(),
                warning: dependency.report.warning.clone(),
                already_approved: dependency.already_approved,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(
        expected_uid: u32,
        request: &[u8],
        handle: impl FnOnce(PluginAdminRequest) -> PluginAdminResponse + Send + 'static,
    ) -> (Result<(), String>, Vec<u8>) {
        let (server, mut client) = UnixStream::pair().unwrap();
        client.write_all(request).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let served = std::thread::spawn(move || serve_connection(server, expected_uid, handle))
            .join()
            .unwrap();
        let mut response = Vec::new();
        // A refused peer sees the connection close, possibly as a reset.
        if client.read_to_end(&mut response).is_err() {
            response.clear();
        }
        (served, response)
    }

    fn applied(_: PluginAdminRequest) -> PluginAdminResponse {
        PluginAdminResponse::Applied(PluginChangeApplied {})
    }

    #[test]
    fn a_peer_with_another_uid_is_refused_before_its_request_is_read() {
        let own = unsafe { libc::geteuid() };
        let request = br#"{"_tag":"SetEnabled","payload":{"id":"@test:a","enabled":true}}"#;
        let (served, response) = exchange(own.wrapping_add(1), request, |_| {
            panic!("a refused peer must never reach the host")
        });
        assert!(served.unwrap_err().contains("not the korrid account"));
        assert!(response.is_empty());
        let (served, response) = exchange(own, request, applied);
        served.unwrap();
        assert_eq!(
            serde_json::from_slice::<PluginAdminResponse>(&response).unwrap(),
            PluginAdminResponse::Applied(PluginChangeApplied {})
        );
    }

    #[test]
    fn unknown_and_unexposed_requests_are_refused_without_reaching_the_host() {
        let own = unsafe { libc::geteuid() };
        for request in [
            br#"{"_tag":"Remove","payload":{"id":"@test:a"}}"#.as_slice(),
            br#"{"_tag":"Update","payload":{"id":"@test:a"}}"#,
            br#"{"_tag":"RawInstall","payload":{"cache":"file:///c","package":"/nix/store/x"}}"#,
            br#"{"_tag":"EnabledPackages","payload":{}}"#,
            br#"{"_tag":"SetEnabled","payload":{"id":"@test:a","enabled":true,"purge":true}}"#,
            b"not json",
            b"",
        ] {
            let (served, response) = exchange(own, request, |_| {
                panic!("an unknown request must never reach the host")
            });
            served.unwrap();
            match serde_json::from_slice::<PluginAdminResponse>(&response).unwrap() {
                PluginAdminResponse::Refused(failure) => assert_eq!(failure.code, CODE_REFUSED),
                other => panic!("unexpected response {other:?}"),
            }
        }
    }

    #[test]
    fn all_admin_requests_parse() {
        for request in [
            r#"{"_tag":"Repositories","payload":{}}"#,
            r#"{"_tag":"RepositoryAdd","payload":{"url":"https://a.example/"}}"#,
            r#"{"_tag":"RepositoryRemove","payload":{"url":"https://a.example/"}}"#,
            r#"{"_tag":"Catalog","payload":{"url":"https://a.example/"}}"#,
            r#"{"_tag":"Inspect","payload":{"url":"https://a.example/","pluginId":"@a:b","releaseVersion":"1"}}"#,
            r#"{"_tag":"Install","payload":{"url":"https://a.example/","pluginId":"@a:b","releaseVersion":"1","approval":"x"}}"#,
            r#"{"_tag":"SetEnabled","payload":{"id":"@a:b","enabled":false}}"#,
        ] {
            serde_json::from_str::<PluginAdminRequest>(request).unwrap();
        }
    }
}
