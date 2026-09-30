# Nixpkgs update and Sway 1.12

## State

This is a worktree-only update. Do not fast-forward main or deploy it.
The root nixpkgs lock moves from `a6531044f6d0bef691ea18d4d4ce44d0daa6e816`
to `4975466d324710c576dc11ad614684e6bd8cad8e`, the existing mountainous
revision dated 2026-09-23. Other inputs remain unchanged.

The ibuki consumer builds, but the complete Korri update is not ready.
Sunshine blocks product and handheld checks. Its existing approval gates remain
in force. No source hash, recipe approval, signature, permission, or device
build policy was bypassed.

## Sway port

Sway moves from 1.11 to 1.12 with wlroots 0.20.2. The new source hash is
`sha256-OcF7jOOHhFPhM5APn5riy8S5jsEr9jALCVh9nBtD7Nk=`.

Inspection of `cmd_seat`, `seat_idle_notify_activity`, and the server's native
idle notifier found the same boundary used by the carried command. The patch
changes only its header declaration's line number. It applies with zero fuzz
and no offsets. The package still preserves upstream patches and dependencies.

The real headless check passes. It covers idle/resume, sustained activity,
re-idle, unchanged focus, no injected pointer events, and output power-off/on.
The test now also rejects `idle_notify` in a configuration file and requires
the command's runtime-only error. An unpatched Sway 1.12 negative control fails
with `Unknown/invalid command 'idle_notify'`. This is not physical OLED acceptance.

```sh
nix build --no-link --print-out-paths -L \
  .#checks.x86_64-linux.sway-controller-activity
```

## Mini V2 systemd port

The new nixpkgs supplies systemd 261.2 instead of 258.2. The Mini V2's approved
stop-thaws-unit behavior needs a port before product checks can evaluate.
The patch changes only line numbers and upstream comment spacing.
It applies to 261.2 with zero fuzz and no offsets. The VM test uses the renamed
`copy_from_machine` driver method without disabling type checks.

The ordinary `rpminiv2-systemd-freezer` check remains blocked while it evaluates
the updated product's Sunshine package. A separate VM regression uses the
original product dependency fixture from Korri commit `1c949905`, with the new
patched PID1. This separates PID1 behavior from the Sunshine update. It does
not validate the full updated product or an ARM device.

The isolated VM passed all regressions, all six upstream freezer cases, and
shutdown marker verification in 87.54 seconds. Review also found and fixed a
missing `sleep` path in the upstream watchdog fixture. The final run checks
both suite output and the watchdog journal for missing-command errors. See
`nix/devices/rpminiv2/systemd/README.md` for the result and its limits.

## Verification on Zao

| Check | Result |
|---|---|
| Sway native activity, runtime-only rejection, and output power | Passed. |
| `korri-portal-module` | Passed. |
| `korri-linux-host-module` | Passed. |
| `rg35xxsp` module check | Passed. |
| Mountainous ibuki full system build with local Korri override | Passed. |
| Systemd 261.2 VM against original product dependency fixture | Passed. |
| `korri-product-module`, `korri-product`, and handheld system evaluation | Blocked by package review gates. |
| ARM builds and physical acceptance | Not run. |

Ibuki's built system is
`/nix/store/ivky49dkfd0n1ysx6mrnkmh54i5mnhgp-nixos-system-ibuki-26.11.20260923.4975466`.
It was built without changing mountainous's lock file. This removes the observed
Go 1.25.4 versus sops Go >= 1.26 build failure. It does not activate ibuki or
prove runtime behavior.

From the mountainous `cairn` worktree, the verification command was:

```sh
nix build --no-link --print-out-paths -L --option eval-cache false \
  --no-write-lock-file \
  --override-input korri \
    path:/home/simonwjackson/code/sandbox/korri/.worktree/nixpkgs-sway-112 \
  .#nixosConfigurations.ibuki.config.system.build.toplevel
```

## Sunshine decision still needed

| Source | Sunshine version | Prepared FFmpeg build-deps |
|---|---|---|
| Existing reviewed package | `2025.924.154138` | Commit `2851db101eeddae8f02489d48a52a4d83e6f7e7b` |
| New nixpkgs | `2026.516.143833` | Tag `v2026.516.30821` |

The new Sunshine source hash is
`sha256-3yuhOyW1Rqz4ddZ40z2ZzpAReZQFva0SL595XrnFB60=`.
The first of 16 carried base patches fails in `src/stream.cpp` on the new
source. Later patches were not tested. The current VAAPI/NVENC runtime code
mirrors exact FFmpeg private layouts and guards libavcodec `62.11.100`.
A version/hash-only approval edit would not establish compatibility.

Two paths remain. Keeping Sunshine at its reviewed version narrows this update,
but needs explicit packaging on the new toolchain and fresh recipe approvals.
Porting Sunshine and its FFmpeg boundary expands the update and requires encoder
and runtime-control verification. Neither path removes signed delivery or
hardware acceptance requirements.

No Sunshine recipe or approval metadata changed. Do not silently select a path.

## Local evidence

Evaluation logs and the new-source patch probe are in
`/tmp/korri-nixpkgs-112-evidence/`. Process logs are under
`/tmp/pi-processes-76k6oc/`. These local logs are not a delivery artifact.
The isolated PID1 regression expression is `/tmp/korri-systemd-261-regression.nix`.

A nixpkgs update changes derivations and can need new builds and signed cache
publication. InputPlumber retains its separately pinned nixpkgs. No device was
contacted, no storage was written, and no system was activated.
