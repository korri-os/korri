set -eu
# systemd supplies this plugin's private, persistent state directory.
key="$STATE_DIRECTORY/ssh_host_ed25519_key"
refuse() {
    echo "Unsafe SSH host-key state: $1; operator repair required" >&2
    exit 1
}
# Root-only directory access makes the checks and publication one protected
# operation. Never follow or replace an existing key directory entry.
[ -d "$STATE_DIRECTORY" ] || refuse "$STATE_DIRECTORY"
[ "$(@coreutils@/bin/stat -Lc '%u:%a' "$STATE_DIRECTORY")" = '0:700' ] || refuse "$STATE_DIRECTORY"
validate() {
    path="$1"
    allowed="$2"
    [ ! -L "$path" ] && [ -f "$path" ] || refuse "$path"
    [ "$(@coreutils@/bin/stat -c '%u' "$path")" = 0 ] || refuse "$path"
    mode="$(@coreutils@/bin/stat -c '%a' "$path")"
    [ "$((0$mode & ~allowed))" = 0 ] || refuse "$path permissions"
}
if [ -e "$key.pub" ] || [ -L "$key.pub" ]; then
    validate "$key.pub" 0644
fi
if [ -e "$key" ] || [ -L "$key" ]; then
    validate "$key" 0600
else
    # A remaining public key is evidence of an earlier identity, not absence.
    [ ! -e "$key.pub" ] || refuse "$key.pub without private key"
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
