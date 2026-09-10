# Optional administrative OpenSSH

`packages.<system>.korri-plugin-ssh` supplies `@korri:ssh`. After one compatible
host update, the existing root CLI installs, enables and disables it without a
NixOS rebuild, system switch, or device-side compilation. It is not published
or enabled by default. Publisher composition can re-export this core output.

**Local owner enrollment and approval UI do not exist yet.** This implements
the root CLI path, not that missing consumer. An SSH-disabled image with no
other administrator access still needs a separately approved local owner path.
Do not promise that the image meets the full parked SSH acceptance item yet.

## Authority and coexistence

| Native request | Effect |
|---|---|
| `User=root` | Device-wide authority, named in the approval report. No dynamic-user sandbox. This applies only to the exact approved build, not to other plugins. |
| `Type=notify` | Wait for the pinned OpenSSH daemon's readiness notification. A failed bind does not commit enablement. |
| `ExecStartPre` | Generate and synchronize a per-device Ed25519 host key in the host's existing private `StateDirectory`, before listening. Never overwrite an existing identity. |
| Native NixOS `services.openssh.ports = [ 2222 ]` | A deliberate separate TCP listener. Generate the plugin's firewall metadata from this same option. No UDP port and no plugin rule for TCP 22. |
| Native `sshd_config` | Public-key authentication only. Password, keyboard-interactive and empty-password authentication are disabled. Root requires a valid key, even on a device with root console autologin. |

The plugin never writes `/etc/ssh/sshd_config`, recovery host keys, or recovery
units. It supplies its own immutable configuration with `-f`, its own persistent
key with `-h`, and its own runtime PID file. It does not include an existing
sshd configuration or authorized-keys command. It preserves upstream account
key paths: `%h/.ssh/authorized_keys` and `/etc/ssh/authorized_keys.d/%u`.
Existing account authorization files remain device-owned. No private host key,
personal public key, password, or new account credential is packaged.

The compatible host supplies the upstream `sshd` privilege-separation account
and PAM account/session support when NixOS recovery SSH is disabled. NixOS base
activation already supplies the `/var/empty` privilege-separation chroot; the
host-support check also verifies that prerequisite. When it
is enabled, the host leaves its account/PAM configuration alone. The plugin
uses that existing PAM policy; account expiry and host login restrictions can
still deny a key. It neither edits accounts nor enrolls keys.

Disable removes the plugin's declared port rules and stops its managed daemon.
Re-enable and boot recovery retain the same host key. `remove` retains data;
`remove --purge` deletes the plugin state and therefore its host identity. A
later installation creates a new identity and clients must verify it again.
A damaged existing key fails activation rather than silently rotating it.

**Cost:** this is a privileged administrative service, not a confined network
capability. Root sessions can change the whole device. Disable does not undo
those changes or guarantee termination of sessions moved by PAM into separate
systemd scopes. TCP 2222 must be free. Changing to TCP 22 later is a separate
native configuration/publication and recovery-listener cutover, not an
implicit fallback. No physical device or recovery listener was changed here.

## Native sources and upstream extraction

`plugin.ts` only names the plugin and service. `sshd.service`, `sshd_config`,
`prepare.sh`, and `start.sh` own native behavior. `plugin.nix` substitutes
immutable prebuilt program/configuration paths and uses the shared builder.
The host's existing immutable-artifact contract requires files **inside**
store output directories, not bare `writeText` store files.

`upstream.nix` evaluates the pinned NixOS OpenSSH module on the build machine.
It extracts native configuration, account key paths, algorithm defaults,
immutable SFTP/moduli paths, and the listener's firewall metadata. The native
key-only policy precedes module defaults because sshd uses the first value.
The module's password defaults are suppressed, not maintained twice.

The trigger for extraction is the second daemon, not a generic new module
language. `upstream-effects.nix` records the disposition of all 15 contributed
option paths. A new top-level contribution fails the extraction check until
reviewed. The generated report contains actual configuration/unit paths,
privilege-separation account facts, PAM text, root-provisioning effects and
NSS path. Nested option changes still need source review. The report and
upstream boot units are build-side only; they are not in the plugin closure.

| Upstream effect | Treatment |
|---|---|
| Native config and config validation | Extract; validate with prebuilt build-platform `sshd -G -T`. Use immutable package moduli, not existing `/etc/ssh/moduli`. |
| `sshd` user/group; PAM account/session | Preserve through the compatible host prerequisites. Never replace existing recovery PAM. |
| `/etc/ssh` configuration and authorization files | Do not install. Read existing account authorization files only. |
| Root tmpfiles credential provisioning | Do not extract. There is no supplied owner credential and no enrollment consumer. |
| Host key generation | Replace `/etc/ssh` boot keygen with native pre-start generation in `StateDirectory`. Keys are never immutable build outputs. |
| Boot units and socket activation | Do not install. One optional host-managed native service replaces them. `notify-reload` becomes `notify`; the CLI does not expose reload. |
| `KillMode=process` | Retain Korri's existing `control-group` stop policy instead. Root/PAM effects outside that group are not revocable by this policy. |
| NSS command environment | Existing device accounts resolve through the host's NSS/nscd. No build-machine NSS environment is injected. External directory-service accounts are not verified by this slice. |

Grounding inspected: nixpkgs `a6531044f6d0bef691ea18d4d4ce44d0daa6e816`,
`nixos/modules/services/networking/ssh/sshd.nix`, OpenSSH 10.2p1, the existing
plugin-host `NativeUnit`, `StateDirectory`, `RuntimeDirectory`, approval digest,
and firewall port treaty. systemd 258's `exec_context_get_clean_directories`
removes both public and private state paths, so existing inactive purge also
covers root-service state without a new persisted layout.

## Operator commands

Use the owner's already configured publisher binding and the exact prebuilt
output for the device architecture. Inspect all root authority before copying
the digest; installation alone leaves the plugin disabled.

```sh
sudo korri-plugin inspect "$CACHE_URL" "$PACKAGE"
sudo korri-plugin install "$CACHE_URL" "$PACKAGE" "$APPROVAL"
sudo korri-plugin enable @korri:ssh
ssh -p 2222 ACCOUNT@DEVICE
sudo korri-plugin disable @korri:ssh
```

No enrollment command is implied. Existing account keys must already be
owner-managed. Inspect and verify the device's generated host public key via
an existing trusted administrator path before trusting the new listener.

## Verification (build machine only)

```sh
PACKAGE=$(nix build --no-link --print-out-paths .#korri-plugin-ssh)
nix build --no-link .#checks.x86_64-linux.korri-ssh-upstream \
  .#checks.x86_64-linux.korri-ssh-host-support
KORRI_TEST_NIX=$(command -v nix) KORRI_TEST_SSH_PACKAGE="$PACKAGE" \
  nix develop .#plugin-host --command cargo test --locked \
  --manifest-path services/korrid/plugin-host/Cargo.toml --test native_ssh -- --ignored
sudo "$(command -v python3)" plugins/ssh/process-test.py "$PACKAGE"
```

The last command requires an existing NixOS `sshd` account/PAM setup on the
build machine. It runs only a temporary loopback daemon on a random high port,
with test-only keys under `/run/korri-ssh-process-*`. It does not edit host
accounts, authorization files, firewall rules, units or configuration. It
checks the actual prebuilt daemon's readiness, key-only root login, PTY,
wrong/no-key rejection, key uniqueness, restart identity, listener stop,
occupied-port failure, and damaged-key refusal.

Verified locally: the above package/admission/process checks and the full
Rust test, clippy and formatter scope. The host-support check evaluates the
actual module with SSH disabled and compares recovery configuration, PAM,
keygen, service policy and firewall options before/after host support. The
existing Tailscale fixture's builder selects the same store output before and
after this change. Test-first repro: bare store-file
artifacts failed actual `package::load` with `store artifact must name a file`;
packaging now uses native directory outputs. A process-test fixture under
`/tmp` failed OpenSSH StrictModes; the fixture now uses root-owned `/run`
without weakening authorization checks.

**Integrated VM assertions are added but not run in this slice.** The parent
runs this single final gate (never on a target device):

```sh
nix build --no-link -L .#checks.x86_64-linux.korri-runtime-plugin-host
```

It covers the initially SSH-disabled host, root and ordinary account keys,
refused approval and keys, enable/disable firewall rules, enabled/disabled
reboot recovery, damaged-key activation rollback, purge, and coexistence with
the real upstream NixOS recovery daemon. The recovery unit PID, configuration,
PAM file, host keys and system generation must remain unchanged. Systemd and
firewall lifecycle remain unverified until that gate passes. Physical-device
acceptance remains a separate, unrun gate.
