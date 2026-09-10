set -eu
# systemd supplies this plugin's private, persistent state directory.
key="$STATE_DIRECTORY/ssh_host_ed25519_key"
if [ ! -e "$key" ]; then
    # Publish only a complete key. A crash cannot leave a truncated active key.
    temporary="$(@coreutils@/bin/mktemp -d "$STATE_DIRECTORY/.host-key.XXXXXXXX")"
    trap '@coreutils@/bin/rm -rf "$temporary"' EXIT
    @openssh@/bin/ssh-keygen -q -t ed25519 -N '' -C '' -f "$temporary/key"
    @coreutils@/bin/mv "$temporary/key.pub" "$key.pub"
    @coreutils@/bin/mv "$temporary/key" "$key"
fi
# Never replace an existing (possibly damaged) identity without operator action.
@openssh@/bin/ssh-keygen -y -P '' -f "$key" > /dev/null
# Persist the identity before sshd can present it or enablement can commit.
@coreutils@/bin/sync -f "$STATE_DIRECTORY"
