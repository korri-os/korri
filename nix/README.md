# Nix composition

| Directory | Owns |
| --- | --- |
| `product/` | The complete shared Korri product composition, its fixed account and constants, and the evaluation gate every exported device must pass. |
| `base/` | Product-wide recovery access, NetworkManager policy, and optional governor and fan modules. It chooses neither a board nor an image format. |
| `device-cache/` | The no-local-build device substitution policy composed by the product. |
| `formats/` | Image assembly, optional WiFi file staging, and first-boot storage expansion. |
| `devices/` | Board hardware facts, boot chain, firmware, input maps, and explicit hardware limits only. |

A normal device imports `product/nixos-module.nix` once and then adds one image
format and its hardware module. Device modules do not assemble, remove, swap, or
force-disable product services. The product currently composes the Linux host,
local portal and browser, plugin host, cache policy, and fixed `korri` runtime
account. Product constants such as relays, surface selection, and korrid's bind
address live with that composition rather than in a board module.

Services keep their Nix modules beside their implementations under `services/`
and `clients/`. The product composes the shared streaming-free
`services/inputd/nix/korri-linux-host-core.nix`; the temporary native streaming
host composes the same substrate with its separate Sunshine integration. Both
consume the one product-agnostic compositor socket and readiness contract beside
the host implementation. A streaming host, its certificate socket, firewall
rules, encoders, and capture packages are plugin concerns. Native units emitted
by product modules remain implementation details: for each target system, the
gate derives required units, exact product executable identities, emission
strategies, suppression state, and required `systemd.packages` contributions
from the delta between a bare NixOS reference and the real product composition.
Device-specific package additions remain allowed; there is no copied manifest
of NetworkManager, nginx, PipeWire, or other native units.

`base/default.nix` remains independently evaluable and imports no device or
image format. Governor and fan modules remain opt-in. `flake.nix` composes the
exported configurations and retains their public output names. Historical work
and acceptance records may still name old paths such as `nix/rg353m/`; current
board files live under `devices/`, and shared image behavior lives under
`formats/`.

## Hardware limits

A board module may record a limit only where observed hardware requires it. For
example, RG353M carries its thermal CPU ceiling and the unaccepted Chromium GPU
path locally; `--disable-gpu` is not universal product policy. Such limits must
not turn into alternate product composition.

Bootloaders, partition labels, image compression, console order, kernel and
device-tree choices, radio settings, and physical input data remain device
facts. The runtime account and product services do not. InputPlumber data keeps
its bytes and uses the shared installed-schema validation.

SSH is disabled by product policy and no operator SSH key is included. Physical
recovery consoles, including RG353M USB serial, remain enabled and grant root.
Owner-controlled SSH is a plugin concern and requires explicit approval.

See [device image builds](formats/IMAGE-BUILDS.md) for the manual GitHub Actions
workflow and its optional, explicitly unverified prereleases. No x86 target or
ISO builder is added here; hardware architecture and media format remain
separate choices.

## Checks

`checks.<system>.korri-product-module` evaluates the product independently,
forces each contractual setting and every derived system or user unit away from
its required final value in a real extended NixOS configuration, and verifies
the complete exact gate failure set. Unit cases cover disablement, activation
links, conditions and assertions, socket listeners, drop-in strategy, package
contributions, generated-hook identity, and the complete exact systemd
`serviceConfig` authority/runtime/sandbox contract. Hardware paths and other
device facts remain service environment or data values, outside executable and
authority identity. The internal product marker improves diagnostics only: Nix
evaluation metadata can
be forged, so the non-security guarantee is the complete resulting behavioral
contract. A marker-only lookalike fails that contract; reproducing the entire
contract is behaviorally equivalent. `checks.<system>.korri-product` enumerates every exported
`nixosConfiguration`; there is no per-device opt-out list. During the staged
migration it intentionally remains red for devices that have not adopted the
product module. A passing evaluation gate records composition, not hardware
support.

Image and hardware checks remain focused under their device and format outputs.
They evaluate only; no check deploys, flashes, or writes to a device. WiFi
staging continues to use `/etc/korri/wifi.env` with `WIFI_SSID` and `WIFI_PSK`.
Use test values, never a real network credential, in fixtures.
