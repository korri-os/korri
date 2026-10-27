#!/usr/bin/env bash
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"
VERIFY_SOURCE="$HERE/verify-korri-launcher-apk.sh"
PIPELINE="$HERE/launcher-image-dry-run.sh"
PRODUCT="$HERE/launcher-product-dry-run.sh"
MANIFEST="$ROOT/clients/android/app/src/main/AndroidManifest.xml"
REAL_ANDROID_HOME="${ANDROID_HOME:?run through a Nix Android task}"
REAL_ZIPALIGN="$(find -L "$REAL_ANDROID_HOME/build-tools" -mindepth 2 -maxdepth 2 -name zipalign -type f -print | sort -V | tail -n1)"
[[ -x "$REAL_ZIPALIGN" ]]
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/verifier/contract"
cp "$VERIFY_SOURCE" "$TMP/verifier/verify-korri-launcher-apk.sh"
cp "$HERE/contract/korri-release-cert-SHA256.txt" \
  "$TMP/verifier/contract/korri-release-cert-SHA256.txt"
VERIFY="$TMP/verifier/verify-korri-launcher-apk.sh"

python3 - "$MANIFEST" <<'PY'
import sys
import xml.etree.ElementTree as ET
android = "{http://schemas.android.com/apk/res/android}"
root = ET.parse(sys.argv[1]).getroot()
activity = next(a for a in root.find("application").findall("activity")
                if a.get(android + "name") == ".KorriShellActivity")
filters = []
for intent_filter in activity.findall("intent-filter"):
    values = {(kind, node.get(android + "name"))
              for kind in ("action", "category")
              for node in intent_filter.findall(kind)}
    filters.append(values)
required = {("action", "android.intent.action.MAIN"),
            ("category", "android.intent.category.HOME"),
            ("category", "android.intent.category.DEFAULT")}
assert sum(required <= values for values in filters) == 1
assert any(("category", "android.intent.category.LAUNCHER") in values for values in filters)
assert any(("category", "android.intent.category.LEANBACK_LAUNCHER") in values for values in filters)
PY

mkdir -p "$TMP/sdk/build-tools/35.0.0" "$TMP/sdk/cmdline-tools/19.0/bin"
cat > "$TMP/sdk/build-tools/35.0.0/apksigner" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == verify ]]
printf '%s\n' \
  'Verifies' \
  "Number of signers: ${MOCK_SIGNER_COUNT:-1}" \
  "Signer #1 certificate SHA-256 digest: ${MOCK_CERT:-f46183b71944a33c4c3d2fde42471846ff8d41d22f33b14c1dcf2265d1c7e8ad}"
SCRIPT
cat > "$TMP/sdk/cmdline-tools/19.0/bin/apkanalyzer" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1 $2" == 'manifest print' ]]; then
  package="${MOCK_PACKAGE:-com.simonwjackson.korri}"
  home="${MOCK_HOME_CATEGORY:-android.intent.category.HOME}"
  debuggable="${MOCK_DEBUGGABLE:-false}"
  application_enabled="${MOCK_APPLICATION_ENABLED:-true}"
  extract_native_libs="${MOCK_EXTRACT_NATIVE_LIBS:-false}"
  activity_enabled="${MOCK_ACTIVITY_ENABLED:-true}"
  activity_name="${MOCK_ACTIVITY_NAME:-com.limelight.KorriShellActivity}"
  cat <<XML
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="$package">
  <application android:debuggable="$debuggable" android:enabled="$application_enabled" android:extractNativeLibs="$extract_native_libs">
    <activity android:name="$activity_name" android:exported="true" android:enabled="$activity_enabled">
      <intent-filter>
        <action android:name="android.intent.action.MAIN"/>
        <category android:name="$home"/>
        <category android:name="android.intent.category.DEFAULT"/>
      </intent-filter>
    </activity>
  </application>
</manifest>
XML
elif [[ "$1 $2" == 'files list' ]]; then
  korrid_file="${MOCK_KORRID_FILE-/lib/arm64-v8a/libkorrid.so}"
  [[ -z "$korrid_file" ]] || printf '%s\n' "$korrid_file"
  printf '%s\n' \
    '/lib/arm64-v8a/libmoonlight-core.so' \
    "${MOCK_PORTAL_INDEX:-/assets/portal/index.html}" \
    "${MOCK_PORTAL_SCRIPT_FILE:-/assets/portal/assets/index.js}" \
    "${MOCK_PORTAL_STYLE_FILE:-/assets/portal/assets/index.css}" \
    "${MOCK_PORTAL_FONT_FILE:-/assets/portal/assets/font.woff2}"
elif [[ "$1 $2" == 'files cat' ]]; then
  if [[ "${4:-}" == *.css ]]; then
    printf '%s\n' "${MOCK_PORTAL_CSS:-@font-face { src: url(./font.woff2); }}"
  elif [[ "${4:-}" == *.js ]]; then
    printf '%s\n' "${MOCK_PORTAL_JS:-console.log('ok');}"
  else
    cat <<HTML
<div id="${MOCK_PORTAL_ROOT:-app}"></div>
<script type="module" src="${MOCK_PORTAL_SCRIPT_REF:-./assets/index.js}"></script>
<link rel="stylesheet" href="${MOCK_PORTAL_STYLE_REF:-./assets/index.css}">
HTML
  fi
else
  printf '%s\n' "${MOCK_HOME_CLASS:-C d 1 1 1 com.limelight.KorriShellActivity}"
fi
SCRIPT
chmod +x "$TMP/sdk/build-tools/35.0.0/apksigner" "$TMP/sdk/cmdline-tools/19.0/bin/apkanalyzer"
ln -s "$REAL_ZIPALIGN" "$TMP/sdk/build-tools/35.0.0/zipalign"
python3 - "$TMP/Korri-unaligned.apk" "$TMP/Korri-compressed-unaligned.apk" <<'PY'
import sys
import zipfile

for path, compression in (
    (sys.argv[1], zipfile.ZIP_STORED),
    (sys.argv[2], zipfile.ZIP_DEFLATED),
):
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr(
            "lib/arm64-v8a/libmoonlight-core.so",
            b"fixture moonlight library",
            compress_type=zipfile.ZIP_STORED,
        )
        archive.writestr(
            "lib/arm64-v8a/libkorrid.so",
            b"fixture korrid library",
            compress_type=compression,
        )
PY
"$REAL_ZIPALIGN" -f -P 16 4 "$TMP/Korri-unaligned.apk" "$TMP/Korri.apk"
"$REAL_ZIPALIGN" -f -P 16 4 \
  "$TMP/Korri-compressed-unaligned.apk" "$TMP/Korri-compressed.apk"
sha256sum "$TMP/Korri.apk" | awk '{print $1}' \
  > "$TMP/verifier/contract/korri-launcher-apk-SHA256.txt"

ANDROID_HOME="$TMP/sdk" "$VERIFY" "$TMP/Korri.apk" "$TMP/pass"
grep -Fx KORRI_LAUNCHER_APK_VERIFIED "$TMP/pass/RESULT.txt" >/dev/null
grep -F 'lib/arm64-v8a/libkorrid.so (OK)' "$TMP/pass/zipalign.txt" >/dev/null
cut -f1 "$TMP/pass/native-library-packaging.txt" > "$TMP/native-library-paths.txt"
printf '%s\n' \
  '/lib/arm64-v8a/libkorrid.so' \
  '/lib/arm64-v8a/libmoonlight-core.so' \
  > "$TMP/expected-native-library-paths.txt"
cmp "$TMP/expected-native-library-paths.txt" "$TMP/native-library-paths.txt"

expect_rejection() {
  local name="$1"
  shift
  if env ANDROID_HOME="$TMP/sdk" "$@" "$VERIFY" "$TMP/Korri.apk" "$TMP/$name" \
    >/dev/null 2>&1; then
    echo "launcher APK verifier accepted $name" >&2
    exit 1
  fi
  [[ ! -e "$TMP/$name" ]]
}
printf 'wrong hash\n' > "$TMP/wrong-hash.apk"
if ANDROID_HOME="$TMP/sdk" "$VERIFY" "$TMP/wrong-hash.apk" "$TMP/wrong-hash" \
  >/dev/null 2>&1; then
  echo 'launcher APK verifier accepted an unapproved artifact hash' >&2
  exit 1
fi
[[ ! -e "$TMP/wrong-hash" ]]

expect_rejection wrong-signer MOCK_CERT=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
expect_rejection extra-signer MOCK_SIGNER_COUNT=2
expect_rejection wrong-package MOCK_PACKAGE=com.simonwjackson.korri.debug
expect_rejection missing-home MOCK_HOME_CATEGORY=android.intent.category.LAUNCHER
expect_rejection debuggable MOCK_DEBUGGABLE=true
expect_rejection disabled-application MOCK_APPLICATION_ENABLED=false
expect_rejection extracted-native-libraries MOCK_EXTRACT_NATIVE_LIBS=true
expect_rejection disabled-activity MOCK_ACTIVITY_ENABLED=false
expect_rejection relative-home-class MOCK_ACTIVITY_NAME=.KorriShellActivity
expect_rejection wrong-abi MOCK_KORRID_FILE=/lib/x86_64/libkorrid.so
expect_rejection missing-korrid MOCK_KORRID_FILE=
expect_rejection missing-home-class MOCK_HOME_CLASS='C d 1 1 1 com.limelight.OtherActivity'
expect_rejection missing-portal-index MOCK_PORTAL_INDEX=/assets/portal/index.html.bak
expect_rejection missing-portal-script MOCK_PORTAL_SCRIPT_FILE=/assets/other.js
expect_rejection missing-portal-style MOCK_PORTAL_STYLE_FILE=/assets/other.css
expect_rejection missing-portal-root MOCK_PORTAL_ROOT=missing
expect_rejection external-portal-script MOCK_PORTAL_SCRIPT_REF=https://example.invalid/index.js
expect_rejection missing-portal-font MOCK_PORTAL_FONT_FILE=/assets/other.woff2
expect_rejection external-portal-font MOCK_PORTAL_CSS='@font-face { src: url(https://example.invalid/font.woff2); }'
expect_rejection imported-portal-css MOCK_PORTAL_CSS='@import "./theme.css";'
expect_rejection imported-portal-js MOCK_PORTAL_JS='import("./chunk.js");'

sha256sum "$TMP/Korri-unaligned.apk" | awk '{print $1}' \
  > "$TMP/verifier/contract/korri-launcher-apk-SHA256.txt"
if ANDROID_HOME="$TMP/sdk" "$VERIFY" "$TMP/Korri-unaligned.apk" "$TMP/unaligned-korrid" \
  >/dev/null 2>&1; then
  echo 'launcher APK verifier accepted unaligned native libraries' >&2
  exit 1
fi
[[ ! -e "$TMP/unaligned-korrid" ]]

sha256sum "$TMP/Korri-compressed.apk" | awk '{print $1}' \
  > "$TMP/verifier/contract/korri-launcher-apk-SHA256.txt"
if ANDROID_HOME="$TMP/sdk" "$VERIFY" "$TMP/Korri-compressed.apk" "$TMP/compressed-korrid" \
  >/dev/null 2>&1; then
  echo 'launcher APK verifier accepted compressed libkorrid.so' >&2
  exit 1
fi
[[ ! -e "$TMP/compressed-korrid" ]]
sha256sum "$TMP/Korri.apk" | awk '{print $1}' \
  > "$TMP/verifier/contract/korri-launcher-apk-SHA256.txt"

for script in "$VERIFY_SOURCE" "$PIPELINE" "$PRODUCT"; do
  if grep -Eq '(^|[[:space:]])(adb|fastboot)([[:space:]]|$)' "$script"; then
    echo "device tool found in host-only launcher pipeline: $script" >&2
    exit 1
  fi
done
grep -F 'launcher-product-dry-run.sh' "$PIPELINE" >/dev/null
grep -F '/app/Korri/Korri.apk' "$PRODUCT" >/dev/null
grep -F 'NON_FLASHABLE_ARTIFACTS' "$PIPELINE" >/dev/null
grep -Fx 'f46183b71944a33c4c3d2fde42471846ff8d41d22f33b14c1dcf2265d1c7e8ad' \
  "$HERE/contract/korri-release-cert-SHA256.txt" >/dev/null

integration_inputs=(
  "${ODIN2PORTAL_STOCK_SOURCE:-}"
  "${ODIN2PORTAL_LAUNCHER_APK:-}"
  "${ODIN2PORTAL_AVB_PRIVATE_KEY:-}"
)
configured=0
for input in "${integration_inputs[@]}"; do
  [[ -z "$input" ]] || configured=$((configured + 1))
done
if [[ "$configured" -ne 0 && "$configured" -ne 3 ]]; then
  echo 'set all three launcher integration variables or none of them' >&2
  exit 1
fi
if [[ "$configured" -eq 3 ]]; then
  integration_output="$TMP/integration-output"
  "$PIPELINE" \
    "${integration_inputs[0]}" \
    "${integration_inputs[1]}" \
    "${integration_inputs[2]}" \
    "$integration_output"
  grep -Fx ODIN2PORTAL_LAUNCHER_IMAGE_DRY_RUN_VERIFIED \
    "$integration_output/RESULT.txt" >/dev/null
  grep -Fx 'device writes: none' "$integration_output/RESULT.txt" >/dev/null
  (
    cd "$integration_output"
    sha256sum --check MANIFEST-SHA256SUMS >/dev/null
  )
else
  echo 'odin2portal launcher image integration skipped: set all three private inputs'
fi

echo 'odin2portal launcher image guards: PASS'
