#!/usr/bin/env bash
# The release key and the signing step of the Android app's release APK
# (#434 D12). Run it through its Nix apps, on the machine that holds the key:
#
#   nix run .#android-app-release-key
#   nix run .#android-app-release-sign -- UNSIGNED.apk SIGNED.apk
#
# The key never enters the repository, the Nix store or a log: apksigner and
# keytool read the keystore and its password from files in the key
# directory, and this script prints only public facts (the certificate's
# SHA-256 and the signed APK's SHA-256). The key directory and its layout
# are the owner's choice for D12 (README):
#
#   ~/.config/korri/android-signing/            mode 0700
#     korri-android-app-release.p12             PKCS#12 keystore, one key, 0600
#     korri-android-app-release.password        its password, one line, 0600
#     korri-android-app-release-cert.pem        its public certificate, 0600
set -euo pipefail

usage() {
  echo "usage: sign-release.sh key [--key-dir DIR]" >&2
  echo "       sign-release.sh sign [--key-dir DIR] UNSIGNED.apk SIGNED.apk" >&2
  exit 2
}

: "${KORRI_APP_BUILD_TOOLS:?run through nix run .#android-app-release-sign or .#android-app-release-key}"
[[ $# -ge 1 ]] || usage
mode="$1"
shift
directory="$HOME/.config/korri/android-signing"
if [[ "${1:-}" == --key-dir ]]; then
  [[ $# -ge 2 ]] || usage
  directory="$2"
  shift 2
fi
stem="korri-android-app-release"
keystore="$directory/$stem.p12"
password="$directory/$stem.password"
certificate="$directory/$stem-cert.pem"
alias="korri-android-app"

fail() {
  echo "sign-release: $*" >&2
  exit 1
}

# A file or directory that only its owner, this user, can read: no link,
# no group or world bits.
private() {
  local path="$1" want="$2"
  [[ ! -L "$path" ]] || fail "$path is a symbolic link"
  [[ "$(stat -c %u "$path")" == "$(id -u)" ]] || fail "$path is not owned by $(id -un)"
  [[ "$(stat -c %a "$path")" == "$want" ]] || fail "$path has mode $(stat -c %a "$path"), not $want"
}

# The SHA-256 of a certificate, lowercase hexadecimal with no separators.
certificate_digest() {
  keytool -printcert -file "$1" | sed -n 's/^[[:space:]]*SHA256: //p' | tr -d ':' | tr '[:upper:]' '[:lower:]'
}

make_key() {
  [[ $# -eq 0 ]] || usage
  umask 077
  if [[ ! -e "$directory" ]]; then
    mkdir -p "$directory"
    chmod 0700 "$directory"
  fi
  private "$directory" 700
  for file in "$keystore" "$password" "$certificate"; do
    [[ ! -e "$file" ]] || fail "$file exists; this command never replaces a key"
  done
  # 32 random bytes as Base64, with no newline, so keytool and apksigner
  # read the same password.
  head -c 32 /dev/urandom | base64 -w 0 > "$password"
  keytool -genkeypair -keystore "$keystore" -storetype PKCS12 \
    -storepass:file "$password" -alias "$alias" \
    -keyalg RSA -keysize 4096 -validity 10950 \
    -dname "CN=Korri Android App Release, O=Korri"
  [[ -s "$keystore" ]] || fail "keytool made no keystore"
  keytool -exportcert -rfc -keystore "$keystore" -storetype PKCS12 \
    -storepass:file "$password" -alias "$alias" -file "$certificate" > /dev/null
  chmod 0600 "$keystore" "$password" "$certificate"
  echo "release key made in $directory"
  echo "certificate SHA-256: $(certificate_digest "$certificate")"
}

sign() {
  [[ $# -eq 2 ]] || usage
  local unsigned="$1" signed="$2" manifest digest signer verified
  private "$directory" 700
  for file in "$keystore" "$password" "$certificate"; do
    [[ -e "$file" ]] || fail "$file is missing: make the key with nix run .#android-app-release-key"
    private "$file" 600
  done
  case "$(realpath -m "$signed")" in
    /nix/store/*) fail "the signed APK must not go into the Nix store" ;;
  esac
  [[ -f "$unsigned" ]] || fail "$unsigned is not a file"
  # Only the release APK: not debuggable, with its notices. The manifest is
  # read whole first: `grep -q` at the end of a pipe can stop its writer
  # early, and pipefail would then read a match as none.
  manifest="$("$KORRI_APP_BUILD_TOOLS/aapt2" dump xmltree --file AndroidManifest.xml "$unsigned")" ||
    fail "$unsigned has no manifest that aapt2 can read"
  if grep -q -i debuggable <<< "$manifest"; then
    fail "$unsigned is debuggable; sign only .#korri-android-app-release-apk"
  fi
  unzip -l "$unsigned" assets/notices/NOTICES.txt > /dev/null 2>&1 ||
    fail "$unsigned has no notices; sign only .#korri-android-app-release-apk"
  # Global, so that the trap still sees it when the function has returned.
  temporary="$(mktemp "$(dirname "$signed")/.korri-android-app-signing.XXXXXX")"
  trap 'rm -f "$temporary"' EXIT
  # apksigner picks the schemes from minSdkVersion 31: APK Signature Scheme
  # v3 alone, which Android 9 and later read. No v4: it is a second file
  # beside the APK, for incremental installs only.
  "$KORRI_APP_BUILD_TOOLS/apksigner" sign --ks "$keystore" --ks-type PKCS12 \
    --ks-key-alias "$alias" --ks-pass "file:$password" \
    --v4-signing-enabled false --out "$temporary" "$unsigned"
  digest="$(certificate_digest "$certificate")"
  verified="$("$KORRI_APP_BUILD_TOOLS/apksigner" verify --verbose --print-certs "$temporary")" ||
    fail "apksigner does not verify the signed APK"
  grep -q -x 'Number of signers: 1' <<< "$verified" || fail "the signed APK does not have exactly one signer"
  signer="$(sed -n 's/^Signer #1 certificate SHA-256 digest: //p' <<< "$verified")"
  [[ -n "$digest" && "$signer" == "$digest" ]] || fail "the signer is $signer, not the key's certificate $digest"
  chmod 0644 "$temporary"
  mv -f "$temporary" "$signed"
  trap - EXIT
  echo "signed: $signed"
  echo "certificate SHA-256: $signer"
  echo "APK SHA-256: $(sha256sum "$signed" | cut -d ' ' -f 1)"
}

case "$mode" in
  key) make_key "$@" ;;
  sign) sign "$@" ;;
  *) usage ;;
esac
