# Nix composition

| Directory | Owns |
| --- | --- |
| `base/` | Shared recovery access, NetworkManager policy, optional governor and fan modules, and InputPlumber data packaging. |
| `formats/` | SD image assembly, optional WiFi file staging, and first-boot expansion. |
| `devices/` | Each board's kernel, firmware, boot chain, radio settings, input maps, and service composition. |

`base/default.nix` is the shared configuration entrypoint. It imports no device
or image format. Governor and fan modules remain opt-in. Services keep their
Nix modules beside their implementations under `services/` and `clients/`.
`flake.nix` composes them and retains the existing public output names.
Historical work and acceptance records still name `nix/rg353m/` and
`nix/odin2portal/`. Board files now live under `devices/`; their shared
`expand-root.nix` lives under `formats/`.

Both device configurations import the base and the SD format. The format's
`gpt` argument preserves the existing MBR path on RG353M and GPT repair on Odin.
First-boot expansion belongs to the format, not the base. Do not run GPT repair
on an MBR card based on the result of one test image.

Device modules use existing NixOS options. They add radio settings through
`networking.networkmanager.ensureProfiles.profiles.korri.wifi` and boot files
through `sdImage.populateRootCommands`. That option merges shell fragments;
there is no separate Korri boot-population option.

## What does not change

Bootloaders, partition labels, image compression, console order, runtime users,
and hardware settings remain device-specific and unchanged. InputPlumber data
keeps its bytes; both packages now use the same installed-schema check. RG353M
also keeps its physical-button assertions.

SSH is disabled in the base and no operator SSH key is included. Physical
recovery consoles, including RG353M USB serial, remain enabled and grant root.
First-boot setup and plugin support remain separate work. A future SSH plugin
requires explicit owner approval and compatible plugin-host support.

See [device image builds](formats/IMAGE-BUILDS.md) for the manual GitHub Actions
workflow and its optional, explicitly unverified prereleases.

## Future output formats

No x86 target or ISO builder is added here. Hardware architecture and media
format remain separate choices. Legacy reference files are
`product/systems/nixos/images/platforms/x86.nix` and
`product/systems/nixos/images/live-usb.nix` on `legacy`. The latter builds a live
USB/ISO appliance, not an installer that writes an internal disk. Reuse only the
parts required by the first real x86 case.

## Checks

Run `nix run .#nixos-layout-check` on a Linux build machine. It evaluates the
base independently of any format, checks both SD expansion paths, validates
both devices' input data, tests invalid data rejection, and runs the USB gadget
and WiFi checks. It does not deploy or write to devices.

WiFi continues to use `/etc/korri/wifi.env` with `WIFI_SSID` and `WIFI_PSK`.
`KORRI_WIFI_ENV=/absolute/path/to/test.env nix run .#nixos-layout-check` also
supplies a specific fixture for the staging check. Without that variable, the
task creates and removes a temporary fixture. Use test values, not a real
network credential.
