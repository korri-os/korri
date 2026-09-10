set -eu
exec @openssh@/bin/sshd -D -e -f @config@ \
    -h "$STATE_DIRECTORY/ssh_host_ed25519_key" \
    -o "PidFile=$RUNTIME_DIRECTORY/sshd.pid" "$@"
