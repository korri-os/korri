// SPDX-License-Identifier: GPL-3.0-or-later
// GameStream's HTTP protocol, extracted from Moonlight Embedded libgamestream/client.c.
use anyhow::{Context, Result, bail, ensure};
use curl::easy::Easy;
use openssl::{
    asn1::Asn1Time,
    hash::MessageDigest,
    pkey::{PKey, Private},
    rsa::Rsa,
    x509::{X509, X509NameBuilder},
};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
    time::Duration,
};

pub struct Identity {
    pub cert: X509,
    pub key: PKey<Private>,
}
impl Identity {
    pub fn load(dir: &Path) -> Result<Self> {
        let cert = X509::from_pem(&fs::read(dir.join("client.pem"))?)?;
        let key = PKey::private_key_from_pem(&fs::read(dir.join("key.pem"))?)?;
        ensure!(
            cert.public_key()?.public_eq(&key),
            "client certificate and key do not match"
        );
        Ok(Self { cert, key })
    }
    pub fn load_or_generate(dir: &Path) -> Result<Self> {
        if dir.join("client.pem").exists() || dir.join("key.pem").exists() {
            return Self::load(dir);
        }
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        let key = PKey::from_rsa(Rsa::generate(2048)?)?;
        let mut name = X509NameBuilder::new()?;
        name.append_entry_by_text("CN", "NVIDIA GameStream Client")?;
        let name = name.build();
        let mut cert = X509::builder()?;
        cert.set_version(2)?;
        cert.set_subject_name(&name)?;
        cert.set_issuer_name(&name)?;
        cert.set_not_before(Asn1Time::days_from_now(0)?.as_ref())?;
        cert.set_not_after(Asn1Time::days_from_now(3650)?.as_ref())?;
        cert.set_pubkey(&key)?;
        cert.sign(&key, MessageDigest::sha256())?;
        let cert = cert.build();
        for (name, bytes) in [
            ("key.pem", key.private_key_to_pem_pkcs8()?),
            ("client.pem", cert.to_pem()?),
        ] {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(dir.join(name))?
                .write_all(&bytes)?;
        }
        Ok(Self { cert, key })
    }
}

pub struct Transport {
    easy: Easy,
    host: String,
    http_port: u16,
    pub https_port: u16,
    unique_id: String,
}
impl Transport {
    pub fn new(host: &str, port: u16, keys: &Path, pin: &Path) -> Result<Self> {
        let cert = X509::from_pem(&fs::read(pin).context("read authorized server certificate")?)
            .context("parse authorized server certificate")?;
        // Same trust anchor AND SPKI check as plugin patch 0020. A Sunshine
        // certificate's CN is not a DNS identity. Never disable peer verification.
        let spki = cert.public_key()?.public_key_to_pem()?;
        let mut easy = Easy::new();
        // Sunshine requires a fresh mutual-TLS authentication on each new
        // connection. Moonlight http_init disables the session cache too.
        easy.ssl_sessionid_cache(false)?;
        easy.ssl_verify_peer(true)?;
        easy.ssl_verify_host(false)?;
        easy.cainfo(pin)?;
        // libcurl accepts PEM public keys too. Keep the bytes alive by passing
        // a SHA-256 pin, rather than a second mutable pin file.
        let key = PKey::public_key_from_pem(&spki)?.public_key_to_der()?;
        let digest = openssl::sha::sha256(&key);
        let encoded = openssl::base64::encode_block(&digest);
        easy.pinned_public_key(&format!("sha256//{encoded}"))?;
        easy.ssl_cert(keys.join("client.pem"))?;
        easy.ssl_key(keys.join("key.pem"))?;
        easy.connect_timeout(Duration::from_secs(10))?;
        easy.timeout(Duration::from_secs(120))?;
        easy.fail_on_error(true)?;
        easy.proxy("")?;
        easy.follow_location(false)?;
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.into()
        };
        // Moonlight's existing producer uses this ID. Pair and stream must use
        // the same value, including with a keydir provisioned by korrid.
        let unique_id = match fs::read_to_string(keys.join("uniqueid.dat")) {
            Ok(id) => {
                ensure!(
                    id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()),
                    "invalid uniqueid.dat"
                );
                id
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "0123456789ABCDEF".into(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            easy,
            host,
            http_port: port,
            https_port: 47984,
            unique_id,
        })
    }
    pub fn get(
        &mut self,
        https: bool,
        endpoint: &str,
        params: &[(&str, String)],
    ) -> Result<String> {
        let mut url = format!(
            "{}://{}:{}/{}?uniqueid={}&uuid={}",
            if https { "https" } else { "http" },
            self.host,
            if https {
                self.https_port
            } else {
                self.http_port
            },
            endpoint,
            self.unique_id,
            uuid::Uuid::new_v4()
        );
        for (key, value) in params {
            url.push_str(&format!(
                "&{key}={}",
                self.easy.url_encode(value.as_bytes())
            ));
        }
        self.easy.url(&url)?;
        let mut bytes = Vec::new();
        {
            let mut transfer = self.easy.transfer();
            transfer.write_function(|data| {
                if bytes.len() + data.len() > 1024 * 1024 {
                    return Ok(0);
                }
                bytes.extend_from_slice(data);
                Ok(data.len())
            })?;
            transfer
                .perform()
                .with_context(|| format!("{endpoint} request failed"))?;
        }
        let xml = String::from_utf8(bytes)?;
        let doc = roxmltree::Document::parse(&xml)?;
        let root = doc.root_element();
        ensure!(
            root.attribute("status_code") == Some("200"),
            "{endpoint}: server rejected request ({})",
            root.attribute("status_code").unwrap_or("missing status")
        );
        Ok(xml)
    }
    pub fn discover_port(&mut self) -> Result<()> {
        // HTTP is used only for port discovery and the PIN exchange, never as
        // a fallback for authenticated serverinfo, applist, launch or resume.
        let xml = self.get(false, "serverinfo", &[])?;
        if let Some(port) = field(&xml, "HttpsPort")? {
            self.https_port = port.parse()?;
            ensure!(self.https_port > 0, "invalid HTTPS port");
        }
        Ok(())
    }
}

pub fn field(xml: &str, name: &str) -> Result<Option<String>> {
    let doc = roxmltree::Document::parse(xml)?;
    Ok(doc
        .descendants()
        .find(|n| n.has_tag_name(name))
        .and_then(|n| n.text())
        .map(str::to_owned))
}
pub fn required(xml: &str, name: &str) -> Result<String> {
    field(xml, name)?
        .filter(|s| !s.is_empty())
        .with_context(|| format!("missing {name}"))
}
pub fn app_id(xml: &str, name: &str) -> Result<u32> {
    let doc = roxmltree::Document::parse(xml)?;
    for app in doc.descendants().filter(|n| n.has_tag_name("App")) {
        if app
            .children()
            .any(|n| n.has_tag_name("AppTitle") && n.text() == Some(name))
        {
            return app
                .children()
                .find(|n| n.has_tag_name("ID"))
                .and_then(|n| n.text())
                .context("missing app ID")?
                .parse()
                .context("invalid app ID");
        }
    }
    bail!("application not found: {name}")
}
