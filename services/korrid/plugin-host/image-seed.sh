set -euo pipefail

if (( $# < 4 )); then
  echo 'usage: korri-image-seed FILES_ROOT KORRI_PLUGIN PUBLISHER_BINDINGS PACKAGE...' >&2
  exit 2
fi
files=$1
host=$2
publishers=$3
shift 3

# Validate the whole exact selection before writing any receipt. The plugin
# host owns dependency ordering, approvals, receipts and unit-name policy.
receipts="$("$host" seed-graph "$publishers" "$@")"
# Preflight destinations too: an existing selection must not leave a partly
# seeded image. Only the existing Receipt schema crosses this boundary.
while IFS= read -r receipt; do
  id=$(printf '%s' "$receipt" | jq -er '.id')
  name="$("$host" unit-name "$id")"
  state="$files/var/lib/korri-plugin-host/$name"
  roots="$files/nix/var/nix/gcroots/korri-plugin-host/$name"
  if [[ -e "$state/selection.json" || -L "$state/selection.json" || -e "$roots/active" || -L "$roots/active" ]]; then
    echo "duplicate seeded plugin: $id" >&2
    exit 1
  fi
done < <(printf '%s' "$receipts" | jq -c '.[]')

while IFS= read -r receipt; do
  id=$(printf '%s' "$receipt" | jq -er '.id')
  package=$(printf '%s' "$receipt" | jq -er '.package')
  name="$("$host" unit-name "$id")"
  state="$files/var/lib/korri-plugin-host/$name"
  roots="$files/nix/var/nix/gcroots/korri-plugin-host/$name"
  install -d -m 0700 "$state" "$roots"
  printf '%s\n' "$receipt" > "$state/selection.json"
  chmod 0600 "$state/selection.json"
  ln -s "$package" "$roots/active"
done < <(printf '%s' "$receipts" | jq -c '.[]')
