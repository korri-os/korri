//! Stateful implementation of korrid's single active staging slot for CLI tests.
use super::{inspection, ok, refused, target, KEY};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

struct Archive {
    id: String,
    sha256: String,
    size: u64,
    bytes: Vec<u8>,
    finished: bool,
}

pub(super) struct Staging {
    active: Option<Archive>,
    trusted: bool,
    pub(super) begins: usize,
    pub(super) installed: bool,
    pub(super) calls: Vec<Value>,
}
impl Staging {
    pub(super) fn new(trusted: bool) -> Self {
        Self {
            active: None,
            trusted,
            begins: 0,
            installed: false,
            calls: vec![],
        }
    }
    pub(super) fn respond(&mut self, remote: bool, request: Value) -> Value {
        self.calls.push(request.clone());
        let tag = request["_tag"].as_str().unwrap();
        let payload = &request["payload"];
        if let Some(exchange) = target()
            .into_iter()
            .find(|exchange| exchange.request == request)
        {
            assert!(!remote);
            return exchange.response;
        }
        assert!(remote);
        match tag {
            "system.plugins.archive.begin" => {
                if self.active.is_some() {
                    return refused(tag, "PluginHostRefused");
                }
                self.begins += 1;
                let id = format!("upload-{}", self.begins);
                self.active = Some(Archive {
                    id: id.clone(),
                    sha256: payload["sha256"].as_str().unwrap().into(),
                    size: payload["size"].as_u64().unwrap(),
                    bytes: vec![],
                    finished: false,
                });
                ok(tag, json!({"uploadId":id}))
            }
            "system.plugins.publisher.trust" => {
                assert_eq!(payload, &json!({"namespace":"@author","publicKey":KEY}));
                self.trusted = true;
                ok(tag, json!({}))
            }
            _ => {
                let Some(archive) = self
                    .active
                    .as_mut()
                    .filter(|archive| payload["uploadId"] == archive.id)
                else {
                    self.active = None;
                    return refused(tag, "PluginUploadNotFound");
                };
                match tag {
                    "system.plugins.archive.chunk" => {
                        assert!(!archive.finished);
                        assert_eq!(
                            payload["offset"].as_u64().unwrap(),
                            archive.bytes.len() as u64
                        );
                        let bytes = STANDARD.decode(payload["data"].as_str().unwrap()).unwrap();
                        assert!(bytes.len() <= 1024 * 1024);
                        archive.bytes.extend(bytes);
                        ok(tag, json!({}))
                    }
                    "system.plugins.archive.finish" => {
                        assert_eq!(archive.bytes.len() as u64, archive.size);
                        assert_eq!(
                            format!("{:x}", Sha256::digest(&archive.bytes)),
                            archive.sha256
                        );
                        archive.finished = true;
                        ok(tag, json!({}))
                    }
                    "system.plugins.archive.inspect" => {
                        assert!(archive.finished);
                        if self.trusted {
                            ok(tag, inspection("Trusted"))
                        } else {
                            refused(tag, "PluginPublisherNotTrusted")
                        }
                    }
                    "system.plugins.archive.install" => {
                        assert!(archive.finished && self.trusted);
                        assert_eq!(payload["approval"], "exact-digest");
                        self.active = None;
                        self.installed = true;
                        ok(tag, json!({}))
                    }
                    _ => panic!("unexpected staged request {request}"),
                }
            }
        }
    }
}
