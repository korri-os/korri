use korrid::{
    saves::{peer::SaveHeadsOutcome, types::*},
    RpcRequest, RpcResponse,
};
use serde_json::json;

#[test]
fn peer_save_shapes_stay_native_and_keep_their_wire() {
    let generated = include_str!("../../../contracts/generated/korrid.ts");
    for name in [
        "SaveHeadsRequest",
        "GameSaveHeads",
        "SaveHeadsOutcome",
        "SaveRecordsRequest",
        "SaveRecordsOutcome",
        "SaveInUseActive",
        "SaveInUseIdle",
        "SaveInUseOutcome",
        "SaveObjectRequest",
        "app.saves.heads",
        "app.saves.records",
        "app.saves.inUse",
        "app.saves.object",
    ] {
        assert!(!generated.contains(name), "peer-only shape leaked: {name}");
    }
    let hash = format!("sha256:{}", "a".repeat(64));
    assert_eq!(
        serde_json::to_value(RpcRequest::SaveObject(SaveObjectRequest {
            sha256: hash.clone()
        }))
        .unwrap(),
        json!({"_tag":"app.saves.object","payload":{"sha256":hash}})
    );
    assert_eq!(
        serde_json::to_value(RpcResponse::SaveHeads(SaveHeadsOutcome::Ok(SaveHeads {
            games: vec![]
        })))
        .unwrap(),
        json!({"_tag":"app.saves.heads","outcome":{"_tag":"Ok","payload":{"games":[]}}})
    );
}

#[test]
fn save_check_has_only_status_warnings_and_in_use() {
    let value = SaveCheck {
        status: SaveMark::NotChecked,
        warnings: vec![],
        in_use: vec![],
    };
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        json!({"status":"NotChecked","warnings":[],"inUse":[]})
    );
    let generated = include_str!("../../../contracts/generated/korrid.ts");
    let check = generated
        .split("export interface SaveCheck {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(!check.contains("releaseId") && !check.contains("runnerId"));
}
