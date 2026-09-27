// [DEBUG-rpmini-input] Test-only probe, compiled only by diagnostics/package.nix.
// Values come from the same DOM/Gamepad APIs consumed by gamepad-adapter.ts.
use super::{json, Protocol, Value};
use std::{
    fs::OpenOptions,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    time::{Duration, Instant},
};

const EXPRESSION: &str = r#"(() => {
if (!Object.prototype.hasOwnProperty.call(window, '__korriInputFrameCount')) {
  let frames = 0;
  Object.defineProperty(window, '__korriInputFrameCount', {get: () => frames});
  const frame = () => { frames++; requestAnimationFrame(frame); };
  requestAnimationFrame(frame);
}
const result = {
  animationFrames: window.__korriInputFrameCount,
  hasFocus: document.hasFocus(),
  visible: document.visibilityState === 'visible',
  secure: window.isSecureContext,
  gamepadApi: typeof navigator.getGamepads === 'function',
  userActive: navigator.userActivation?.isActive === true,
  userEverActive: navigator.userActivation?.hasBeenActive === true,
  focusableCount: document.querySelectorAll('button,a[href],input,select,textarea,[tabindex]').length,
  buttonCount: document.querySelectorAll('button').length,
  activeIsControl: document.activeElement?.matches('button,a[href],input,select,textarea,[tabindex]') === true,
  focusIndex: Array.from(document.querySelectorAll('button,a[href],input,select,textarea,[tabindex]')).indexOf(document.activeElement) + 1,
  frameCount: window.frames.length
};
if (PROBE_GAMEPADS) {
  try {
    result.pads = Array.from(navigator.getGamepads()).map(p => p ? {
      index: p.index, connected: p.connected, standard: p.mapping === 'standard',
      portal: p.id.includes('(Korri portal)'), game: p.id.includes('(Korri game)'),
      seat: /Korri Seat P[1-4]/.test(p.id),
      buttons: p.buttons.length, axes: p.axes.length,
      pressed: p.buttons.filter(b => b.pressed).length,
      right: p.buttons[15]?.pressed === true,
      activeAxes: p.axes.filter(a => Math.abs(a) > 0.5).length
    } : null);
  } catch (e) { result.gamepadDenied = e?.name === 'SecurityError'; result.gamepadError = true; }
}
return result;
})()"#;

pub struct Diagnostic {
    started: Instant,
    next: Instant,
    screenshot: bool,
}

impl Diagnostic {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            next: now + Duration::from_secs(1),
            screenshot: false,
        }
    }

    pub fn tick(&mut self, protocol: &mut Protocol) {
        if Instant::now() < self.next {
            return;
        }
        self.next = Instant::now() + Duration::from_millis(250);
        let Some(ready) = protocol.readiness.as_ref() else {
            return;
        };
        let Some(context) = ready.consumed else {
            return;
        };
        // Query only the authenticated top-frame context. No global binding reads.
        let session = ready.session.clone();
        let probe_gamepads = self.started.elapsed() >= Duration::from_secs(2);
        let expression = EXPRESSION.replace(
            "PROBE_GAMEPADS",
            if probe_gamepads { "true" } else { "false" },
        );
        match protocol.command(
            "Runtime.evaluate",
            json!({
                "expression": expression, "contextId": context,
                "returnByValue": true, "timeout": 2000, "silent": true,
            }),
            Some(&session),
        ) {
            Ok(reply) => {
                let mut safe = sanitize(&reply["result"]["value"]);
                // CDP's backend node identity stays stable if the DOM list changes.
                let focus = protocol.command(
                    "Runtime.evaluate",
                    json!({
                        "expression": "document.activeElement", "contextId": context,
                        "objectGroup": "rpmini-input-probe", "silent": true,
                    }),
                    Some(&session),
                );
                if let Ok(focus) = focus {
                    if let Some(object) = focus["result"]["objectId"].as_str() {
                        if let Ok(node) = protocol.command(
                            "DOM.describeNode",
                            json!({"objectId": object}),
                            Some(&session),
                        ) {
                            safe["focusNode"] = node["node"]["backendNodeId"]
                                .as_u64()
                                .map(|n| json!(n))
                                .unwrap_or(Value::Null);
                        }
                    }
                }
                let _ = protocol.command(
                    "Runtime.releaseObjectGroup",
                    json!({"objectGroup": "rpmini-input-probe"}),
                    Some(&session),
                );
                eprintln!("[DEBUG-rpmini-input] queried_gamepads={probe_gamepads} state={safe}");
            }
            Err(_) => eprintln!("[DEBUG-rpmini-input] evaluation-failed"),
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
                    // A bounded image, not a page dump or protocol transcript.
                    if data.len() < 900_000
                        && data
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"+/=".contains(&c))
                    {
                        let path = std::path::PathBuf::from(runtime)
                            .join("rpmini-input-diagnostic.jpg.b64");
                        if let Ok(mut file) = OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .mode(0o600)
                            .custom_flags(libc::O_NOFOLLOW)
                            .open(path)
                        {
                            if file.write_all(data.as_bytes()).is_ok() {
                                eprintln!("[DEBUG-rpmini-input] screenshot-saved");
                            }
                        }
                    }
                }
            }
        }
    }
}

// The page cannot make this logger emit strings, errors, URLs, IDs or credentials.
fn sanitize(value: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in [
        "hasFocus",
        "activeIsControl",
        "visible",
        "secure",
        "gamepadApi",
        "userActive",
        "userEverActive",
        "gamepadDenied",
        "gamepadError",
    ] {
        result.insert(
            key.into(),
            value[key].as_bool().map(Value::Bool).unwrap_or(Value::Null),
        );
    }
    for key in [
        "focusableCount",
        "buttonCount",
        "frameCount",
        "focusIndex",
        "animationFrames",
    ] {
        result.insert(
            key.into(),
            value[key]
                .as_u64()
                .map(|n| json!(n.min(100_000)))
                .unwrap_or(Value::Null),
        );
    }
    if let Some(pads) = value["pads"].as_array() {
        result.insert(
            "pads".into(),
            Value::Array(
                pads.iter()
                    .take(16)
                    .map(|pad| {
                        if pad.is_null() {
                            return Value::Null;
                        }
                        let mut entry = serde_json::Map::new();
                        for key in ["connected", "standard", "portal", "game", "seat", "right"] {
                            entry.insert(
                                key.into(),
                                pad[key].as_bool().map(Value::Bool).unwrap_or(Value::Null),
                            );
                        }
                        for key in ["index", "buttons", "axes", "pressed", "activeAxes"] {
                            entry.insert(
                                key.into(),
                                pad[key]
                                    .as_u64()
                                    .map(|n| json!(n.min(256)))
                                    .unwrap_or(Value::Null),
                            );
                        }
                        Value::Object(entry)
                    })
                    .collect(),
            ),
        );
    }
    Value::Object(result)
}

pub fn observe(message: &Value) {
    if message["method"] == "Runtime.exceptionThrown" {
        eprintln!("[DEBUG-rpmini-input] browser-exception");
    }
    if message["method"] == "Runtime.consoleAPICalled" && message["params"]["type"] == "error" {
        eprintln!("[DEBUG-rpmini-input] browser-console-error");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_rejects_strings_and_unlisted_fields() {
        let secret = "do-not-emit-this-value";
        let safe = sanitize(&json!({
            "hasFocus": secret, "buttonCount": secret, "unknown": secret,
            "pads": [{"connected": true, "id": secret, "mapping": secret, "index": 0, "pressed": secret}],
        }));
        assert!(!safe.to_string().contains(secret));
        assert_eq!(safe["pads"][0]["connected"], true);
        assert_eq!(safe["pads"][0]["index"], 0);
        assert!(safe["pads"][0]["pressed"].is_null());
    }
}
