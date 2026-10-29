//! Build/sign only here, on the author computer. Korrid owns target staging.
use crate::{
    error::CliError,
    module::Context,
    output::{self, Output},
    plugin_author::{self, AuthorKey},
    plugins, prompt,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use clap::ArgMatches;
use korrid::{plugin_admin::*, *};
use serde::Deserialize;
use std::{io::Read, path::Path};

struct PreparedArchive {
    // Keep the local signed bytes until review finishes, including expiry recovery.
    _workspace: tempfile::TempDir,
    archive: Archive,
    namespace: String,
    key: AuthorKey,
    sha256: String,
    size: u64,
}

pub fn run(args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
    if !context.device_flag {
        return Err(CliError::usage("Sideload needs --device NAME_OR_KEY. Builds always run on this computer, never on the selected device."));
    }
    let supplied_upload = args.get_one::<String>("upload");
    let prepared = if supplied_upload.is_some() {
        None
    } else {
        Some(prepare(args)?)
    };
    let (mut upload_id, mut observed) = if let Some(id) = supplied_upload {
        (id.clone(), vec![])
    } else {
        match send_prepared(
            context,
            prepared.as_ref().expect("fresh build"),
            args.get_flag("json"),
        )? {
            Upload::Ready { upload_id, begin } => (upload_id, vec![*begin]),
            Upload::Refused(responses) => return Ok(upload_refused(responses)),
        }
    };
    announce(args, context, &upload_id);
    let inspect = |id: &str| {
        RpcRequest::PluginArchiveInspect(PluginArchiveUploadRequest {
            upload_id: id.into(),
        })
    };
    let mut response =
        plugins::inspect_with_trust(context, &inspect(&upload_id), args.get_flag("json"))?;
    if output::failure(&response).is_some_and(|failure| {
        matches!(
            failure.code.as_str(),
            CODE_PUBLISHER_NOT_TRUSTED | CODE_PUBLISHER_KEY_CHANGED
        )
    }) {
        if !plugins::can_prompt(args.get_flag("json")) {
            let next = format!(
                "{}\n{}",
                output::failure(&response).expect("trust refusal").message,
                manual_trust(args, context, &upload_id, prepared.as_ref())
            );
            return Ok(finish(
                args,
                context,
                &upload_id,
                observed,
                response,
                Some(next),
            ));
        }
        // An opaque upload ID has no publisher facts. A resumed terminal trust
        // uses this project's already-built manifest and existing local key,
        // never a guessed namespace, a new key, or a CLI session file.
        let publisher = match prepared.as_ref() {
            Some(prepared) => Ok(PluginTrustPublisherRequest {
                namespace: prepared.namespace.clone(),
                public_key: prepared.key.public.clone(),
            }),
            None => local_publisher(args),
        };
        let publisher = match publisher {
            Ok(publisher) => publisher,
            Err(_) => {
                let next = format!(
                    "This directory has no available plugin publisher and existing author key.\n{}",
                    manual_trust(args, context, &upload_id, None)
                );
                return Ok(finish(
                    args,
                    context,
                    &upload_id,
                    observed,
                    response,
                    Some(next),
                ));
            }
        };
        if let Some(failure) = output::failure(&response) {
            eprintln!("{}", output::text(&failure.message));
        }
        eprintln!(
            "Publisher: {}\nFingerprint: {}",
            publisher.namespace,
            plugin_author::fingerprint(&publisher.public_key)?
        );
        prompt::confirm("Trust this author's key on the selected device? Plugin permissions still need separate approval.", false)?;
        let applied = context.rpc(&RpcRequest::PluginTrustPublisher(publisher))?;
        if output::failure(&applied).is_some() {
            return Ok(finish(args, context, &upload_id, observed, applied, None));
        }
        response =
            plugins::inspect_with_trust(context, &inspect(&upload_id), args.get_flag("json"))?;
        if output::failure(&response).is_some_and(|failure| failure.code == CODE_UPLOAD_NOT_FOUND) {
            if let Some(prepared) = prepared.as_ref() {
                eprintln!("The staged archive expired while you reviewed the publisher key. Sending the same signed archive again.");
                match send_prepared(context, prepared, args.get_flag("json"))? {
                    Upload::Ready {
                        upload_id: next_id,
                        begin,
                    } => {
                        upload_id = next_id;
                        observed.push(*begin);
                    }
                    Upload::Refused(mut responses) => {
                        observed.append(&mut responses);
                        return Ok(upload_refused(observed));
                    }
                }
                announce(args, context, &upload_id);
                response = plugins::inspect_with_trust(
                    context,
                    &inspect(&upload_id),
                    args.get_flag("json"),
                )?;
            }
        }
    }
    let Some(report) = plugins::inspection(&response) else {
        return Ok(finish(args, context, &upload_id, observed, response, None));
    };
    if args.get_one::<String>("approval").is_none() && !plugins::can_prompt(args.get_flag("json")) {
        return Ok(finish(args, context, &upload_id, observed, response, None));
    }
    let approval = plugins::approval(args, report)?;
    let response = context.rpc(&RpcRequest::PluginArchiveInstall(
        PluginArchiveUploadInstallRequest {
            upload_id: upload_id.clone(),
            approval,
        },
    ))?;
    Ok(finish(args, context, &upload_id, observed, response, None))
}

fn prepare(args: &ArgMatches) -> Result<PreparedArchive, CliError> {
    let directory = Path::new(args.get_one::<String>("dir").expect("default"));
    let platform = args.get_one::<String>("platform");
    let outputs = plugin_author::build(directory, platform.map(String::as_str))?;
    if outputs.len() != 1 {
        return Err(CliError::usage("Sideload needs one plugin output. Supply --platform with the selected device's Nix system."));
    }
    let namespace = plugin_author::namespace(&outputs)?;
    let key = plugin_author::author_key(&namespace)?;
    let workspace = tempfile::tempdir().map_err(|_| {
        CliError::failure(
            "PluginFilesUnavailable",
            "Could not create a local archive folder.",
            "Check the temporary folder's free space and permissions.",
        )
    })?;
    let archive = plugin_author::archive(
        &outputs[0],
        workspace.path(),
        &key,
        platform.map(String::as_str),
        args.get_flag("self-contained"),
    )?;
    if archive.platform != "any" && platform != Some(&archive.platform) {
        return Err(CliError::usage("The selected device does not report its build platform. Supply --platform with its Nix system; only any-architecture packages can omit it."));
    }
    let (sha256, size) = plugin_author::hash_archive(&archive.archive)?;
    if sha256 != archive.sha256 || size == 0 {
        return Err(CliError::failure(
            "PluginArchiveInvalid",
            "The publisher's archive hash does not match its bytes, or the archive is empty.",
            "Check korri-publish, then build and sideload again.",
        ));
    }
    Ok(PreparedArchive {
        _workspace: workspace,
        archive,
        namespace,
        key,
        sha256,
        size,
    })
}

fn local_publisher(args: &ArgMatches) -> Result<PluginTrustPublisherRequest, CliError> {
    let directory = Path::new(args.get_one::<String>("dir").expect("default"));
    let namespace = plugin_author::project_namespace(directory)?;
    let key = plugin_author::existing_author_key(&namespace)?;
    Ok(PluginTrustPublisherRequest {
        namespace,
        public_key: key.public,
    })
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn device<'a>(args: &'a ArgMatches, context: &'a Context) -> &'a str {
    context
        .device
        .as_deref()
        .unwrap_or_else(|| args.get_one::<String>("device").expect("selected device"))
}
fn resume_command(
    args: &ArgMatches,
    context: &Context,
    upload_id: &str,
    approval: Option<&str>,
) -> String {
    let directory = args.get_one::<String>("dir").expect("default");
    let directory = if directory == "." {
        String::new()
    } else {
        format!(" {}", shell_word(directory))
    };
    let approval = approval
        .map(|digest| format!(" --approval {}", shell_word(digest)))
        .unwrap_or_default();
    format!(
        "korri plugins sideload{directory} --device {} --upload {}{approval}",
        shell_word(device(args, context)),
        shell_word(upload_id)
    )
}
fn announce(args: &ArgMatches, context: &Context, upload_id: &str) {
    if plugins::can_prompt(args.get_flag("json")) {
        eprintln!(
            "Upload: {}\nResume: {}",
            output::text(upload_id),
            output::text(&resume_command(args, context, upload_id, None))
        );
    }
}
fn manual_trust(
    args: &ArgMatches,
    context: &Context,
    id: &str,
    prepared: Option<&PreparedArchive>,
) -> String {
    let (namespace, key) = prepared
        .map(|prepared| (prepared.namespace.clone(), shell_word(&prepared.key.public)))
        .unwrap_or_else(|| ("<namespace>".into(), "<full public key>".into()));
    let fingerprint = prepared
        .and_then(|prepared| plugin_author::fingerprint(&prepared.key.public).ok())
        .map(|fingerprint| format!("Fingerprint: {fingerprint}\n"))
        .unwrap_or_default();
    format!("{fingerprint}Next: run korri plugins trust {namespace} --key {key} --device {}, then run sideload again with --upload {} (same plugin directory and device).", shell_word(device(args, context)), shell_word(id))
}

fn finish(
    args: &ArgMatches,
    context: &Context,
    upload_id: &str,
    mut observed: Vec<RpcResponse>,
    response: RpcResponse,
    note: Option<String>,
) -> Output {
    let body = if let Some(note) = note {
        note
    } else if let Some(failure) =
        output::failure(&response).filter(|failure| failure.code == CODE_UPLOAD_NOT_FOUND)
    {
        format!(
            "{}\nNext: run sideload again without --upload",
            failure.message
        )
    } else {
        plugins::result(response.clone()).human
    };
    let mut human = format!(
        "Upload: {}\n{}",
        output::text(upload_id),
        body.lines()
            .map(output::text)
            .collect::<Vec<_>>()
            .join("\n")
    );
    if plugins::inspection(&response).is_some()
        || matches!(&response, RpcResponse::PluginTrustPublisher(_))
        || output::failure(&response).is_some_and(|failure| {
            matches!(
                failure.code.as_str(),
                CODE_PUBLISHER_NOT_TRUSTED | CODE_PUBLISHER_KEY_CHANGED
            )
        })
    {
        let approval = plugins::inspection(&response).map(|report| report.approval.as_str());
        human.push_str(&format!(
            "\nResume: {}",
            output::text(&resume_command(args, context, upload_id, approval))
        ));
    }
    observed.push(response);
    Output::many(observed, human)
}
fn upload_refused(responses: Vec<RpcResponse>) -> Output {
    let mut human = plugins::result(responses.last().expect("upload response").clone()).human;
    if let Some(id) = responses.iter().rev().find_map(|response| match response {
        RpcResponse::PluginArchiveBegin(PluginArchiveBeginOutcome::Ok(upload)) => {
            Some(&upload.upload_id)
        }
        _ => None,
    }) {
        human = format!(
            "Upload: {}\n{}\nNext: run sideload again without --upload",
            output::text(id),
            human
        );
    }
    Output::many(responses, human)
}

enum Upload {
    Ready {
        upload_id: String,
        begin: Box<RpcResponse>,
    },
    Refused(Vec<RpcResponse>),
}
fn send_prepared(
    context: &Context,
    prepared: &PreparedArchive,
    json: bool,
) -> Result<Upload, CliError> {
    upload(
        context,
        &prepared.archive.archive,
        prepared.sha256.clone(),
        prepared.size,
        json,
    )
}
fn upload(
    context: &Context,
    path: &Path,
    sha256: String,
    size: u64,
    json: bool,
) -> Result<Upload, CliError> {
    let begin = context.rpc(&RpcRequest::PluginArchiveBegin(PluginArchiveBeginRequest {
        sha256,
        size,
    }))?;
    let upload_id = match &begin {
        RpcResponse::PluginArchiveBegin(PluginArchiveBeginOutcome::Ok(upload)) => {
            upload.upload_id.clone()
        }
        _ => return Ok(Upload::Refused(vec![begin])),
    };
    let mut file = plugin_author::read_archive(path)?;
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut offset = 0;
    loop {
        let count = file.read(&mut buffer).map_err(|_| {
            CliError::failure(
                "PluginArchiveUnavailable",
                "Could not read the local archive during upload.",
                "Check the local archive folder, then sideload again.",
            )
        })?;
        if count == 0 {
            break;
        }
        let response = context.rpc(&RpcRequest::PluginArchiveChunk(PluginArchiveChunkRequest {
            upload_id: upload_id.clone(),
            offset,
            data: STANDARD.encode(&buffer[..count]),
        }))?;
        if output::failure(&response).is_some() {
            return Ok(Upload::Refused(vec![begin, response]));
        }
        offset += count as u64;
        if plugins::can_prompt(json) {
            eprintln!("Uploaded {offset} of {size} bytes.");
        }
    }
    let response = context.rpc(&RpcRequest::PluginArchiveFinish(
        PluginArchiveUploadRequest {
            upload_id: upload_id.clone(),
        },
    ))?;
    if output::failure(&response).is_some() {
        return Ok(Upload::Refused(vec![begin, response]));
    }
    Ok(Upload::Ready {
        upload_id,
        begin: Box::new(begin),
    })
}

/// Build-side korri-publish output. This shape is approved by the parent and
/// produced by the host worker's archive command, not a CLI-owned manifest.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Archive {
    pub archive: std::path::PathBuf,
    pub sha256: String,
    pub platform: String,
    pub store_path: String,
}
