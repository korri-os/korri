#!/usr/bin/env bash
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APK="${1:?usage: verify-korri-launcher-apk.sh <signed-release-apk> <evidence-directory>}"
EVIDENCE="${2:?usage: verify-korri-launcher-apk.sh <signed-release-apk> <evidence-directory>}"
CERT_CONTRACT="$HERE/contract/korri-release-cert-SHA256.txt"
APK_SHA256_CONTRACT="$HERE/contract/korri-launcher-apk-SHA256.txt"

[[ -f "$APK" && ! -L "$APK" ]] || {
  echo 'Korri APK must be a regular non-symbolic file' >&2
  exit 1
}
[[ -f "$CERT_CONTRACT" && ! -L "$CERT_CONTRACT" ]] || {
  echo 'Korri release certificate contract is missing or symbolic' >&2
  exit 1
}
[[ -f "$APK_SHA256_CONTRACT" && ! -L "$APK_SHA256_CONTRACT" ]] || {
  echo 'Korri launcher APK hash contract is missing or symbolic' >&2
  exit 1
}
[[ ! -e "$EVIDENCE" && ! -L "$EVIDENCE" ]] || {
  echo 'APK evidence directory already exists' >&2
  exit 1
}
expected_cert="$(tr -d '[:space:]' < "$CERT_CONTRACT")"
expected_apk_sha256="$(tr -d '[:space:]' < "$APK_SHA256_CONTRACT")"
[[ "$expected_cert" =~ ^[0-9a-f]{64}$ ]] || {
  echo 'Korri release certificate contract is malformed' >&2
  exit 1
}
[[ "$expected_apk_sha256" =~ ^[0-9a-f]{64}$ ]] || {
  echo 'Korri launcher APK hash contract is malformed' >&2
  exit 1
}
ANDROID_HOME="${ANDROID_HOME:?run through a Nix Android task}"
apksigner="$(find -L "$ANDROID_HOME/build-tools" -mindepth 2 -maxdepth 2 -name apksigner -type f -print | sort -V | tail -n1)"
zipalign="$(find -L "$ANDROID_HOME/build-tools" -mindepth 2 -maxdepth 2 -name zipalign -type f -print | sort -V | tail -n1)"
apkanalyzer="$(find -L "$ANDROID_HOME/cmdline-tools" -mindepth 3 -maxdepth 3 -name apkanalyzer -type f -print | sort -V | tail -n1)"
[[ -x "$apksigner" && -x "$zipalign" && -x "$apkanalyzer" ]] || {
  echo 'Android APK inspection tools are unavailable' >&2
  exit 1
}

umask 077
mkdir "$EVIDENCE"
cleanup() {
  status=$?
  if [[ "$status" -ne 0 ]]; then
    rm -rf "$EVIDENCE"
  fi
  exit "$status"
}
trap cleanup EXIT

VERIFIED_APK="$EVIDENCE/.Korri.approved.apk"
cp "$APK" "$VERIFIED_APK"
actual_apk_sha256="$(sha256sum "$VERIFIED_APK" | awk '{print $1}')"
[[ "$actual_apk_sha256" == "$expected_apk_sha256" ]] || {
  echo 'Korri APK does not match the approved release artifact contract' >&2
  exit 1
}

"$apksigner" verify --verbose --print-certs "$VERIFIED_APK" > "$EVIDENCE/apksigner.txt"
"$zipalign" -c -P 16 -v 4 "$VERIFIED_APK" > "$EVIDENCE/zipalign.txt"
"$apkanalyzer" manifest print "$VERIFIED_APK" > "$EVIDENCE/AndroidManifest.xml"
"$apkanalyzer" files list "$VERIFIED_APK" > "$EVIDENCE/files.txt"
"$apkanalyzer" files cat --file /assets/portal/index.html "$VERIFIED_APK" \
  > "$EVIDENCE/portal-index.html"
"$apkanalyzer" dex packages --defined-only "$VERIFIED_APK" > "$EVIDENCE/dex-packages.txt"
printf '%s  %s\n' "$actual_apk_sha256" "$(basename "$APK")" \
  > "$EVIDENCE/apk-SHA256SUMS"

signer_count="$(sed -n 's/^Number of signers: //p' "$EVIDENCE/apksigner.txt" | head -n1 | tr -d '[:space:]')"
[[ "$signer_count" == 1 ]] || {
  echo 'Korri APK must have exactly one signer' >&2
  exit 1
}
actual_cert="$(sed -n -e 's/^Signer #1 certificate SHA-256 digest: //p' -e 's/^V[0-9.]* Signer: certificate SHA-256 digest: //p' "$EVIDENCE/apksigner.txt" | tr '[:upper:]' '[:lower:]' | tr -d ':[:space:]' | sort -u)"
[[ "$actual_cert" == "$expected_cert" ]] || {
  echo 'Korri APK signer does not match the release certificate contract' >&2
  exit 1
}

python3 - "$EVIDENCE/AndroidManifest.xml" <<'PY'
import sys
import xml.etree.ElementTree as ET

manifest_path = sys.argv[1]
android = "{http://schemas.android.com/apk/res/android}"
root = ET.parse(manifest_path).getroot()
if root.get("package") != "com.simonwjackson.korri":
    raise SystemExit("Korri APK package is not the release application ID")
application = root.find("application")
if application is None:
    raise SystemExit("Korri APK has no application")
if application.get(android + "debuggable", "false") == "true":
    raise SystemExit("Korri release APK is debuggable")
if application.get(android + "enabled", "true") == "false":
    raise SystemExit("Korri release application is disabled")
if application.get(android + "extractNativeLibs") != "false":
    raise SystemExit("Korri system APK must load native libraries directly from the APK")
activity = None
for candidate in application.findall("activity"):
    name = candidate.get(android + "name", "")
    if name == "com.limelight.KorriShellActivity":
        activity = candidate
        break
if activity is None or activity.get(android + "exported") != "true":
    raise SystemExit("Korri HOME activity is absent or not exported")
if activity.get(android + "enabled", "true") == "false":
    raise SystemExit("Korri HOME activity is disabled")
required = {
    ("action", "android.intent.action.MAIN"),
    ("category", "android.intent.category.HOME"),
    ("category", "android.intent.category.DEFAULT"),
}
matched = False
for intent_filter in activity.findall("intent-filter"):
    values = set()
    for kind in ("action", "category"):
        for node in intent_filter.findall(kind):
            values.add((kind, node.get(android + "name", "")))
    if required <= values:
        matched = True
        break
if not matched:
    raise SystemExit("Korri HOME activity lacks one MAIN+HOME+DEFAULT filter")
PY

python3 - "$VERIFIED_APK" "$EVIDENCE/native-library-packaging.txt" <<'PY'
import sys
import zipfile

apk_path, evidence_path = sys.argv[1:]
try:
    with zipfile.ZipFile(apk_path) as archive:
        libraries = sorted(
            (
                entry
                for entry in archive.infolist()
                if entry.filename.startswith("lib/arm64-v8a/")
                and entry.filename.endswith(".so")
            ),
            key=lambda entry: entry.filename,
        )
except zipfile.BadZipFile as error:
    raise SystemExit("Korri APK is not a valid ZIP archive") from error

for library in libraries:
    if library.compress_type != zipfile.ZIP_STORED:
        raise SystemExit(
            f"Korri system APK contains a compressed native library: {library.filename}"
        )

with open(evidence_path, "w", encoding="utf-8") as evidence:
    for library in libraries:
        evidence.write(
            f"/{library.filename}\tstored\t{library.file_size}\t{library.CRC:08x}\n"
        )
PY

if ! grep -Fx '/lib/arm64-v8a/libkorrid.so' "$EVIDENCE/files.txt" >/dev/null; then
  echo 'Korri APK does not contain the arm64 korrid library' >&2
  exit 1
fi
if grep -Eq '^/lib/(x86|x86_64|armeabi-v7a)/' "$EVIDENCE/files.txt"; then
  echo 'Korri Odin APK must be the arm64-only release split' >&2
  exit 1
fi
if ! grep -Eq '^C[[:space:]].*[[:space:]]com\.limelight\.KorriShellActivity$' "$EVIDENCE/dex-packages.txt"; then
  echo 'Korri APK does not define its HOME activity class' >&2
  exit 1
fi
python3 - "$EVIDENCE/files.txt" "$EVIDENCE/portal-index.html" \
  "$EVIDENCE/portal-stylesheets.txt" "$EVIDENCE/portal-scripts.txt" <<'PY'
import posixpath
import sys
from html.parser import HTMLParser

files = set(open(sys.argv[1], encoding="utf-8").read().splitlines())
index_path = "/assets/portal/index.html"
if index_path not in files:
    raise SystemExit("Korri APK lacks the exact bundled portal entrypoint")

class PortalIndex(HTMLParser):
    def __init__(self):
        super().__init__()
        self.app = False
        self.scripts = []
        self.stylesheets = []

    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        if tag == "div" and values.get("id") == "app":
            self.app = True
        if tag == "script" and values.get("type") == "module" and values.get("src"):
            self.scripts.append(values["src"])
        if tag == "link" and "stylesheet" in values.get("rel", "").split() and values.get("href"):
            self.stylesheets.append(values["href"])

index = PortalIndex()
index.feed(open(sys.argv[2], encoding="utf-8").read())
if not index.app or not index.scripts or not index.stylesheets:
    raise SystemExit("Korri portal entrypoint lacks its app root, module, or stylesheet")
resolved_stylesheets = []
resolved_scripts = []
for reference in index.scripts + index.stylesheets:
    if ":" in reference or reference.startswith("//"):
        raise SystemExit("Korri portal entrypoint contains a non-bundled resource")
    resolved = posixpath.normpath(posixpath.join("/assets/portal", reference))
    if resolved not in files:
        raise SystemExit(f"Korri portal entrypoint references missing content: {reference}")
    if reference in index.stylesheets:
        resolved_stylesheets.append(resolved)
    if reference in index.scripts:
        resolved_scripts.append(resolved)
open(sys.argv[3], "w", encoding="utf-8").write("\n".join(resolved_stylesheets) + "\n")
open(sys.argv[4], "w", encoding="utf-8").write("\n".join(resolved_scripts) + "\n")
PY

script_number=0
while IFS= read -r script; do
  [[ -n "$script" ]]
  script_number=$((script_number + 1))
  "$apkanalyzer" files cat --file "$script" "$VERIFIED_APK" \
    > "$EVIDENCE/portal-script-$script_number.js"
  python3 - "$EVIDENCE/portal-script-$script_number.js" <<'PY'
import re
import sys
content = open(sys.argv[1], encoding="utf-8").read()
if re.search(r"(^|[;\s])import\s*(?:\(|[\"'])|\sfrom\s*[\"']", content):
    raise SystemExit("Korri portal JavaScript contains an unresolved module import")
PY
done < "$EVIDENCE/portal-scripts.txt"

: > "$EVIDENCE/portal-stylesheet-paths.txt"
stylesheet_number=0
while IFS= read -r stylesheet; do
  [[ -n "$stylesheet" ]]
  stylesheet_number=$((stylesheet_number + 1))
  printf '%s\t%s\n' "$stylesheet_number" "$stylesheet" \
    >> "$EVIDENCE/portal-stylesheet-paths.txt"
  "$apkanalyzer" files cat --file "$stylesheet" "$VERIFIED_APK" \
    > "$EVIDENCE/portal-stylesheet-$stylesheet_number.css"
done < "$EVIDENCE/portal-stylesheets.txt"
python3 - "$EVIDENCE/files.txt" "$EVIDENCE/portal-stylesheet-paths.txt" "$EVIDENCE" <<'PY'
import posixpath
import re
import sys

files = set(open(sys.argv[1], encoding="utf-8").read().splitlines())
for mapping in open(sys.argv[2], encoding="utf-8"):
    number, stylesheet = mapping.rstrip("\n").split("\t", 1)
    content = open(f"{sys.argv[3]}/portal-stylesheet-{number}.css", encoding="utf-8").read()
    if re.search(r"@import(?:\s|url\()", content, re.IGNORECASE):
        raise SystemExit("Korri portal stylesheet contains an unresolved import")
    for raw in re.findall(r"url\(([^)]+)\)", content):
        reference = raw.strip().strip("'\"")
        if not reference or reference.startswith(("data:", "#")):
            continue
        if ":" in reference or reference.startswith("//"):
            raise SystemExit("Korri portal stylesheet contains a non-bundled resource")
        resolved = posixpath.normpath(posixpath.join(posixpath.dirname(stylesheet), reference))
        if resolved not in files:
            raise SystemExit(f"Korri portal stylesheet references missing content: {reference}")
PY

rm "$VERIFIED_APK"
[[ "$(sha256sum "$APK" | awk '{print $1}')" == "$actual_apk_sha256" ]] || {
  echo 'Korri APK changed during verification' >&2
  exit 1
}

printf '%s\n' \
  'KORRI_LAUNCHER_APK_VERIFIED' \
  'package: com.simonwjackson.korri' \
  'component: com.simonwjackson.korri/com.limelight.KorriShellActivity' \
  'home filter: MAIN+HOME+DEFAULT' \
  'ABI: arm64-v8a' \
  "APK SHA-256: $actual_apk_sha256" \
  "certificate SHA-256: $actual_cert" \
  > "$EVIDENCE/RESULT.txt"
trap - EXIT
printf 'KORRI_LAUNCHER_APK_VERIFIED apk=%s\n' "$APK"
