#!/usr/bin/env nix
#! nix shell nixpkgs#bash nixpkgs#nix nixpkgs#coreutils nixpkgs#findutils nixpkgs#diffutils nixpkgs#curl nixpkgs#util-linux --command bash
set -euo pipefail
# The package wrapper supplies these from runtime-policy.nix. Tests supply a
# temporary Nix profile and a controlled service subprocess through this seam.
profile=$1
url=$2
systemctl=$3
shift 3
action=${1:-status}
shift "$(( $# > 0 ? 1 : 0 ))"
mkdir -p "$(dirname "$profile")"
exec 9>"$profile.deploy-lock"
flock 9

validate() {
  local bundle
  bundle=$(readlink -f "$1")
  if [[ ! "$bundle" =~ ^/nix/store/[a-z0-9]{32}-[^/]+$ ]] ||
    [ ! -s "$bundle/index.html" ] || [ ! -d "$bundle/assets" ]; then
    echo 'Expected an immutable portal bundle with index.html and assets/.' >&2
    return 1
  fi
  [ -n "$(find "$bundle/assets" -type f -name '*.js' -print -quit)" ] &&
    [ -n "$(find "$bundle/assets" -type f -name '*.css' -print -quit)" ]
}

restart_and_check() {
  "$systemctl" restart korri-chromium-kiosk.service || return 1
  local attempt
  for attempt in 1 2 3; do
    if "$systemctl" is-active --quiet nginx.service &&
      "$systemctl" is-active --quiet korri-chromium-kiosk.service &&
      curl --fail --silent --show-error --max-time 5 "$url" | cmp -s "$profile/index.html" -; then
      return 0
    fi
    echo "Portal readiness attempt $attempt failed." >&2
    sleep 1
  done
  return 1
}

case "$action" in
  initialize)
    [ "$#" -eq 1 ]
    if [ -L "$profile" ]; then
      validate "$profile"
    elif [ -e "$profile" ]; then
      echo 'Refusing to replace a non-profile path.' >&2
      exit 1
    else
      validate "$1"
      nix-env --profile "$profile" --set "$(readlink -f "$1")"
    fi
    ;;
  switch|rollback)
    validate "$profile"
    # Nix owns profile generations, their GC roots and atomic selection. Keep
    # the exact generation so failure recovery never selects a failed update.
    link=$(readlink "$profile")
    basename=$(basename "$profile")
    generation=${link##*/}
    generation=${generation#"$basename"-}
    generation=${generation%-link}
    [[ "$generation" =~ ^[0-9]+$ ]]
    prior_generations=( "$profile"-[0-9]*-link )
    if [ "$action" = switch ]; then
      [ "$#" -eq 1 ]
      validate "$1"
      nix-env --profile "$profile" --set "$(readlink -f "$1")"
    else
      [ "$#" -eq 0 ]
      nix-env --profile "$profile" --rollback
    fi
    if ! restart_and_check; then
      failed_link=$(readlink "$profile")
      failed_generation=${failed_link##*/}
      failed_generation=${failed_generation#"$basename"-}
      failed_generation=${failed_generation%-link}
      [[ "$failed_generation" =~ ^[0-9]+$ ]]
      existed=false
      for prior in "${prior_generations[@]}"; do
        if [ "$prior" = "$profile-$failed_generation-link" ]; then existed=true; fi
      done
      echo 'Portal activation failed; restoring the previous selection.' >&2
      nix-env --profile "$profile" --switch-generation "$generation"
      # A rejected new generation must never become a later rollback target.
      # Do not remove an older, previously accepted generation if its restart
      # failed because of a transient device problem.
      if [ "$existed" = false ]; then
        nix-env --profile "$profile" --delete-generations "$failed_generation"
      fi
      if ! restart_and_check; then
        echo 'Selection restored, but portal services still need attention.' >&2
      fi
      exit 1
    fi
    ;;
  status)
    [ "$#" -eq 0 ]
    validate "$profile"
    ;;
  *)
    echo 'Usage: korri-portal-select initialize|switch BUNDLE; rollback; status' >&2
    exit 2
    ;;
esac
printf 'active=%s\n' "$(readlink -f "$profile")"
