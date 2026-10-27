//! Android edge: JNI functions mirroring
//! clients/android/.../korrid/KorridServer.java.

use crate::{
    apply_local_owner_binding, issue_folder_selection_receipt, korrid_version,
    local_identity_status_json, local_owner_binding_template, local_server_capability,
    moonlight_host_candidates, start_local_server, stop_local_server,
    MoonlightCertificateProvisionOutcome, MoonlightCertificateProvisionRequest, RpcRequest,
    RpcResponse,
};
use jni::{
    objects::{JClass, JObject, JString},
    sys::{jint, jstring},
    JNIEnv,
};
use std::ptr;

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_version(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    match env.new_string(korrid_version()) {
        Ok(value) => value.into_raw(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_start(
    mut env: JNIEnv,
    _class: JClass,
    application_context: JObject,
    allowed_origin: JString,
    local_storage_root: JString,
    private_state_root: JString,
) -> jint {
    // iroh's rustls verifier needs the application's class loader and trust
    // store before federation networking starts. Both wrappers borrow the
    // same live JNI call; no local reference or Env escapes it.
    let context = application_context.as_raw();
    let mut verifier_env = unsafe { jni_runtime::EnvUnowned::from_raw(env.get_raw().cast()) };
    verifier_env
        .with_env(|runtime| {
            let context =
                unsafe { jni_runtime::objects::JObject::from_raw(runtime, context.cast()) };
            rustls_platform_verifier::android::init_with_env(runtime, context)
        })
        .resolve::<jni_runtime::errors::ThrowRuntimeExAndDefault>();
    if env.exception_check().unwrap_or(true) {
        return -1;
    }
    let allowed_origin: String = match env.get_string(&allowed_origin) {
        Ok(value) => value.into(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", error.to_string());
            return -1;
        }
    };
    let local_storage_root: String = match env.get_string(&local_storage_root) {
        Ok(value) => value.into(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", error.to_string());
            return -1;
        }
    };
    let private_state_root: String = match env.get_string(&private_state_root) {
        Ok(value) => value.into(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", error.to_string());
            return -1;
        }
    };
    match start_local_server(&allowed_origin, &local_storage_root, &private_state_root) {
        Ok(port) => port.into(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
            -1
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_capability(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    match local_server_capability() {
        Some(capability) => match env.new_string(capability) {
            Ok(value) => value.into_raw(),
            Err(error) => {
                let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
                ptr::null_mut()
            }
        },
        None => {
            let _ = env.throw_new("java/lang/IllegalStateException", "korrid is not running");
            ptr::null_mut()
        }
    }
}

fn string_result(env: &mut JNIEnv, result: Result<String, crate::ServerError>) -> jstring {
    match result {
        Ok(json) => match env.new_string(json) {
            Ok(value) => value.into_raw(),
            Err(error) => {
                let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
                ptr::null_mut()
            }
        },
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_identityStatus(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    string_result(&mut env, local_identity_status_json())
}

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_ownerBindingTemplate(
    mut env: JNIEnv,
    _class: JClass,
    created_at: jni::sys::jlong,
) -> jstring {
    if created_at < 0 {
        let _ = env.throw_new(
            "java/lang/IllegalArgumentException",
            "created_at must be non-negative",
        );
        return ptr::null_mut();
    }
    string_result(&mut env, local_owner_binding_template(created_at as u64))
}

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_applyOwnerBinding(
    mut env: JNIEnv,
    _class: JClass,
    unsigned_template_json: JString,
    expected_owner_public_key: JString,
    signed_event_json: JString,
) -> jstring {
    let unsigned_template_json: String = match env.get_string(&unsigned_template_json) {
        Ok(value) => value.into(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", error.to_string());
            return ptr::null_mut();
        }
    };
    let expected_owner_public_key: String = match env.get_string(&expected_owner_public_key) {
        Ok(value) => value.into(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", error.to_string());
            return ptr::null_mut();
        }
    };
    let signed_event_json: String = match env.get_string(&signed_event_json) {
        Ok(value) => value.into(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", error.to_string());
            return ptr::null_mut();
        }
    };
    string_result(
        &mut env,
        apply_local_owner_binding(
            &unsigned_template_json,
            &expected_owner_public_key,
            &signed_event_json,
        ),
    )
}

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_moonlightHostCandidates(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let json = match moonlight_host_candidates() {
        Ok(candidates) => serde_json::json!({
            "_tag": "Candidates",
            "items": candidates
                .into_iter()
                .map(|candidate| serde_json::json!({
                    "label": candidate.label,
                    "address": candidate.address,
                }))
                .collect::<Vec<_>>(),
        }),
        Err(_) => serde_json::json!({"_tag": "Failed"}),
    };
    match env.new_string(json.to_string()) {
        Ok(value) => value.into_raw(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_issueFolderSelectionReceipt(
    mut env: JNIEnv,
    _class: JClass,
    canonical_approved_path: JString,
) -> jstring {
    let canonical_approved_path: String = match env.get_string(&canonical_approved_path) {
        Ok(value) => value.into(),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", error.to_string());
            return ptr::null_mut();
        }
    };
    match issue_folder_selection_receipt(&canonical_approved_path) {
        Ok(receipt) => match env.new_string(receipt) {
            Ok(value) => value.into_raw(),
            Err(error) => {
                let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
                ptr::null_mut()
            }
        },
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_provisionMoonlightCertificate(
    mut env: JNIEnv,
    _class: JClass,
    host_uuid: JString,
    public_client_certificate: JString,
) -> jstring {
    let host_uuid: String = match env.get_string(&host_uuid) {
        Ok(value) => value.into(),
        Err(_) => return provision_result(&mut env, "InvalidRequest", None),
    };
    let public_client_certificate: String = match env.get_string(&public_client_certificate) {
        Ok(value) => value.into(),
        Err(_) => return provision_result(&mut env, "InvalidRequest", None),
    };
    if crate::host::moonlight_certificate::validate_host_uuid(&host_uuid).is_err()
        || crate::host::moonlight_certificate::validate_single_pem(&public_client_certificate)
            .is_err()
    {
        return provision_result(&mut env, "InvalidRequest", None);
    }

    let Some(port) = crate::local_server_port() else {
        return provision_result(&mut env, "ServerUnavailable", None);
    };
    let Some(capability) = crate::local_server_capability() else {
        return provision_result(&mut env, "ServerUnavailable", None);
    };
    let request = RpcRequest::MoonlightCertificateProvision(MoonlightCertificateProvisionRequest {
        host_uuid,
        client_certificate: public_client_certificate,
    });
    let outcome = std::thread::Builder::new()
        .name("korrid-moonlight-provision".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "ServerUnavailable")?;
            runtime.block_on(async move {
                // The broker owns one deadline across peer selection and provision.
                // Keep the localhost caller alive longer than that complete operation.
                let client = reqwest::Client::builder()
                    .timeout(crate::upstreams::MOONLIGHT_CERTIFICATE_CALLER_TIMEOUT)
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|_| "ServerUnavailable")?;
                let response = client
                    .post(format!("http://127.0.0.1:{port}/rpc"))
                    .bearer_auth(capability)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|_| "PeerUnavailable")?;
                if !response.status().is_success() {
                    return Err("PeerUnavailable");
                }
                let response = response
                    .json::<RpcResponse>()
                    .await
                    .map_err(|_| "InvalidResponse")?;
                match response {
                    RpcResponse::MoonlightCertificateProvision(
                        MoonlightCertificateProvisionOutcome::Ok(provisioned),
                    ) => Ok(provisioned.server_certificate),
                    RpcResponse::MoonlightCertificateProvision(
                        MoonlightCertificateProvisionOutcome::Err(failure),
                    ) => Err(match failure.code.as_str() {
                        "MoonlightHostNotFound" => "HostNotFound",
                        "MoonlightHostAmbiguous" => "HostAmbiguous",
                        "MoonlightCertificatePeerUnavailable" => "PeerUnavailable",
                        "MoonlightCertificateInvalid" => "InvalidCertificate",
                        _ => "ProvisioningFailed",
                    }),
                    _ => Err("InvalidResponse"),
                }
            })
        })
        .map_err(|_| "ServerUnavailable")
        .and_then(|thread| thread.join().unwrap_or(Err("ServerUnavailable")));

    match outcome {
        Ok(server_certificate) => {
            provision_result(&mut env, "Provisioned", Some(server_certificate.as_str()))
        }
        Err(code) => provision_result(&mut env, code, None),
    }
}

fn provision_result(
    env: &mut JNIEnv,
    tag_or_code: &str,
    server_certificate: Option<&str>,
) -> jstring {
    let json = match server_certificate {
        Some(certificate) => serde_json::json!({
            "_tag": tag_or_code,
            "serverCertificate": certificate,
        }),
        None => serde_json::json!({
            "_tag": "Failed",
            "code": tag_or_code,
        }),
    };
    match env.new_string(json.to_string()) {
        Ok(value) => value.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_simonwjackson_korri_korrid_KorridServer_stop(
    mut env: JNIEnv,
    _class: JClass,
) {
    if let Err(error) = stop_local_server() {
        let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
    }
}
