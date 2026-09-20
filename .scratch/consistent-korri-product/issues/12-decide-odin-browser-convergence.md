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
