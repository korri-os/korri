# Review inventory, not device configuration. A new module contribution must
# receive an explicit disposition before this build-side extraction can pass.
{
  assertions = "Pinned module assertions are checked at build time.";
  "environment.etc" =
    "Extract sshd_config only; use package moduli. Never write /etc/ssh or account keys.";
  "networking.firewall.allowedTCPPorts" =
    "Generate plugin ports from the same upstream listener option.";
  "security.pam.services" =
    "Compatible host supplies sshd account/session PAM only when recovery sshd is disabled; preserve existing PAM otherwise.";
  "services.openssh.authorizedKeysFiles" =
    "Preserve upstream home and /etc/ssh/authorized_keys.d account paths; no enrollment or key provisioning.";
  "services.openssh.extraConfig" =
    "Preserve generated native configuration, with the native key-only policy first.";
  "services.openssh.moduliFile" = "Reference immutable package moduli directly, not /etc/ssh/moduli.";
  "services.openssh.settings" =
    "Preserve upstream defaults after the native key-only policy; suppress password defaults owned by that file.";
  "services.openssh.sftpServerExecutable" = "Preserve immutable upstream sftp-server path.";
  "system.checks" = "Run the pinned prebuilt build-platform sshd configuration validator.";
  "systemd.services" =
    "Replace boot units with one native optional service and ExecStartPre per-device keygen; keep host control-group stop semantics and runtime/state directories. Host NSS supplies existing device accounts.";
  "systemd.sockets" = "No socket activation; startWhenNeeded is false.";
  "systemd.tmpfiles.settings" =
    "Do not extract root credential provisioning; no owner key or credential is available. Host StateDirectory/RuntimeDirectory replace daemon directory effects.";
  "users.groups" =
    "Compatible host supplies the upstream sshd privilege-separation group if recovery sshd is disabled.";
  "users.users" =
    "Compatible host supplies the upstream sshd privilege-separation user if recovery sshd is disabled. Existing login accounts remain host-owned.";
}
