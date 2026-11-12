//! A plugin's secret: a value korrid uses for the plugin and the plugin's
//! code never sees (ADR 0034, section 2).
//!
//! One secret per plugin, in `<private>/<plugin id>/credential`. korrid
//! creates the folder with mode 0700 and the file with mode 0600 itself, so
//! the modes do not depend on the unit's `UMask`. The value never goes into a
//! log, a failure, readable config or an RPC answer: callers get a
//! `SecretSettingStatus`, and only the code that adds the header reads it.

use crate::config::settings::SecretSettingStatus;
use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

/// The file a plugin's secret is kept in: main's `steamgriddb.credential`
/// without the SteamGridDB part (ADR 0034, "Names").
const CREDENTIAL_FILE_NAME: &str = "credential";
/// The length the settings row allows (`sensitiveText`, 256 characters).
const MAX_SECRET_BYTES: usize = 256;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum PluginSecretError {
    #[error("plugin ID must be @namespace:name with lowercase names")]
    InvalidPluginId,
    #[error("the secret cannot be empty")]
    Empty,
    #[error("the secret is too long")]
    TooLong,
    #[error("the secret has a line break or another control character, which a request header cannot carry")]
    Unsendable,
    #[error("the secret has a character outside ASCII, which a request header does not carry in one fixed form")]
    NotAscii,
    #[error("private state root is unavailable")]
    Storage,
}

/// The secrets of the plugins on this device.
#[derive(Clone, Debug)]
pub struct PluginSecrets {
    private_root: PathBuf,
}

impl PluginSecrets {
    pub fn new(private_root: impl AsRef<Path>) -> Self {
        Self {
            private_root: private_root.as_ref().to_owned(),
        }
    }

    /// Whether the plugin's secret is set. Never the value.
    pub fn status(&self, plugin_id: &str) -> Result<SecretSettingStatus, PluginSecretError> {
        Ok(match self.read(plugin_id)? {
            Some(_) => SecretSettingStatus::Configured,
            None => SecretSettingStatus::NotConfigured,
        })
    }

    /// Keep `secret` for the plugin, replacing any earlier one.
    pub fn set(
        &self,
        plugin_id: &str,
        secret: &str,
    ) -> Result<SecretSettingStatus, PluginSecretError> {
        let folder = self.plugin_folder(plugin_id)?;
        let secret = secret.trim();
        if secret.is_empty() {
            return Err(PluginSecretError::Empty);
        }
        if secret.len() > MAX_SECRET_BYTES {
            return Err(PluginSecretError::TooLong);
        }
        // korrid sends it as a request header and as nothing else.
        bearer(secret)?;
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&folder)
            .map_err(|_| PluginSecretError::Storage)?;
        // A folder that already existed keeps its mode: tighten it.
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))
            .map_err(|_| PluginSecretError::Storage)?;
        write_private_file(&folder.join(CREDENTIAL_FILE_NAME), secret.as_bytes())?;
        Ok(SecretSettingStatus::Configured)
    }

    /// The secret, for the one caller that adds it to a request. `None` when
    /// it is not set.
    pub(crate) fn read(&self, plugin_id: &str) -> Result<Option<String>, PluginSecretError> {
        let path = self.plugin_folder(plugin_id)?.join(CREDENTIAL_FILE_NAME);
        let mut file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(PluginSecretError::Storage),
        };
        let mut secret = String::new();
        file.read_to_string(&mut secret)
            .map_err(|_| PluginSecretError::Storage)?;
        let secret = secret.trim();
        Ok((!secret.is_empty()).then(|| secret.to_owned()))
    }

    /// The plugin's secret, bound to the one host that receives it. `None`
    /// when it is not set: then korrid does not call the plugin's handler
    /// that needs it (ADR 0034, "No secret, no call").
    pub(crate) fn credential(
        &self,
        plugin_id: &str,
        host: &str,
    ) -> Result<Option<PluginCredential>, PluginSecretError> {
        let Some(secret) = self.read(plugin_id)? else {
            return Ok(None);
        };
        Ok(Some(PluginCredential {
            host: host.to_owned(),
            header: bearer(&secret)?,
            secret,
        }))
    }

    /// Every plugin folder that holds a secret, by plugin id. An identity
    /// switch keeps or deletes each one (ADR 0034, "Owner and devices").
    pub(crate) fn plugins_with_a_secret(&self) -> Result<Vec<String>, PluginSecretError> {
        let entries = match fs::read_dir(&self.private_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(PluginSecretError::Storage),
        };
        let mut plugins = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|_| PluginSecretError::Storage)?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if valid_plugin_id(&name) && entry.path().join(CREDENTIAL_FILE_NAME).exists() {
                plugins.push(name);
            }
        }
        plugins.sort();
        Ok(plugins)
    }

    /// Delete the plugin's secret. Deleting one that is already gone succeeds,
    /// so a step that runs twice is safe.
    pub fn clear(&self, plugin_id: &str) -> Result<SecretSettingStatus, PluginSecretError> {
        let folder = self.plugin_folder(plugin_id)?;
        match fs::remove_file(folder.join(CREDENTIAL_FILE_NAME)) {
            Ok(()) => {
                sync_directory(&folder);
                Ok(SecretSettingStatus::NotConfigured)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(SecretSettingStatus::NotConfigured)
            }
            Err(_) => Err(PluginSecretError::Storage),
        }
    }

    /// The plugin's own folder in korrid's private state
    /// (`<private>/<plugin id>/`): its secret, and its artwork attempts
    /// (ADR 0032). The id is checked here, again, before it becomes a path
    /// part.
    pub(crate) fn plugin_folder(&self, plugin_id: &str) -> Result<PathBuf, PluginSecretError> {
        if !valid_plugin_id(plugin_id) {
            return Err(PluginSecretError::InvalidPluginId);
        }
        Ok(self.private_root.join(plugin_id))
    }
}

/// `Authorization: Bearer <secret>`, the only form korrid sends (ADR 0034).
/// The value is marked sensitive, so no `Debug` of a request prints it.
///
/// Two rules, and no other limit on its characters:
/// - A header must carry it unchanged: no line break and no other control
///   character.
/// - It has one text form: ASCII only. A header has no fixed encoding for
///   any other character, so a site could repeat such a secret in bytes the
///   response guard (`PluginCredential::appears_in`) cannot compare.
fn bearer(secret: &str) -> Result<reqwest::header::HeaderValue, PluginSecretError> {
    let mut header = reqwest::header::HeaderValue::from_str(&format!("Bearer {secret}"))
        .map_err(|_| PluginSecretError::Unsendable)?;
    if !secret.is_ascii() {
        return Err(PluginSecretError::NotAscii);
    }
    header.set_sensitive(true);
    Ok(header)
}

/// `text` with JSON string escapes decoded wherever they stand: `\"`, `\\`,
/// `\/`, `\b`, `\f`, `\n`, `\r`, `\t` and `\uXXXX`, with surrogate pairs
/// joined. In a JSON text a backslash stands only inside a string and always
/// starts an escape, so every string value and every object key comes out
/// exactly as `JSON.parse` gives it, at any depth and without a parse: a body
/// korrid's own parser would refuse, or JSON inside another text, is decoded
/// too. One pass over the text.
fn json_unescaped(text: &str) -> String {
    fn code_unit(rest: &str) -> Option<u32> {
        let digits = rest.get(..4)?;
        digits
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
            .then(|| u32::from_str_radix(digits, 16).ok())
            .flatten()
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        let Some(escape) = rest.chars().next() else {
            out.push('\\');
            break;
        };
        let simple = match escape {
            '"' | '\\' | '/' => Some(escape),
            'b' => Some('\u{8}'),
            'f' => Some('\u{c}'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            _ => None,
        };
        if let Some(decoded) = simple {
            out.push(decoded);
            rest = &rest[1..];
        } else if let Some(unit) = escape.eq(&'u').then(|| code_unit(&rest[1..])).flatten() {
            rest = &rest[5..];
            // A high surrogate and the low one that follows are one character.
            let low = (0xd800..0xdc00)
                .contains(&unit)
                .then(|| rest.strip_prefix("\\u").and_then(code_unit))
                .flatten()
                .filter(|low| (0xdc00..0xe000).contains(low));
            let scalar = match low {
                Some(low) => {
                    rest = &rest[6..];
                    0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00)
                }
                None => unit,
            };
            out.push(char::from_u32(scalar).unwrap_or(char::REPLACEMENT_CHARACTER));
        } else {
            // Not a JSON escape: the backslash is itself.
            out.push('\\');
        }
    }
    out.push_str(rest);
    out
}

/// `text` with `%XX` decoded wherever it stands, once. Bytes that are not
/// UTF-8 afterwards become the replacement character.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let decoded = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        match decoded {
            Some(byte) => {
                out.push(byte);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A plugin's secret with the one declared host that receives it. It has no
/// `Debug` and no accessor for the value. It gives the header for a request
/// to that host, says whether a text repeats the secret, and takes the secret
/// out of a text. Nothing else can be done with it.
#[derive(Clone)]
pub(crate) struct PluginCredential {
    host: String,
    header: reqwest::header::HeaderValue,
    secret: String,
}

/// What stands where a secret was, in a text korrid may show or log.
const REDACTED: &str = "<redacted>";

impl PluginCredential {
    /// The `Authorization` header for a request to `url`, or `None` when
    /// `url` is not HTTPS on the declared host. A secret never leaves in
    /// clear text: a plain `http` request to the declared host gets none. A
    /// redirect hop asks again with its own URL, so the secret follows a
    /// redirect only to the same host.
    pub(crate) fn authorization(&self, url: &url::Url) -> Option<reqwest::header::HeaderValue> {
        (url.scheme() == "https" && self.is_for(url)).then(|| self.header.clone())
    }

    /// As `authorization`, for a test's loopback server that speaks plain
    /// HTTP. It does not exist outside tests, so shipped korrid cannot send
    /// a secret over `http`.
    #[cfg(test)]
    pub(crate) fn authorization_over_test_http(
        &self,
        url: &url::Url,
    ) -> Option<reqwest::header::HeaderValue> {
        self.is_for(url).then(|| self.header.clone())
    }

    /// Whether `text` holds the secret: its exact value, which is also what
    /// its `Bearer <value>` form holds. A host that repeats a request's
    /// `Authorization` header in its answer would otherwise hand the secret
    /// to the plugin's code (ADR 0034: never "in a response").
    ///
    /// Three forms are compared, in any part of a response: the text as it
    /// is, the text with JSON string escapes decoded (a string value or an
    /// object key that `JSON.parse` would turn back into the secret, such as
    /// `\"`, `\\`, `\/` or `\uXXXX`), and the text percent-decoded once,
    /// also inside a JSON string (an address in a JSON body). Any other
    /// encoding, such as base64, is not compared. Each decode is one pass
    /// over the text, and a turn's response is at most 512 KiB.
    pub(crate) fn appears_in(&self, text: &str) -> bool {
        let secret = self.secret.as_str();
        let percent = |text: &str| text.contains('%') && percent_decoded(text).contains(secret);
        if text.contains(secret) || percent(text) {
            return true;
        }
        if !text.contains('\\') {
            return false;
        }
        let unescaped = json_unescaped(text);
        unescaped.contains(secret) || percent(&unescaped)
    }

    /// `text` without the secret, for a failure text or a log line that is
    /// built from something a site or a plugin wrote. A text that holds the
    /// secret only in an escaped form is not shown at all.
    pub(crate) fn redact(&self, text: &str) -> String {
        if text.contains(self.secret.as_str()) {
            text.replace(self.secret.as_str(), REDACTED)
        } else if self.appears_in(text) {
            REDACTED.to_owned()
        } else {
            text.to_owned()
        }
    }

    fn is_for(&self, url: &url::Url) -> bool {
        matches!(url.host(), Some(url::Host::Domain(host)) if host == self.host)
    }

    /// Whether `other` holds the same secret for the same host: the artwork
    /// pass records what it learned only while the secret it used still
    /// stands.
    pub(crate) fn same_secret(&self, other: &Self) -> bool {
        self.host == other.host && self.header == other.header
    }
}

/// The plugin host's rule for an id (`plugin-host/src/declaration.rs`,
/// `validate_id`): `@namespace:name`, each of lowercase letters, digits,
/// `-`, `_` and `.`, starting with a letter or a digit. Such an id is always
/// one safe path part.
fn valid_plugin_id(plugin_id: &str) -> bool {
    fn valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 64
            && (name.as_bytes()[0].is_ascii_lowercase() || name.as_bytes()[0].is_ascii_digit())
            && name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_.".contains(&c))
    }
    plugin_id.split_once(':').is_some_and(|(namespace, name)| {
        namespace.strip_prefix('@').is_some_and(valid_name) && valid_name(name)
    })
}

/// Replace `path` with a new mode-0600 file holding `content`: a temporary
/// file, `fsync`, rename, folder `fsync`.
fn write_private_file(path: &Path, content: &[u8]) -> Result<(), PluginSecretError> {
    let parent = path.parent().ok_or(PluginSecretError::Storage)?;
    let temporary = parent.join(format!(
        ".{CREDENTIAL_FILE_NAME}.{}.tmp",
        hex::encode(rand::random::<[u8; 8]>())
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| PluginSecretError::Storage)?;
        // `mode` is masked by the process umask only downwards; set it
        // exactly in case the umask is unusual.
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| PluginSecretError::Storage)?;
        file.write_all(content)
            .map_err(|_| PluginSecretError::Storage)?;
        file.sync_all().map_err(|_| PluginSecretError::Storage)?;
        fs::rename(&temporary, path).map_err(|_| PluginSecretError::Storage)?;
        sync_directory(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_directory(path: &Path) {
    if let Ok(directory) = fs::File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLUGIN: &str = "@korri:steamgriddb";

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn a_set_secret_is_private_on_disk_whatever_the_umask() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        assert_eq!(
            secrets.status(PLUGIN),
            Ok(SecretSettingStatus::NotConfigured)
        );

        // A folder left wider by an earlier run is tightened too.
        let folder = private.path().join(PLUGIN);
        fs::create_dir(&folder).unwrap();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(
            secrets.set(PLUGIN, "  key-one\n"),
            Ok(SecretSettingStatus::Configured)
        );
        assert_eq!(mode(&folder), 0o700);
        assert_eq!(mode(&folder.join("credential")), 0o600);
        assert_eq!(secrets.read(PLUGIN), Ok(Some("key-one".into())));
        assert_eq!(secrets.status(PLUGIN), Ok(SecretSettingStatus::Configured));

        // A new value replaces the old one, and no temporary file stays.
        secrets.set(PLUGIN, "key-two").unwrap();
        assert_eq!(secrets.read(PLUGIN), Ok(Some("key-two".into())));
        assert_eq!(mode(&folder.join("credential")), 0o600);
        let left: Vec<_> = fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("credential")]);
    }

    #[test]
    fn each_plugin_has_its_own_secret() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        secrets.set(PLUGIN, "one").unwrap();
        secrets.set("@other:art", "two").unwrap();
        assert_eq!(secrets.read(PLUGIN), Ok(Some("one".into())));
        assert_eq!(secrets.read("@other:art"), Ok(Some("two".into())));
        secrets.clear(PLUGIN).unwrap();
        assert_eq!(secrets.read(PLUGIN), Ok(None));
        assert_eq!(secrets.read("@other:art"), Ok(Some("two".into())));
    }

    #[test]
    fn clear_deletes_the_secret_and_can_run_twice() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        // Nothing was ever set: no folder, and still a success.
        assert_eq!(
            secrets.clear(PLUGIN),
            Ok(SecretSettingStatus::NotConfigured)
        );
        secrets.set(PLUGIN, "key").unwrap();
        for _ in 0..2 {
            assert_eq!(
                secrets.clear(PLUGIN),
                Ok(SecretSettingStatus::NotConfigured)
            );
        }
        assert_eq!(secrets.read(PLUGIN), Ok(None));
        assert!(!private.path().join(PLUGIN).join("credential").exists());
    }

    #[test]
    fn an_empty_or_overlong_secret_is_refused_and_changes_nothing() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        secrets.set(PLUGIN, "kept").unwrap();
        assert_eq!(secrets.set(PLUGIN, "  \n"), Err(PluginSecretError::Empty));
        assert_eq!(
            secrets.set(PLUGIN, &"k".repeat(257)),
            Err(PluginSecretError::TooLong)
        );
        assert_eq!(
            secrets.set(PLUGIN, &"k".repeat(256)),
            Ok(SecretSettingStatus::Configured)
        );
        secrets.set(PLUGIN, "kept").unwrap();
        assert_eq!(secrets.read(PLUGIN), Ok(Some("kept".into())));
    }

    #[test]
    fn a_secret_a_header_cannot_carry_is_refused_and_changes_nothing() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        secrets.set(PLUGIN, "kept").unwrap();
        for unsendable in ["line\nbreak", "nul\0byte", "delete\u{7f}char"] {
            assert_eq!(
                secrets.set(PLUGIN, unsendable),
                Err(PluginSecretError::Unsendable)
            );
        }
        assert_eq!(secrets.read(PLUGIN), Ok(Some("kept".into())));
    }

    #[test]
    fn a_credential_gives_its_header_to_its_host_only_and_never_prints() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        assert!(secrets
            .credential(PLUGIN, "www.steamgriddb.com")
            .unwrap()
            .is_none());
        secrets.set(PLUGIN, "very-secret-value").unwrap();
        let credential = secrets
            .credential(PLUGIN, "www.steamgriddb.com")
            .unwrap()
            .unwrap();
        let url = |text: &str| url::Url::parse(text).unwrap();
        let header = credential
            .authorization(&url("https://www.steamgriddb.com/api/v2/grids/game/1"))
            .unwrap();
        assert_eq!(header.to_str().unwrap(), "Bearer very-secret-value");
        assert!(header.is_sensitive());
        assert!(!format!("{header:?}").contains("very-secret-value"));
        // The host is compared as the URL parser normalises it.
        assert!(credential
            .authorization(&url("https://WWW.SteamGridDB.com:8443/x"))
            .is_some());
        for other in [
            "https://cdn2.steamgriddb.com/grid/a.png",
            "https://steamgriddb.com/api",
            "https://www.steamgriddb.com.example.org/api",
            "https://example.org/?next=www.steamgriddb.com",
            "https://93.184.216.34/api",
        ] {
            assert!(credential.authorization(&url(other)).is_none(), "{other}");
            assert!(
                credential
                    .authorization_over_test_http(&url(other))
                    .is_none(),
                "{other}"
            );
        }
        // Never in clear text: the declared host over plain `http` gets no
        // header, on any port.
        for plain in [
            "http://www.steamgriddb.com/api/v2/grids/game/1",
            "http://www.steamgriddb.com:443/api/v2/grids/game/1",
            "http://WWW.SteamGridDB.com:8443/x",
        ] {
            assert!(credential.authorization(&url(plain)).is_none(), "{plain}");
        }
    }

    #[test]
    fn a_credential_finds_its_secret_in_a_text_and_takes_it_out() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        secrets.set(PLUGIN, "very-secret-value").unwrap();
        let credential = secrets
            .credential(PLUGIN, "www.steamgriddb.com")
            .unwrap()
            .unwrap();
        // The exact value, and the form korrid sends it in.
        for repeated in [
            "very-secret-value",
            "Bearer very-secret-value",
            r#"{"headers":{"Authorization":"Bearer very-secret-value"}}"#,
            "authorization: bearer very-secret-value",
            "https://www.steamgriddb.com/?key=very-secret-value&x=1",
        ] {
            assert!(credential.appears_in(repeated), "{repeated}");
            let redacted = credential.redact(repeated);
            assert!(!redacted.contains("very-secret-value"), "{redacted}");
            assert!(redacted.contains("<redacted>"), "{redacted}");
        }
        for other in [
            "",
            "Bearer another-value",
            "very-secret",
            "VERY-SECRET-VALUE",
            r#"{"success":true,"data":[]}"#,
        ] {
            assert!(!credential.appears_in(other), "{other}");
            assert_eq!(credential.redact(other), other);
        }
    }

    #[test]
    fn a_credential_finds_its_secret_json_escaped_and_percent_encoded() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        let found = |secret: &str, text: &str| {
            secrets.set(PLUGIN, secret).unwrap();
            let credential = secrets
                .credential(PLUGIN, "www.steamgriddb.com")
                .unwrap()
                .unwrap();
            // A text that holds it only in an escaped form is not shown at all.
            if credential.appears_in(text) && !text.contains(secret) {
                assert_eq!(credential.redact(text), "<redacted>");
            }
            credential.appears_in(text)
        };
        // What `JSON.parse` turns back into the secret, as a value or a key.
        assert!(found(r#"a"b"#, r#"{"auth":"Bearer a\"b"}"#));
        assert!(found(r"a\b", r#"{"auth":"a\\b"}"#));
        assert!(found("a/b", r#"{"auth":"a\/b"}"#));
        assert!(found("abc", r#"{"auth":"\u0061\u0062\u0063"}"#));
        assert!(found("abc", r#"{"\u0061bc":true}"#));
        assert!(found("a\tb", r#"["a\tb"]"#));
        // Upper-case hex, and a mix of escaped and plain characters.
        assert!(found("key-JK", r#""key-\u004A\u004b""#));
        // Percent-encoded, once: in a header or in an address.
        assert!(found("a b/c", "Bearer%20a%20b%2Fc"));
        assert!(found("abc", "https://cdn.test/x?key=%61%62%63&y=1"));
        assert!(found("a%41b", "a%41b"));
        // Both: an address inside a JSON string.
        assert!(found(
            "a b/c",
            r#"{"url":"https:\/\/cdn.test\/x?key=a%20b\/c"}"#
        ));
        assert!(found(r"a\nb", r"%61\nb"));
        // Not the secret: nothing is found, and the text is left alone.
        for (secret, text) in [
            ("abc", r#"{"auth":"\u0061\u0062"}"#),
            ("abc", "%61%62"),
            ("abc", r"a\bc"),
            ("abc", "100% done \\ not an escape \\u00"),
            // Encodings that are not compared.
            ("abc", "YWJj"),
            ("abc", "%2561%2562%2563"),
        ] {
            assert!(!found(secret, text), "{secret} in {text}");
        }
    }

    #[test]
    fn json_escapes_and_percent_escapes_are_decoded_where_they_stand() {
        assert_eq!(json_unescaped(r#"a\"b\\c\/d"#), r#"a"b\c/d"#);
        assert_eq!(json_unescaped(r"\b\f\n\r\t"), "\u{8}\u{c}\n\r\t");
        assert_eq!(json_unescaped(r"\u0041\u00e9\u30ef"), "Aéワ");
        // A surrogate pair is one character; a lone surrogate is not one.
        assert_eq!(json_unescaped(r"\ud83d\ude00"), "😀");
        assert_eq!(json_unescaped(r"\ud83dx"), "\u{fffd}x");
        // Not escapes: left as they are, and the text after them is still read.
        assert_eq!(
            json_unescaped(r"\x \u12 \u00zz end\"),
            r"\x \u12 \u00zz end\"
        );
        assert_eq!(json_unescaped(r"\\u0041"), r"\u0041");
        assert_eq!(json_unescaped("no escapes"), "no escapes");
        assert_eq!(percent_decoded("a%20b%2Fc%2f"), "a b/c/");
        assert_eq!(percent_decoded("100% %zz %4"), "100% %zz %4");
        assert_eq!(percent_decoded("%E3%83%AF"), "ワ");
        assert_eq!(percent_decoded("%ff"), "\u{fffd}");
    }

    #[test]
    fn a_refused_secret_says_which_rule_failed_and_never_the_value() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path());
        secrets.set(PLUGIN, "kept").unwrap();
        for (secret, rule, words) in [
            (
                "line\nbreak-secret",
                PluginSecretError::Unsendable,
                "control character",
            ),
            (
                "nul\0byte-secret",
                PluginSecretError::Unsendable,
                "control character",
            ),
            (
                "caf\u{e9}-secret",
                PluginSecretError::NotAscii,
                "outside ASCII",
            ),
            (
                "\u{30ef}\u{30ea}\u{30aa}-secret",
                PluginSecretError::NotAscii,
                "outside ASCII",
            ),
            (
                "smart\u{201c}quote-secret",
                PluginSecretError::NotAscii,
                "outside ASCII",
            ),
        ] {
            let error = secrets.set(PLUGIN, secret).unwrap_err();
            assert_eq!(error, rule);
            let text = error.to_string();
            assert!(text.contains(words), "{text}");
            assert!(
                !text.contains("secret-") && !text.contains("-secret"),
                "{text}"
            );
        }
        assert_eq!(secrets.read(PLUGIN), Ok(Some("kept".into())));
        // Every printable ASCII character is a character a key may have.
        let printable: String = (0x20u8..0x7f).map(char::from).collect();
        let printable = printable.trim();
        assert_eq!(
            secrets.set(PLUGIN, printable),
            Ok(SecretSettingStatus::Configured)
        );
        assert_eq!(secrets.read(PLUGIN), Ok(Some(printable.to_owned())));
        // A secret written by another hand that breaks a rule is not used.
        fs::write(private.path().join(PLUGIN).join("credential"), "caf\u{e9}").unwrap();
        assert_eq!(
            secrets.credential(PLUGIN, "www.steamgriddb.com").err(),
            Some(PluginSecretError::NotAscii)
        );
    }

    #[test]
    fn plugins_with_a_secret_lists_only_folders_that_hold_one() {
        let private = tempfile::tempdir().unwrap();
        let secrets = PluginSecrets::new(private.path().join("not-made-yet"));
        assert_eq!(secrets.plugins_with_a_secret(), Ok(Vec::new()));
        let secrets = PluginSecrets::new(private.path());
        secrets.set("@other:art", "two").unwrap();
        secrets.set(PLUGIN, "one").unwrap();
        secrets.set("@gone:cleared", "three").unwrap();
        secrets.clear("@gone:cleared").unwrap();
        // korrid's other private state is not a plugin folder.
        fs::create_dir_all(private.path().join("identity")).unwrap();
        fs::write(private.path().join("identity/credential"), b"x").unwrap();
        fs::create_dir_all(private.path().join("game-assets")).unwrap();
        assert_eq!(
            secrets.plugins_with_a_secret(),
            Ok(vec![PLUGIN.to_owned(), "@other:art".to_owned()])
        );
    }

    #[test]
    fn an_id_that_is_not_one_plugin_id_never_becomes_a_path() {
        let outside = tempfile::tempdir().unwrap();
        let private = outside.path().join("private");
        fs::create_dir(&private).unwrap();
        let secrets = PluginSecrets::new(&private);
        for id in [
            "",
            "steamgriddb",
            "@korri",
            "@korri:",
            "@:steamgriddb",
            "@korri:../identity",
            "@korri:a/b",
            "../@korri:steamgriddb",
            "@korri:.hidden",
            "@Korri:steamgriddb",
            "@korri:steam griddb",
            "identity",
        ] {
            assert_eq!(
                secrets.set(id, "key"),
                Err(PluginSecretError::InvalidPluginId),
                "{id}"
            );
            assert_eq!(
                secrets.read(id),
                Err(PluginSecretError::InvalidPluginId),
                "{id}"
            );
            assert_eq!(
                secrets.clear(id),
                Err(PluginSecretError::InvalidPluginId),
                "{id}"
            );
        }
        // Nothing was written anywhere.
        assert_eq!(fs::read_dir(&private).unwrap().count(), 0);
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
    }

    #[test]
    fn no_error_text_carries_the_secret() {
        let private = tempfile::tempdir().unwrap();
        // The private root is a file: every write fails.
        let blocked = private.path().join("blocked");
        fs::write(&blocked, b"").unwrap();
        let secrets = PluginSecrets::new(&blocked);
        let failure = secrets.set(PLUGIN, "very-secret-value").unwrap_err();
        assert_eq!(failure, PluginSecretError::Storage);
        assert!(!failure.to_string().contains("very-secret-value"));
        assert!(!format!("{failure:?}").contains("very-secret-value"));
    }
}
