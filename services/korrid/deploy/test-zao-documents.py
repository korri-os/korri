#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 bash coreutils
"""Run the shipped installer against temporary files and configured subprocesses."""

import hashlib
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

DEPLOY = Path(__file__).resolve().parent
DOCUMENTS = ("device.yaml", "catalog/games.yaml", "catalog/releases.yaml")


def digest(root):
    hashes = subprocess.check_output(["sha256sum", *DOCUMENTS], cwd=root)
    return hashlib.sha256(hashes).hexdigest() + "\n"


def executable(path, source):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/usr/bin/env bash\nset -euo pipefail\n" + source)
    path.chmod(0o755)


class ZaoDocuments(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.home = self.root / "home"
        self.storage = self.home / ".local/share/korri"
        self.state = self.home / ".local/state/korrid"
        self.handoff = self.root / "handoff"
        self.bin = self.root / "bin"
        self.handoff.mkdir()
        (self.handoff / "catalog").mkdir()
        (self.storage / "catalog").mkdir(parents=True)
        (self.storage / "roms").mkdir()
        (self.storage / "roms/wl4.gba").write_bytes(b"provisioned")
        executable(self.home / ".nix-profile/bin/neverball", "exit 0\n")
        (self.state / "profiles/candidate").mkdir(parents=True)
        (self.state / "current").symlink_to("previous-profile")
        for name, fixture in zip(
            DOCUMENTS, ("device.zao.yaml", "games.zao.yaml", "releases.zao.yaml")
        ):
            (self.handoff / name).write_bytes((DEPLOY / fixture).read_bytes())
            (self.storage / name).write_bytes(
                b"# previous generation\n" + (DEPLOY / fixture).read_bytes()
            )
        self.previous = {name: (self.storage / name).read_bytes() for name in DOCUMENTS}
        self.marker = self.state / "deployed-documents.sha256"
        self.marker.write_text(digest(self.storage))
        self.previous_marker = self.marker.read_bytes()
        (self.handoff / "revision").write_text("candidate-revision\n")
        (self.handoff / "environment").write_text("KORRID_RELAYS='[]'\n")
        for name in ("host.zao.toml", "korrid.service", "zao-remote.sh"):
            shutil.copyfile(
                DEPLOY / name, self.handoff / name.replace("host.zao", "host")
            )
        self.service_state = self.root / "service-state"
        self.service_state.write_text("inactive\n")
        executable(
            self.bin / "systemctl",
            """printf '%s\\n' "$*" >>"$INSTALL_LOG"
case "$*" in
  '--system show korrid.service -p ActiveState --value'|'--system show korrid-control.socket -p ActiveState --value') printf 'inactive\\n' ;;
  '--user show korrid.service -p ActiveState --value') cat "$INSTALL_SERVICE_STATE" ;;
  '--system list-units '*|'--user list-units '*) ;;
  '--user is-active --quiet korrid.service') [[ "$(cat "$INSTALL_SERVICE_STATE")" == active ]] ;;
  '--user stop korrid.service') printf 'inactive\\n' >"$INSTALL_SERVICE_STATE" ;;
  '--user start korrid.service'|'--user restart korrid.service') printf 'active\\n' >"$INSTALL_SERVICE_STATE" ;;
  '--user daemon-reload'|'--user enable korrid.service') ;;
  *) echo "unexpected service command: $*" >&2; exit 2 ;;
esac
""",
        )
        executable(self.bin / "curl", "printf '%s\\n' \"$INSTALL_RESPONSE\"\n")
        executable(self.bin / "sleep", "exit 0\n")
        # No test can accidentally invoke real package provisioning or networking.
        for name in ("nix", "ssh", "adb"):
            executable(
                self.bin / name, 'echo "unexpected external command" >&2; exit 99\n'
            )
        self.environment = {
            **os.environ,
            "HOME": str(self.home),
            "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
            "INSTALL_LOG": str(self.root / "install.log"),
            "INSTALL_SERVICE_STATE": str(self.service_state),
            "INSTALL_RESPONSE": '{"id":"neverball","games":[{"id":"01K4J6K8Y00000000000000002","title":"Wario Land 4","host":"zao","identity":{"kind":"hash","value":"sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414"}}]}',
        }

    def run_install(self):
        return subprocess.run(
            [
                "bash",
                str(DEPLOY / "zao-remote.sh"),
                "install",
                "/unused/candidate",
                str(self.handoff),
            ],
            env=self.environment,
            text=True,
            capture_output=True,
            timeout=20,
        )

    def test_installs_and_hashes_all_three_documents(self):
        result = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in DOCUMENTS:
            self.assertEqual(
                (self.storage / name).read_bytes(), (self.handoff / name).read_bytes()
            )
        self.assertEqual(self.marker.read_text(), digest(self.storage))

    def test_failed_health_restores_all_bytes_and_checksum(self):
        self.environment["INSTALL_RESPONSE"] = "{}"
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rolled back", result.stderr)
        for name, content in self.previous.items():
            self.assertEqual((self.storage / name).read_bytes(), content)
        self.assertEqual(self.marker.read_bytes(), self.previous_marker)
        self.assertEqual(os.readlink(self.state / "current"), "previous-profile")

    def test_fresh_failed_install_restores_absence(self):
        for name in DOCUMENTS:
            (self.storage / name).unlink()
        (self.storage / "catalog").rmdir()
        self.marker.unlink()
        self.environment["INSTALL_RESPONSE"] = "{}"
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rolled back", result.stderr)
        self.assertFalse(self.marker.exists())
        self.assertFalse((self.storage / "catalog").exists())
        for name in DOCUMENTS:
            self.assertFalse((self.storage / name).exists())

    def assert_unsafe_catalog_preserves_active_install(self, kind):
        for name in DOCUMENTS:
            (self.storage / name).unlink()
        catalog = self.storage / "catalog"
        catalog.rmdir()
        if kind == "file":
            catalog.write_bytes(b"blocking catalog file\n")
        else:
            target = self.root / "catalog-target"
            if kind == "directory-symlink":
                target.mkdir()
            elif kind == "file-symlink":
                target.write_bytes(b"catalog link target\n")
            catalog.symlink_to(target)
        for name in (
            ".config/korrid/host.toml",
            ".config/korrid/environment",
            ".config/systemd/user/korrid.service",
            ".local/libexec/korrid-deploy",
        ):
            path = self.home / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(f"previous {name}\n".encode())
        self.service_state.write_text("active\n")

        def files():
            # Include directories and link identity, but never follow catalog links.
            return {
                str(path.relative_to(self.root)): (
                    path.lstat().st_mode,
                    os.readlink(path)
                    if path.is_symlink()
                    else path.read_bytes()
                    if path.is_file()
                    else None,
                )
                for path in self.root.rglob("*")
            }

        before = files()
        result = self.run_install()
        self.assertEqual(self.service_state.read_text(), "active\n", result.stderr)
        self.assertEqual(files(), before)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unsafe external Korri catalog directory", result.stderr)
        self.assertFalse((self.root / "install.log").exists())

    def test_catalog_file_is_rejected_before_active_install_changes(self):
        self.assert_unsafe_catalog_preserves_active_install("file")

    def test_catalog_directory_symlink_is_rejected_before_active_install_changes(self):
        self.assert_unsafe_catalog_preserves_active_install("directory-symlink")

    def test_catalog_file_symlink_is_rejected_before_active_install_changes(self):
        self.assert_unsafe_catalog_preserves_active_install("file-symlink")

    def test_catalog_dangling_symlink_is_rejected_before_active_install_changes(self):
        self.assert_unsafe_catalog_preserves_active_install("dangling-symlink")

    def test_each_external_edit_is_rejected_without_mutation(self):
        for name in DOCUMENTS:
            with self.subTest(document=name):
                document = self.storage / name
                document.write_bytes(self.previous[name] + b"# external edit\n")
                edited = document.read_bytes()
                result = self.run_install()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("externally edited", result.stderr)
                self.assertEqual(document.read_bytes(), edited)
                self.assertFalse((self.root / "install.log").exists())
                document.write_bytes(self.previous[name])

    def test_each_partial_generation_is_rejected(self):
        for name in DOCUMENTS:
            with self.subTest(document=name):
                document = self.storage / name
                document.unlink()
                result = self.run_install()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("partial external", result.stderr)
                self.assertFalse(document.exists())
                self.assertFalse((self.root / "install.log").exists())
                document.write_bytes(self.previous[name])


if __name__ == "__main__":
    unittest.main()
