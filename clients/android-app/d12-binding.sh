#!/usr/bin/env bash
# D12's owner binding (#434): the debuggable Android app on the bench phone,
# bound to the owner's real person key, as the holder proof binds the
# emulator's app to the test owner (proof/holder.py). Run it through its Nix
# app on zao, in a checkout:
#
#   nix run .#android-app-d12-binding -- make WORK
#   nix run .#android-app-d12-binding -- sign-debug UNSIGNED.apk SIGNED.apk
#   adb install --no-incremental SIGNED.apk      # do not start the app yet
#   nix run .#android-app-d12-binding -- seed WORK [--serial SERIAL]
#
# make: korrid's own producer makes the phone's device key in WORK/state
# (`korrid identity owner-binding-request`, DeviceIdentity::load_or_create)
# and prints the owner statement template for it. The owner's signer signs
# that template over NIP-46: the bunker address is read from a file of mode
# 0600 or from the terminal, never from an argument, and goes to nak only in
# its environment. The signed statement must be the template exactly, by
# the owner (a80db13f…3dca unless --owner says otherwise); korrid then imports
# it (`korrid identity import`), which verifies the signature and stores
# identity/owner.event.json beside identity/device.key.
#
# seed: writes those two files into the installed app's private state with
# run-as, which only a debuggable APK allows, before the app's first start;
# then deletes WORK/state, so the phone's device key stays on the phone only.
set -euo pipefail

package="com.simonwjackson.korri.app"
identity="no_backup/korrid-state/identity"
# The owner's person key: the author of the owner statements of the owner's
# devices on the product relays, zao's among them (README, D12 prep).
owner="a80db13fd36d6e15cb45df12db658da04dd6dc7c6d9200dcebabdcb207ed3dca"

usage() {
  echo "usage: d12-binding.sh make WORK [--owner KEY] [--bunker-file FILE]" >&2
  echo "       d12-binding.sh sign-debug UNSIGNED.apk SIGNED.apk" >&2
  echo "       d12-binding.sh seed WORK [--serial SERIAL]" >&2
  exit 2
}

fail() {
  echo "d12-binding: $*" >&2
  exit 1
}

: "${KORRID:?run through nix run .#android-app-d12-binding}"
: "${KORRI_APP_BUILD_TOOLS:?run through nix run .#android-app-d12-binding}"

# WORK, or the nearest folder above it that exists, must not be inside a Git
# work tree: WORK holds the phone's device key until seed deletes it.
outside_git() {
  local path
  path="$(realpath -m "$1")"
  while [[ ! -e "$path" ]]; do path="$(dirname "$path")"; done
  if [[ "$(env -u GIT_DIR -u GIT_WORK_TREE git -C "$path" rev-parse --is-inside-work-tree 2> /dev/null)" == true ]]; then
    fail "$1 is inside a Git work tree; keep it outside every checkout"
  fi
}

make_binding() {
  [[ $# -ge 1 ]] || usage
  local work="$1" bunker_file="" template signed state
  shift
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --owner) [[ $# -ge 2 ]] || usage; owner="$2"; shift 2 ;;
      --bunker-file) [[ $# -ge 2 ]] || usage; bunker_file="$2"; shift 2 ;;
      *) usage ;;
    esac
  done
  [[ "$owner" =~ ^[0-9a-f]{64}$ ]] || fail "the owner must be 64 lowercase hexadecimal characters"
  [[ ! -e "$work" ]] || fail "$work exists; make takes a new folder"
  outside_git "$work"
  # The bunker address first, so that a wrong one leaves no device key.
  local bunker
  if [[ -n "$bunker_file" ]]; then
    [[ -f "$bunker_file" && ! -L "$bunker_file" ]] || fail "$bunker_file is not a regular file"
    [[ "$(stat -c %a "$bunker_file")" == 600 ]] || fail "$bunker_file must have mode 0600"
    bunker="$(head -n 1 "$bunker_file")"
  else
    read -r -s -p "the owner's NIP-46 bunker address (bunker://…), not shown: " bunker
    echo
  fi
  [[ "$bunker" == bunker://* ]] || fail "a NIP-46 bunker address starts with bunker://"
  umask 077
  mkdir -p "$work/state"
  work="$(realpath "$work")"
  template="$work/template.json"
  signed="$work/owner.event.json"
  state="$work/state"

  KORRID_PRIVATE_STATE_ROOT="$state" "$KORRID" identity owner-binding-request > "$template"
  local device created_at
  device="$(python3 - "$template" <<'EOF'
import json, sys
template = json.load(open(sys.argv[1]))
tags = template["tags"]
device = tags[1][1]
assert template["kind"] == 30078 and template["content"] == ""
assert tags == [["d", "org.korri.device-owner:" + device], ["device", device], ["status", "owned"]]
print(device)
EOF
)" || fail "korrid's owner statement template has an unexpected shape"
  created_at="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["created_at"])' "$template")"
  echo "the phone's device key: $device"
  echo "the statement to sign: kind 30078, created_at $created_at, by the owner $owner"

  echo "asking the owner's signer; approve the signing request there (kind 30078)"
  NOSTR_SECRET_KEY="$bunker" nak event -k 30078 --created-at "$created_at" \
    -t "d=org.korri.device-owner:$device" -t "device=$device" -t "status=owned" -c '' > "$signed"
  unset bunker

  python3 - "$template" "$signed" "$owner" <<'EOF' || fail "the signer returned another statement than the template, or another owner's"
import json, sys
template, signed, owner = json.load(open(sys.argv[1])), json.load(open(sys.argv[2])), sys.argv[3]
assert signed["pubkey"] == owner, "owner"
for field in ("kind", "created_at", "tags", "content"):
    assert signed[field] == template[field], field
EOF
  local imported
  imported="$(KORRID_PRIVATE_STATE_ROOT="$state" "$KORRID" identity import --file "$signed")"
  python3 - "$imported" "$device" "$owner" <<'EOF' || fail "korrid did not store an owned identity for this device and owner: $imported"
import json, sys
state = json.loads(sys.argv[1])
assert state["_tag"] == "Owned"
assert state["devicePublicKey"] == sys.argv[2] and state["ownerPublicKey"] == sys.argv[3]
EOF
  echo "bound: device $device, owner $owner"
  echo "next: install the debug APK, do not start it, then run seed $work"
}

sign_debug() {
  [[ $# -eq 2 ]] || usage
  local unsigned="$1" signed="$2" keystore="$HOME/.android/debug.keystore" manifest
  # Only a debuggable APK: run-as, which seed needs, refuses any other. The
  # manifest is read whole first, so that grep -q cannot cut its writer off.
  manifest="$("$KORRI_APP_BUILD_TOOLS/aapt2" dump xmltree --file AndroidManifest.xml "$unsigned")" ||
    fail "$unsigned has no manifest that aapt2 can read"
  grep -q 'debuggable(0x0101000f)=true' <<< "$manifest" || fail "$unsigned is not debuggable"
  # Android's debug key, as the proofs sign with it (proof/run.py,
  # debug_key): Android's own fixed name, alias and password.
  if [[ ! -e "$keystore" ]]; then
    mkdir -p "$(dirname "$keystore")"
    keytool -genkeypair -keystore "$keystore" -storepass android -alias androiddebugkey \
      -keypass android -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
  fi
  "$KORRI_APP_BUILD_TOOLS/apksigner" sign --ks "$keystore" --ks-pass pass:android --key-pass pass:android \
    --v4-signing-enabled false --out "$signed" "$unsigned"
  "$KORRI_APP_BUILD_TOOLS/apksigner" verify "$signed"
  echo "signed with the debug key: $signed"
}

seed() {
  [[ $# -ge 1 ]] || usage
  local work="$1" serial=() name local_size listed installed
  shift
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --serial) [[ $# -ge 2 ]] || usage; serial=(-s "$2"); shift 2 ;;
      *) usage ;;
    esac
  done
  for name in device.key owner.event.json; do
    [[ -s "$work/state/identity/$name" ]] || fail "$work/state/identity/$name is missing: run make first"
  done
  installed="$(adb "${serial[@]}" shell pm path "$package")" || true
  grep -q '^package:' <<< "$installed" || fail "$package is not installed"
  adb "${serial[@]}" shell run-as "$package" true ||
    fail "run-as refuses $package: install the debuggable APK (sign-debug)"
  if adb "${serial[@]}" shell run-as "$package" ls "$identity/device.key" > /dev/null 2>&1; then
    fail "the app already has a device key, so it has started once; seed only a new install"
  fi
  adb "${serial[@]}" shell run-as "$package" sh -c "'umask 077 && mkdir -p $identity'"
  for name in device.key owner.event.json; do
    # adb exec-in returns neither the device command's status nor waits for
    # its end: read the size back until it is all there (proof/holder.py).
    adb "${serial[@]}" exec-in "run-as $package sh -c 'umask 077 && cat > $identity/$name'" < "$work/state/identity/$name"
    local_size="$(stat -c %s "$work/state/identity/$name")"
    for _ in $(seq 15); do
      [[ "$(adb "${serial[@]}" shell run-as "$package" wc -c "$identity/$name" | awk '{print $1}')" == "$local_size" ]] && break
      sleep 1
    done
    [[ "$(adb "${serial[@]}" shell run-as "$package" wc -c "$identity/$name" | awk '{print $1}')" == "$local_size" ]] ||
      fail "the app's storage did not take $name"
  done
  listed="$(adb "${serial[@]}" shell run-as "$package" ls -l "$identity")"
  echo "$listed"
  [[ "$(grep -c -- '-rw-------' <<< "$listed")" == 2 ]] || fail "the seeded files are not the app's own 0600 files"
  # The phone's device key stays on the phone only.
  rm -rf "$work/state"
  echo "seeded; start the app: it shows \"Identity connected\" and publishes its owner statement"
}

[[ $# -ge 1 ]] || usage
mode="$1"
shift
case "$mode" in
  make) make_binding "$@" ;;
  sign-debug) sign_debug "$@" ;;
  seed) seed "$@" ;;
  *) usage ;;
esac
