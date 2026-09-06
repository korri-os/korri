//! Tracked stale-grant cleanup after the approved offline owner cut. This is
//! not Sunshine's all-client erase and cannot discover untracked pairings.
use crate::{
    authorization::Authorization,
    host::moonlight_certificate::{encode_revoke_request, MoonlightCertificateAdapter},
    identity::offline::VerifiedPrivateOwner,
};
use std::path::Path;

pub(crate) fn reconcile(
    root: &Path,
    expected_device: &str,
    expected_owner: &str,
    expected_event: &str,
    now: u64,
    adapter: &dyn MoonlightCertificateAdapter,
) -> Result<String, String> {
    let operation = || {
        let identity =
            VerifiedPrivateOwner::open(root, expected_device, expected_owner, expected_event)
                .map_err(|error| error.to_string())?;
        let authorization = Authorization::open_offline(root).map_err(|error| error.to_string())?;
        let plan = authorization
            .all_stale_certificate_revocations(&identity.state, now)
            .map_err(|error| error.to_string())?;
        // No first revoke until the whole inventory and every outgoing payload
        // pass the same bounded producer validation used by the socket adapter.
        for revocation in &plan {
            encode_revoke_request(&revocation.host_uuid, &revocation.client_certificate)
                .map_err(|error| error.code)?;
        }
        for revocation in &plan {
            identity.recheck().map_err(|error| error.to_string())?;
            // changed=false is a successful durable absence after bulk erase or
            // an interrupted prior attempt. Never delete after a failed reply.
            adapter
                .revoke(&revocation.host_uuid, &revocation.client_certificate)
                .map_err(|error| error.code)?;
            identity.recheck().map_err(|error| error.to_string())?;
            authorization
                .complete_certificate_revocation(&revocation.device_public_key)
                .map_err(|error| error.to_string())?;
        }
        identity.recheck().map_err(|error| error.to_string())?;
        authorization
            .confirm_offline_reconciliation()
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "Reconciled {} stale client grants; remaining grants: 0.",
            plan.len()
        ))
    };
    operation().map_err(|error: String| format!(
        "offline grant reconciliation failed: {error}; keep all authority isolated; retry only after verifying the new binding and remaining inventory; never restore retired trust"
    ))
}
