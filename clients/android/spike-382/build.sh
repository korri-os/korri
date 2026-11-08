#!/usr/bin/env bash
# Spike #382: build the two throwaway APKs with the raw SDK tools (no Gradle).
# Run from the repo root:  nix develop .#android -c bash clients/android/spike-382/build.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bt="$ANDROID_HOME/build-tools/35.0.0"
jar="$ANDROID_HOME/platforms/android-36/android.jar"
out="$here/out"
ks="$out/spike.keystore"
mkdir -p "$out"
[ -f "$ks" ] || keytool -genkeypair -keystore "$ks" -storepass spike382 -keypass spike382 \
  -alias spike -keyalg RSA -keysize 2048 -validity 365 -dname "CN=spike382" >/dev/null 2>&1

build() { # name
  local name="$1" src="$here/$1" b="$out/$1"
  rm -rf "$b"; mkdir -p "$b/gen" "$b/classes" "$b/dex"
  local res=()
  if [ -d "$src/res" ]; then
    "$bt/aapt2" compile --dir "$src/res" -o "$b/res.zip"
    res=("$b/res.zip")
  fi
  "$bt/aapt2" link -I "$jar" --manifest "$src/AndroidManifest.xml" \
    --min-sdk-version 26 --target-sdk-version 35 --java "$b/gen" \
    -o "$b/unsigned.apk" "${res[@]}"
  find "$src/src" "$b/gen" -name '*.java' > "$b/sources.txt"
  javac --release 17 -classpath "$jar" -d "$b/classes" @"$b/sources.txt" -Xlint:-options
  "$bt/d8" --lib "$jar" --min-api 26 --output "$b/dex" $(find "$b/classes" -name '*.class')
  python3 - "$b/unsigned.apk" "$b/dex/classes.dex" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1], "a", zipfile.ZIP_DEFLATED) as z:
    z.write(sys.argv[2], "classes.dex")
PY
  "$bt/zipalign" -f -p 4 "$b/unsigned.apk" "$b/aligned.apk"
  "$bt/apksigner" sign --ks "$ks" --ks-pass pass:spike382 --out "$out/$name.apk" "$b/aligned.apk"
  echo "built $out/$name.apk"
}

build guide
build probe
