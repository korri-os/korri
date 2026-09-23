set -euo pipefail

if (( $# < 4 )); then
  echo 'usage: korri-image-seed FILES_ROOT KORRI_PLUGIN CACHE_URL PACKAGE...' >&2
  exit 2
fi
files=$1
host=$2
cache=$3
shift 3

# The plugin host owns the receipt format and unit-name policy. The SD image
# stores only the resulting receipt and GC root, not another plugin schema.
for package in "$@"; do
  receipt="$("$host" seed "$package" "$cache")"
  id=$(printf '%s' "$receipt" | jq -er '.id')
  selected=$(printf '%s' "$receipt" | jq -er '.package')
  test "$selected" = "$package"
  name="$("$host" unit-name "$id")"
  state="$files/var/lib/korri-plugin-host/$name"
  roots="$files/nix/var/nix/gcroots/korri-plugin-host/$name"
  if [[ -e "$state/selection.json" || -L "$roots/active" ]]; then
    echo "duplicate seeded plugin: $id" >&2
    exit 1
  fi
  install -d -m 0700 "$state" "$roots"
  printf '%s\n' "$receipt" > "$state/selection.json"
  chmod 0600 "$state/selection.json"
  ln -s "$package" "$roots/active"
done
