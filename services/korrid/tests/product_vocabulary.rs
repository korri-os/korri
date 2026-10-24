//! The product wire is a clean cut. Disk records are not part of this rename.
use korrid::{
    LibraryFolderAddRequest, LibraryFolderChangeRequest, LibraryFolderRemoveRequest,
    LibraryFoldersRescanRequest, LibraryFoldersSnapshotRequest, RpcRequest, RpcResponse,
    SessionEndOutcome, SessionEndPhase, SessionEndRequest, SessionEndResult, SessionLeaveRequest,
    SessionResumeRequest,
};

#[test]
fn product_requests_serialize_with_product_tags_and_fields() {
    for (request, tag, payload) in [
        (
            RpcRequest::LibraryFoldersSnapshot(LibraryFoldersSnapshotRequest {}),
            "app.library.folders.snapshot",
            serde_json::json!({}),
        ),
        (
            RpcRequest::LibraryFolderRemove(LibraryFolderRemoveRequest {
                folder: "gba-2".into(),
            }),
            "app.library.folders.remove",
            serde_json::json!({"folder":"gba-2"}),
        ),
        (
            RpcRequest::SessionLeave(SessionLeaveRequest {
                expected_launch_id: Some("exact-launch".into()),
            }),
            "app.session.leave",
            serde_json::json!({"expectedLaunchId":"exact-launch"}),
        ),
        (
            RpcRequest::SessionResume(SessionResumeRequest {
                expected_launch_id: Some("exact-launch".into()),
            }),
            "app.session.resume",
            serde_json::json!({"expectedLaunchId":"exact-launch"}),
        ),
        (
            RpcRequest::SessionEnd(SessionEndRequest {
                expected_launch_id: Some("exact-launch".into()),
                force: None,
            }),
            "app.session.end",
            serde_json::json!({"expectedLaunchId":"exact-launch"}),
        ),
        (
            RpcRequest::LibraryFolderAdd(LibraryFolderAddRequest {
                receipt: "issued-receipt".into(),
                system: "gba".into(),
            }),
            "app.library.folders.add",
            serde_json::json!({"receipt":"issued-receipt","system":"gba"}),
        ),
        (
            RpcRequest::LibraryFolderChange(LibraryFolderChangeRequest {
                folder: "gba-2".into(),
                system: "gba".into(),
            }),
            "app.library.folders.change",
            serde_json::json!({"folder":"gba-2","system":"gba"}),
        ),
        (
            RpcRequest::LibraryFoldersRescan(LibraryFoldersRescanRequest {}),
            "app.library.folders.rescan",
            serde_json::json!({}),
        ),
    ] {
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({"_tag":tag,"payload":payload})
        );
    }
    assert_eq!(
        serde_json::to_value(RpcResponse::SessionEnd(SessionEndOutcome::Ok(
            SessionEndResult {
                phase: SessionEndPhase::Ended
            }
        )))
        .unwrap(),
        serde_json::json!({"_tag":"app.session.end","outcome":{"_tag":"Ok","payload":{"phase":"ended"}}})
    );
}

#[test]
fn removed_wire_tags_and_folder_fields_are_not_accepted() {
    for tag in [
        "app.discovery.snapshot",
        "app.discovery.pair",
        "app.discovery.change",
        "app.discovery.unpair",
        "app.discovery.rescan",
        "app.session.freeze",
        "app.session.thaw",
        "app.session.stop",
    ] {
        assert!(
            serde_json::from_value::<RpcRequest>(serde_json::json!({"_tag":tag,"payload":{}}))
                .is_err(),
            "accepted {tag}"
        );
    }
    assert!(serde_json::from_value::<RpcRequest>(
        serde_json::json!({"_tag":"app.library.folders.remove","payload":{"source":"gba-2"}})
    )
    .is_err());
}
