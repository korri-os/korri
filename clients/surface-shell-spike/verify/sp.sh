#!/usr/bin/env bash
# THROWAWAY CLI ONLY. Requires a bundle built on fuji/yari. No services touched.
set -euo pipefail
archive=$(realpath "$1")
out=$(realpath -m "$2")
host=root@10.42.5.1
part=${SURFACE_VERIFY_PART:-all}
case "$part" in all|installed|large) ;; *) echo "invalid SURFACE_VERIFY_PART" >&2; exit 2 ;; esac
mkdir -p "$out"
# Check the real recursive game cgroup, not a service's reported state.
ssh -o ConnectTimeout=10 -p 2222 "$host" '
  if test -d /sys/fs/cgroup/korri-games.slice; then
    pids=$(find /sys/fs/cgroup/korri-games.slice -name cgroup.procs -exec cat {} \;)
    if test -n "$pids"; then echo "SKIP: game running: $pids"; exit 3; fi
  fi
  echo "PASS: korri-games.slice absent or recursively empty"
' | tee "$out/game-gate.txt"
ssh -p 2222 "$host" 'systemctl show korri-chromium-kiosk.service korri-compositor.service inputplumber.service -p Id -p ActiveState -p MainPID -p NRestarts' > "$out/services-before.txt"
remote=/tmp/surface-verify-$(date +%s)-$$
ssh -p 2222 "$host" "mkdir '$remote'"
cleanup() { ssh -p 2222 "$host" "rm -rf '$remote'" || :; }
trap cleanup EXIT
scp -P 2222 "$archive" "$host:$remote/bundle.tar.gz"
# Copy the launcher to /tmp too. No compilation, service operations, display or input.
ssh -p 2222 "$host" "tar -xzf '$remote/bundle.tar.gz' -C '$remote'; uname -a; free -h; df -h /tmp" > "$out/machine.txt"
name=$(tar -tzf "$archive" | sed -n '1{s,/.*,,;p;}' )
# Keep the remote command streaming so disconnect/cleanup cannot hide unfinished work.
ssh -p 2222 "$host" "
 set -eu
 ulimit -c 0
 cd '$remote/$name'
 mkdir -p '$remote/tmp' '$remote/home' '$remote/cache' '$remote/config'
 export TMPDIR='$remote/tmp' HOME='$remote/home' XDG_CACHE_HOME='$remote/cache' XDG_CONFIG_HOME='$remote/config'
 printf '<fontconfig><cachedir>%s/cache/fontconfig</cachedir></fontconfig>\n' '$remote' > '$remote/config/fonts.conf'
 export FONTCONFIG_FILE='$remote/config/fonts.conf'
 export SURFACE_VERIFY_JOBS=4
 export SURFACE_VERIFY_LAUNCHER=\"\$PWD/run.sh\"
 if test -d /sys/fs/cgroup/korri-games.slice; then
   test -z \"\$(find /sys/fs/cgroup/korri-games.slice -name cgroup.procs -exec cat {} \\;)\"
 fi
 if test '$part' != large; then
 for n in 10 100 1000 5000; do
   nice -n 10 ./run.sh many \"\$PWD/bin\" \"\$n\" > '$remote/many-'\"\$n\"'.jsonl' 2> '$remote/many-'\"\$n\"'.log'
 done
 fi
 if test '$part' != installed; then
 nice -n 10 ./run.sh large \"\$PWD/bin\" '$remote/fixtures' > '$remote/large.jsonl' 2> '$remote/large.log'
 fi
" & ssh_pid=$!
wait "$ssh_pid"
scp -P 2222 "$host:$remote/*.jsonl" "$host:$remote/*.log" "$out/"
ssh -p 2222 "$host" 'systemctl show korri-chromium-kiosk.service korri-compositor.service inputplumber.service -p Id -p ActiveState -p MainPID -p NRestarts' > "$out/services-after.txt"
# The trap removes only this run's /tmp directory, including generated fixtures.
