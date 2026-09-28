// Test-only, read-only observer. Never inspects RPC bindings or WebSocket frames.
use super::{json, Protocol, Value};
use std::{
    fs::OpenOptions,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    time::{Duration, Instant},
};

const EXPRESSION: &str = r#"(() => {
const controls = Array.from(document.querySelectorAll('button,a[href],input,select,textarea,[tabindex]'));
const counts = window.__korriNativeProbeCounts;
return {
  hasFocus: document.hasFocus(), visible: document.visibilityState === 'visible',
  activeIsControl: controls.includes(document.activeElement),
  focusableCount: controls.length, focusIndex: controls.indexOf(document.activeElement) + 1,
  countersPresent: counts !== undefined,
  nativeStarts: counts?.nativeStarts, nativeSamples: counts?.nativeSamples,
  nativeInitializations: counts?.nativeInitializations,
  gamepadStarts: counts?.gamepadStarts, gamepadReads: counts?.gamepadReads
};
})()"#;

pub struct Diagnostic {
    started: Instant,
    next: Instant,
    samples: u32,
    screenshot: bool,
}
impl Diagnostic {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            next: now + Duration::from_secs(1),
            samples: 0,
            screenshot: false,
        }
    }
    pub fn tick(&mut self, protocol: &mut Protocol) {
        // At most 120 seconds and 480 bounded records per shell lifetime.
        if self.started.elapsed() > Duration::from_secs(120)
            || self.samples >= 480
            || Instant::now() < self.next
        {
            return;
        }
        self.next = Instant::now() + Duration::from_millis(250);
        let Some(ready) = protocol.readiness.as_ref() else {
            return;
        };
        let Some(context) = ready.consumed else {
            return;
        };
        let session = ready.session.clone();
        self.samples += 1;
        if let Ok(reply) = protocol.command(
            "Runtime.evaluate",
            json!({
                "expression": EXPRESSION, "contextId": context,
                "returnByValue": true, "timeout": 1000, "silent": true,
            }),
            Some(&session),
        ) {
            let mut safe = sanitize(&reply["result"]["value"]);
            // Stable backend identity, not DOM text, labels, attributes or IDs.
            if let Ok(focus) = protocol.command(
                "Runtime.evaluate",
                json!({
                    "expression": "document.activeElement", "contextId": context,
                    "objectGroup": "native-input-probe", "silent": true,
                }),
                Some(&session),
            ) {
                if let Some(object) = focus["result"]["objectId"].as_str() {
                    if let Ok(node) = protocol.command(
                        "DOM.describeNode",
                        json!({"objectId": object}),
                        Some(&session),
                    ) {
                        safe["focusNode"] = node["node"]["backendNodeId"]
                            .as_u64()
                            .filter(|n| *n > 0 && *n <= i32::MAX as u64)
                            .map(|n| json!(n))
                            .unwrap_or(Value::Null);
                    }
                }
            }
            let _ = protocol.command(
                "Runtime.releaseObjectGroup",
                json!({"objectGroup": "native-input-probe"}),
                Some(&session),
            );
            eprintln!(
                "[DEBUG-native-input] monotonic_us={} state={safe}",
                monotonic_us()
            );
        }
        if !self.screenshot && self.started.elapsed() >= Duration::from_secs(15) {
            self.screenshot = true;
            if let Ok(reply) = protocol.command(
                "Page.captureScreenshot",
                json!({
                    "format": "jpeg", "quality": 35, "captureBeyondViewport": false,
                }),
                Some(&session),
            ) {
                if let (Some(data), Some(runtime)) =
                    (reply["data"].as_str(), std::env::var_os("XDG_RUNTIME_DIR"))
                {
                    if data.len() < 900_000
                        && data
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"+/=".contains(&c))
                    {
                        let path = std::path::PathBuf::from(runtime)
                            .join("native-input-diagnostic.jpg.b64");
                        if let Ok(mut file) = OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .mode(0o600)
                            .custom_flags(libc::O_NOFOLLOW)
                            .open(path)
                        {
                            let _ = file.write_all(data.as_bytes());
                        }
                    }
                }
            }
        }
    }
}
fn monotonic_us() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) } != 0 {
        return 0;
    }
    now.tv_sec as u64 * 1_000_000 + now.tv_nsec as u64 / 1000
}
fn sanitize(value: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in ["hasFocus", "visible", "activeIsControl", "countersPresent"] {
        result.insert(
            key.into(),
            value[key].as_bool().map(Value::Bool).unwrap_or(Value::Null),
        );
    }
    for key in [
        "focusableCount",
        "focusIndex",
        "nativeStarts",
        "nativeSamples",
        "nativeInitializations",
        "gamepadStarts",
        "gamepadReads",
    ] {
        result.insert(
            key.into(),
            value[key]
                .as_u64()
                .filter(|n| *n <= 1_000_000)
                .map(|n| json!(n))
                .unwrap_or(Value::Null),
        );
    }
    Value::Object(result)
}
// Do not inspect or log console, exception, network or authentication messages.
pub fn observe(_message: &Value) {}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_bounded_counts_and_booleans_leave_observer() {
        let safe = sanitize(&json!({"hasFocus": "secret", "nativeStarts": 1,
            "nativeSamples": 1_000_001, "gamepadReads": -1, "url": "secret", "pads": ["secret"]}));
        assert!(!safe.to_string().contains("secret"));
        assert_eq!(safe["nativeStarts"], 1);
        assert!(safe["nativeSamples"].is_null());
        assert!(safe["gamepadReads"].is_null());
        assert!(safe["hasFocus"].is_null());
    }
    #[test]
    fn observation_cannot_deliver_input_or_read_authority() {
        for forbidden in [
            "getGamepads",
            "KorriRpc",
            "WebSocket",
            "dispatchEvent",
            ".focus(",
            ".click(",
            "requestAnimationFrame",
        ] {
            assert!(!EXPRESSION.contains(forbidden));
        }
    }
}
