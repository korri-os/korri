#!/usr/bin/env python3
"""Add the Korri save bridge to an upstream Android app tree (ADR 0054, case 2).

Usage: integrate.py APP_MODULE_DIR FOLDERS
  APP_MODULE_DIR  the Gradle app module (holds src/main/AndroidManifest.xml)
  FOLDERS         "key=relative/path[;key=path]", relative to getExternalFilesDir(null),
                  keyed like the plugin's saveLocations

It copies the provider sources into the module and declares, in the app's own
manifest: a signature-level permission, the provider guarded by it, and the
folder map. Run it in the APK's Nix build, after upstream's patches. It edits
nothing else and refuses to run twice.
"""
import re
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCES = HERE / "src/main/java/dev/korri/savebridge"
FOLDERS_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,63}=[^;=\s]+(;[a-z0-9][a-z0-9._-]{0,63}=[^;=\s]+)*$")

BLOCK = """
    <!-- Korri save bridge (ADR 0054): one permission name in every Korri-signed
         app; signature level, so only Korri's key can hold it. -->
    <permission
        android:name="dev.korri.permission.SAVE_BRIDGE"
        android:protectionLevel="signature" />
"""
PROVIDER = """
        <provider
            android:name="dev.korri.savebridge.KorriSaveBridgeProvider"
            android:authorities="${{applicationId}}.korri.savebridge"
            android:exported="true"
            android:permission="dev.korri.permission.SAVE_BRIDGE" />
        <meta-data android:name="dev.korri.savebridge.folders" android:value="{folders}" />
"""


def integrate(module: Path, folders: str) -> None:
    if not FOLDERS_RE.match(folders) or ".." in folders:
        raise SystemExit(f"invalid folder map: {folders!r}")
    manifest = module / "src/main/AndroidManifest.xml"
    text = manifest.read_text()
    if "dev.korri.savebridge" in text:
        raise SystemExit("the save bridge is already integrated")
    target = module / "src/main/java/dev/korri/savebridge"
    target.mkdir(parents=True)
    for name in ("KorriSaveBridgeProvider.java", "Paths.java"):
        shutil.copy(SOURCES / name, target / name)
    text, n = re.subn(r"(<manifest\b[^>]*>)", lambda m: m.group(1) + BLOCK.format(), text, count=1)
    if n != 1:
        raise SystemExit("manifest has no <manifest> element")
    text, n = re.subn(r"(<application\b[^>]*>)", lambda m: m.group(1) + PROVIDER.format(folders=folders), text, count=1)
    if n != 1:
        raise SystemExit("manifest has no <application> element")
    manifest.write_text(text)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    integrate(Path(sys.argv[1]), sys.argv[2])
