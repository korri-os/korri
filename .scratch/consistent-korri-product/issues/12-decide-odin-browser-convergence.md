# Decide the Odin browser path convergence

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by: 07

## Question

The portal module with the private `window.KorriRpc` binding is the product browser path, and Odin must converge to it ([Decide the shared product and hardware boundary](07-decide-product-device-boundary.md#ownership)). Odin today runs the Rust kiosk crate ([services/kiosk](../../../services/kiosk/README.md), enabled in [web-session.nix](../../../nix/devices/odin2portal/web-session.nix)), which intercepts `/runtime.json` over a CDP pipe. What does the kiosk crate become, and what does Odin need from the portal module that it does not have today?

Inspect why Odin took a different path: compare [clients/portal/nix/nixos-module.nix](../../../clients/portal/nix/nixos-module.nix) with the kiosk crate and Odin's `web-session.nix` for compositor, GPU, `neverFocusAppIds`, and Sunshine settings that the portal module cannot express. Recheck the [kiosk module check](../../../services/kiosk/module-check.nix) and the flake check that registers it.

Resolve with Simon whether the crate is deleted or kept as an internal adapter behind the portal module, which Odin-specific needs become hardware-fact options on the product module, and what evidence proves the converged Odin still boots to the portal. Record decisions, not the port.

## Comments

Source inspection at `7bbd41fb`, 2026-09-20. No builds or device tests were run.

Odin's `nix/devices/odin2portal/web-session.nix:32-57` already supplies compositor hardware facts through `korriLinuxHost`: DRM and render nodes, DSI output, mode, GLES renderer, rotation, and touch mapping. It also mixes in product behavior: the browser app id, game-return exclusion, and browser/Xwayland window rules. `services/kiosk/nixos-module.nix:14-20` adds ANGLE/GLES and GPU flags. The shared portal module accepts `chromiumArgs` and a non-reserved environment, so browser flags are not themselves proof that a second launcher is necessary.

The shared portal uses its own `korri-portal` service user, private Wayland socket access, systemd credentials, and readiness notification (`clients/portal/nix/nixos-module.nix:10-21,195-294`). That service identity is separate from ticket 11's product runtime account. Its Wayland display name currently comes from Sunshine's service environment (`:21`). This dependency needs attention because ticket 13 makes the streaming host removable; it is not an Odin hardware fact.

The actual shared Rust shell already opens an inert app window with a shell-owned `data:` URL and then installs `window.KorriRpc` before navigation (`clients/linux/src/main.rs:191-230,475-536`). Its README still says `about:blank`; use the code, not that stale description. The native app-window test uses X11 (`clients/linux/src/bootstrap_tests.rs:269-307`), not Odin's Wayland compositor. Odin's existing app-id exclusion derives from the old HTTP bootstrap URL, so it cannot be carried forward as a verified identity for the new shell.

The current portal reads only `window.KorriRpc` in production (`clients/portal/src/main.tsx:76`, `src/runtime-config.ts`). The old kiosk delivers an intercepted `/runtime.json` response instead. Its module check asserts old-path wiring and permissions, not successful binding consumption (`services/kiosk/module-check.nix:51-66`); `flake.nix:283` still registers it. Source therefore establishes a producer/consumer mismatch, not the current installed device's behavior.

History separates three concerns. `4b221691` introduced the private-response web session and Odin wiring. `4407c961` added game-return focus behavior. `5dcd724f` added a separate Chromium transparency experiment under `services/kiosk/chromium/` and native pixel tests. The latter README records local x86_64 results, not Odin acceptance. Retiring the launcher crate does not by itself justify deleting those independent patch and test artifacts.

## Answer

Resolved 2026-09-20. Simon selected retirement through `ask_user`, confirmed the existing hardware/product boundary in chat, and approved the acceptance requirements and ticket closure through `ask_user`.

### Launcher

Retire the old kiosk launcher crate and its separate NixOS module. Odin uses the shared portal module and `clients/linux` shell with `window.KorriRpc`. Do not retain an internal adapter, `/runtime.json` fallback, or a second credential-delivery implementation.

Move required Odin integration into the shared product path. Preserve the independent Chromium transparency patch and native pixel tests; retiring the launcher does not delete or certify that work. Remove the retired launcher's package/module/check registrations and obsolete references when implementing the cut. Keep relevant behavioral coverage in the shared path instead of retaining checks for the removed implementation.

The shared portal's separate `korri-portal` service identity remains distinct from the `korri` runtime account chosen in ticket 11. No account migration, device write, or implementation is authorized by this planning decision.

Cost: the shared path must reproduce required Odin rendering, input, and game-return behavior. Its current X11 and headless tests do not establish native Wayland operation on Odin.

### Hardware and product responsibilities

Simon confirmed the application of ticket 07: Odin describes its screen, GPU, and controls; the shared product owns browser startup, security, and game-return behavior. This is the existing product boundary, not a new choice between browser implementations.

Ground the device inputs in `nix/devices/odin2portal/web-session.nix`: DRM and render nodes, output and mode, rotation, renderer, and touch mapping. The existing shared host options already express these needs, including native Sway configuration for rotation and touch mapping. Extract the final product options during `/to-spec`; this decision introduces no option names or schema.

The shared product translates hardware needs into browser flags and compositor rules. It owns the browser window identity and game-return exclusion together. Determine the new shell's actual Wayland app id rather than retaining the old bootstrap URL's id. Preserve required behavior, not every historical flag without evidence.

The portal must obtain its Wayland connection from the compositor, not depend on an installed streaming host. Its current read of Sunshine's environment is an integration dependency to remove under the already-decided removable-plugin boundary. Sunshine capture and encoding contributions remain ticket 13's work. This ticket does not choose a plugin schema or change streaming defaults.

### Convergence acceptance

Implementation must pass shared browser security tests and recorded physical checks on Odin. Configuration evaluation or a successful build alone does not complete the change.

- Shared browser tests verify private credential delivery and refusal to start without consumption of both binding methods. Keep the binding's existing isolation and non-disclosure requirements.
- On Odin, normal boot reaches the portal with correct rendering and touch/controller input. The portal completes an authenticated RPC call to its local korrid; binding consumption alone does not prove that connection works.
- Launch, leave, return, and end work under ticket 09's contract. Returning raises the exact game's window, never the portal or another launch.
- The portal recovers after browser, compositor, and korrid restarts. Restart verification must exercise the converged service wiring, not only a standalone headless browser.
- The portal remains usable with the streaming host removed. Removing Sunshine must not remove the browser's Wayland connection or other required product behavior.

These are acceptance requirements for browser convergence, not evidence that they passed. No numeric performance target or new transparency requirement is added. Full supported-image acceptance and delivery remain governed by tickets 02 and 07.

Cost: physical Odin access and regression testing are required after implementation. The current source tests and historical transparency results cannot replace them. This planning session ran no browser tests, built no runtime artifacts, and made no device changes. It authorizes no deployment or flashing.
