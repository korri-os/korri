use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn cli(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_korri"));
    command
        .current_dir(root)
        .env("KORRI_CLI_SOCKET", root.join("missing.sock"));
    command
}

pub fn executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

pub struct AuthorTools {
    pub root: tempfile::TempDir,
}
impl AuthorTools {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("bin")).unwrap();
        fs::create_dir(root.path().join("output")).unwrap();
        fs::write(
            root.path().join("output/manifest.json"),
            r#"{"publisher":{"namespace":"@author"}}"#,
        )
        .unwrap();
        executable(
            &root.path().join("bin/nix"),
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$TOOLS_ROOT/nix.calls"
if [ "$1" = key ] && [ -n "${NIX_REAL_KEYS:-}" ]; then exec "$NIX_REAL_KEYS" "$@"; fi
if [ "${NIX_EXIT:-0}" -ne 0 ]; then exit "$NIX_EXIT"; fi
case "$1 $2" in
  'config show') printf '%s\n' '{"system":{"value":"x86_64-linux"},"extra-platforms":{"value":["aarch64-linux"]}}' ;;
  'eval --json') printf '%s\n' '["x86_64-linux","aarch64-linux"]' ;;
  'build --json') printf '[{"outputs":{"out":"%s/output"}}]\n' "$TOOLS_ROOT" ;;
  'key generate-secret') printf '%s\n' 'author:secret-bytes' ;;
  'key convert-secret-to-public') cat >/dev/null; printf '%s\n' 'author:AbCdEfGhIjKlMnOpAAAAAAAAAAAAAAAAAAAAAAAAAAA=' ;;
  *) exit 9 ;;
esac
"#,
        );
        executable(
            &root.path().join("bin/korri-publish"),
            r#"#!/bin/sh
printf '%s\n' "$@" >> "$TOOLS_ROOT/publisher.calls"
if [ "$1" = archive ]; then
  [ "$2" = --secret-key ] || exit 8
  [ -f "$3" ] || exit 8
  cp "$TOOLS_ROOT/archive.bytes" "$4/plugin.tar"
  printf '{"archive":"%s/plugin.tar","sha256":"%s","platform":"%s","storePath":"/nix/store/example-plugin"}\n' "$4" "$ARCHIVE_SHA256" "${ARCHIVE_PLATFORM:-aarch64-linux}"
  exit "${PUBLISH_EXIT:-0}"
fi
[ "$1" = repository ] || exit 8
[ "$2" = --secret-key ] || exit 8
[ -f "$3" ] || exit 8
[ "$4" = --release ] || exit 8
[ "$6" = --base-url ] || exit 8
mkdir -p "$8"
printf '%s\n' '{"records":[],"latest":{}}' > "$8/catalog.json"
exit "${PUBLISH_EXIT:-0}"
"#,
        );
        Self { root }
    }
    pub fn cli(&self) -> Command {
        let mut command = cli(self.root.path());
        self.configure(&mut command);
        command
    }
    pub fn configure(&self, command: &mut Command) {
        command
            .current_dir(self.root.path())
            .env("TOOLS_ROOT", self.root.path())
            .env("XDG_CONFIG_HOME", self.root.path().join("config"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.root.path().join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            );
    }
}
