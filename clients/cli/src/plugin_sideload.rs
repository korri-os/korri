//! Build/sign only here, on the author computer. Korrid owns target staging.
use crate::{
    error::CliError,
    module::Context,
    output::Output,
    plugin_author::{self, AuthorKey},
    plugin_wire::*,
    plugins, prompt,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use clap::ArgMatches;
use serde::Deserialize;
use std::{io::Read, path::Path};

pub fn run(args: &ArgMatches, context: &Context) -> Result<Output, CliError> {
    if !context.device_flag {
        return Err(CliError::usage("Sideload needs --device NAME_OR_KEY. Builds always run on this computer, never on the selected device."));
    }
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
    let archive = plugin_author::archive(&outputs[0], workspace.path(), &key)?;
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
    let staged = upload(
        context,
        &archive.archive,
        sha256.clone(),
        size,
        args.get_flag("json"),
    )?;
    let mut upload_id = match staged {
        Upload::Ready(upload_id) => upload_id,
        Upload::Refused(response) => return Ok(plugins::result(*response)),
    };
    let inspect = RpcRequest::PluginArchiveInspect(PluginArchiveUploadRequest {
        upload_id: upload_id.clone(),
    });
    let mut response = plugins::inspect_with_trust(context, &inspect, args.get_flag("json"))?;
    if response.failure().is_some_and(|failure| {
        failure.code == "PluginPublisherNotTrusted" || failure.code == "PluginPublisherKeyChanged"
    }) {
        // Signed archives do not carry a key-discovery catalog. Use the actual
        // local author's public key, never a key inferred from the signature.
        if !plugins::can_prompt(args.get_flag("json")) {
            if !args.get_flag("json") {
                eprintln!("Publisher: {}\nFingerprint: {}\nTrust explicitly with korri plugins trust {} --key '{}' on the same --device, then sideload again.", namespace, plugin_author::fingerprint(&key.public)?, namespace, key.public);
            }
            return Ok(plugins::result(response));
        }
        if let Some(failure) = response.failure() {
            eprintln!("{}", crate::output::text(&failure.message));
        }
        eprintln!(
            "Publisher: {}\nFingerprint: {}",
            namespace,
            plugin_author::fingerprint(&key.public)?
        );
        prompt::confirm("Trust this author's key on the selected device? Plugin permissions still need separate approval.", false)?;
        let applied = trust(context, &namespace, &key)?;
        if applied.failure().is_some() {
            return Ok(plugins::result(applied));
        }
        response = plugins::inspect_with_trust(context, &inspect, args.get_flag("json"))?;
        if response
            .failure()
            .is_some_and(|failure| failure.code == "PluginUploadNotFound")
        {
            if !args.get_flag("json") {
                eprintln!("The staged archive expired while you reviewed the publisher key. Sending the same signed archive again.");
            }
            upload_id = match upload(
                context,
                &archive.archive,
                sha256,
                size,
                args.get_flag("json"),
            )? {
                Upload::Ready(upload_id) => upload_id,
                Upload::Refused(response) => return Ok(plugins::result(*response)),
            };
            response = plugins::inspect_with_trust(
                context,
                &RpcRequest::PluginArchiveInspect(PluginArchiveUploadRequest {
                    upload_id: upload_id.clone(),
                }),
                args.get_flag("json"),
            )?;
        }
    }
    let Some(report) = plugins::inspection(&response) else {
        return Ok(plugins::result(response));
    };
    // Piped/JSON use first returns the exact wire inspection for review. A
    // subsequent invocation must supply its exact digest; --yes grants none.
    if args.get_one::<String>("approval").is_none() && !plugins::can_prompt(args.get_flag("json")) {
        return Ok(plugins::result(response));
    }
    let approval = plugins::approval(args, report)?;
    Ok(plugins::result(context.plugin_rpc(
        &RpcRequest::PluginArchiveInstall(PluginArchiveInstallRequest {
            upload_id,
            approval,
        }),
    )?))
}

fn trust(context: &Context, namespace: &str, key: &AuthorKey) -> Result<RpcResponse, CliError> {
    context.plugin_rpc(&RpcRequest::PluginTrustPublisher(
        PluginTrustPublisherRequest {
            namespace: namespace.to_owned(),
            public_key: key.public.clone(),
        },
    ))
}

enum Upload {
    Ready(String),
    Refused(Box<RpcResponse>),
}
fn upload(
    context: &Context,
    path: &Path,
    sha256: String,
    size: u64,
    json: bool,
) -> Result<Upload, CliError> {
    let response =
        context.plugin_rpc(&RpcRequest::PluginArchiveBegin(PluginArchiveBeginRequest {
            sha256,
            size,
        }))?;
    let upload_id = match response {
        RpcResponse::PluginArchiveBegin(PluginArchiveBeginOutcome::Ok(upload)) => upload.upload_id,
        response => return Ok(Upload::Refused(Box::new(response))),
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
        let response =
            context.plugin_rpc(&RpcRequest::PluginArchiveChunk(PluginArchiveChunkRequest {
                upload_id: upload_id.clone(),
                offset,
                data: STANDARD.encode(&buffer[..count]),
            }))?;
        if response.failure().is_some() {
            return Ok(Upload::Refused(Box::new(response)));
        }
        offset += count as u64;
        if plugins::can_prompt(json) {
            eprintln!("Uploaded {offset} of {size} bytes.");
        }
    }
    let response = context.plugin_rpc(&RpcRequest::PluginArchiveFinish(
        PluginArchiveUploadRequest {
            upload_id: upload_id.clone(),
        },
    ))?;
    if response.failure().is_some() {
        return Ok(Upload::Refused(Box::new(response)));
    }
    Ok(Upload::Ready(upload_id))
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
