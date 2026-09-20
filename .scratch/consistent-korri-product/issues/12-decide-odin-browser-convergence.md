# Decide the Odin browser path convergence

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
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

Pending Simon's decisions.
