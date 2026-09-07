#!/usr/bin/env nix
#! nix shell nixpkgs#bash nixpkgs#coreutils nixpkgs#findutils nixpkgs#diffutils nixpkgs#nix nixpkgs#curl nixpkgs#python3 nixpkgs#util-linux --command bash
set -euo pipefail
# Exercise Nix's real profiles and a real HTTP server. Only the subprocess
# service outcome is configured, as in the native bundle selector tests.
source_dir=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
server_pid=
cleanup() {
  if [ -n "$server_pid" ]; then kill "$server_pid"; wait "$server_pid" 2>/dev/null || true; fi
  rm -rf "$tmp"
}
trap cleanup EXIT
mkdir -p "$tmp/a/assets" "$tmp/b/assets" "$tmp/invalid"
printf '<html><script src="./assets/app.js"></script><link href="./assets/app.css" rel="stylesheet">A</html>\n' > "$tmp/a/index.html"
printf 'console.info("A")\n' > "$tmp/a/assets/app.js"
printf 'body {}\n' > "$tmp/a/assets/app.css"
cp -r "$tmp/a/." "$tmp/b/"
printf '<html><script src="./assets/app.js"></script><link href="./assets/app.css" rel="stylesheet">B</html>\n' > "$tmp/b/index.html"
a=$(nix-store --add "$tmp/a")
b=$(nix-store --add "$tmp/b")
invalid=$(nix-store --add "$tmp/invalid")
for title in c d; do
  cp -r "$tmp/a" "$tmp/$title"
  printf '<html>%s</html>\n' "$title" > "$tmp/$title/index.html"
done
c=$(nix-store --add "$tmp/c")
d=$(nix-store --add "$tmp/d")
# Use the installed systemctl CLI shape with controlled subprocess outcomes.
printf '#!/usr/bin/env bash\nif [ -e %q ]; then exit 1; fi\nexit 0\n' "$tmp/fail-service" > "$tmp/systemctl"
chmod +x "$tmp/systemctl"
ln -s "$tmp/profile" "$tmp/served"
python3 "$source_dir/test-server.py" "$tmp/served" "$tmp/port" &
server_pid=$!
for attempt in $(seq 1 100); do
  [ ! -f "$tmp/port" ] || break
  sleep 0.05
done
[ -f "$tmp/port" ] || { echo "HTTP server did not start after $attempt attempts" >&2; exit 1; }
read -r port < "$tmp/port"
select_bundle() { bash "$source_dir/select.sh" "$tmp/profile" "http://127.0.0.1:$port/" "$tmp/systemctl" "$@"; }
select_bundle initialize "$a"
[ "$(readlink -f "$tmp/profile")" = "$a" ]
select_bundle initialize "$b"
[ "$(readlink -f "$tmp/profile")" = "$a" ]
select_bundle switch "$b"
[ "$(curl -fsS "http://127.0.0.1:$port/")" = "$(< "$b/index.html")" ]
select_bundle rollback
[ "$(readlink -f "$tmp/profile")" = "$a" ]
if select_bundle switch "$invalid"; then echo 'Accepted an invalid bundle' >&2; exit 1; fi
[ "$(readlink -f "$tmp/profile")" = "$a" ]
touch "$tmp/fail-service"
if select_bundle switch "$b"; then echo 'Accepted a failed restart' >&2; exit 1; fi
[ "$(readlink -f "$tmp/profile")" = "$a" ]
rm "$tmp/fail-service"
# A failed update must not replace the known selection at the next initialize.
select_bundle initialize "$b"
[ "$(readlink -f "$tmp/profile")" = "$a" ]
select_bundle switch "$b"
# Successful service restart with stale HTTP must also restore selection.
ln -sfn "$a" "$tmp/served"
if select_bundle switch "$c"; then echo 'Accepted stale HTTP content' >&2; exit 1; fi
[ "$(readlink -f "$tmp/profile")" = "$b" ]
ln -sfn "$tmp/profile" "$tmp/served"
# B -> rejected C -> accepted D -> rollback must reach B, never rejected C.
select_bundle switch "$d"
select_bundle rollback
[ "$(readlink -f "$tmp/profile")" = "$b" ]
select_bundle status
printf 'PASS: initialize, update, rollback, invalid bundle, failed restart and stale HTTP\n'
