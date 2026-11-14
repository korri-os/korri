// SPDX-License-Identifier: GPL-3.0-or-later
//! The pinned server certificate is the only trust anchor. On Android the
//! viewer links the curl crate's own libcurl, and the crate adds the CA
//! directory that it probes for when it makes a handle (`SSL_CERT_DIR` or a
//! system path). On Linux the system libcurl has none.
//!
//! This file holds one test, so that the probe, which runs once in each
//! process, reads the directory set here.
use korri_viewer::session::{Identity, Transport};
use openssl::{
    asn1::Asn1Time,
    bn::BigNum,
    hash::MessageDigest,
    pkey::{PKey, Private},
    rsa::Rsa,
    ssl::{SslAcceptor, SslMethod},
    x509::{X509, X509NameBuilder, extension::BasicConstraints},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    thread,
};

fn certificate(
    subject: &str,
    key: &PKey<Private>,
    issuer: Option<(&X509, &PKey<Private>)>,
) -> X509 {
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", subject).unwrap();
    let name = name.build();
    let mut builder = X509::builder().unwrap();
    builder.set_version(2).unwrap();
    let serial = BigNum::from_u32(rand_serial())
        .unwrap()
        .to_asn1_integer()
        .unwrap();
    builder.set_serial_number(&serial).unwrap();
    builder.set_subject_name(&name).unwrap();
    builder
        .set_issuer_name(issuer.map_or(&name, |(ca, _)| ca.subject_name()))
        .unwrap();
    builder
        .set_not_before(&Asn1Time::days_from_now(0).unwrap())
        .unwrap();
    builder
        .set_not_after(&Asn1Time::days_from_now(30).unwrap())
        .unwrap();
    builder.set_pubkey(key).unwrap();
    if issuer.is_none() {
        builder
            .append_extension(BasicConstraints::new().critical().ca().build().unwrap())
            .unwrap();
    }
    builder
        .sign(
            issuer.map_or(key, |(_, ca_key)| ca_key),
            MessageDigest::sha256(),
        )
        .unwrap();
    builder.build()
}

fn rand_serial() -> u32 {
    let mut bytes = [0; 4];
    openssl::rand::rand_bytes(&mut bytes).unwrap();
    u32::from_be_bytes(bytes) >> 1
}

fn key() -> PKey<Private> {
    PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap()
}

/// A host that presents `certificate` with `key` and answers serverinfo.
fn host(certificate: X509, key: &PKey<Private>) -> (u16, thread::JoinHandle<()>) {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    acceptor.set_private_key(key).unwrap();
    acceptor.set_certificate(&certificate).unwrap();
    let acceptor = acceptor.build();
    let handle = thread::spawn(move || {
        let (socket, _) = server.accept().unwrap();
        if let Ok(mut tls) = acceptor.accept(socket) {
            let mut request = [0; 4096];
            if tls.read(&mut request).unwrap_or(0) > 0 {
                let body = "<root status_code=\"200\"><PairStatus>1</PairStatus></root>";
                let _ = write!(
                    tls,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        }
    });
    (port, handle)
}

fn serverinfo(keys: &Path, pin: &Path, port: u16) -> anyhow::Result<String> {
    let mut transport = Transport::new("127.0.0.1", port, keys, pin)?;
    transport.https_port = port;
    transport.get(true, "serverinfo", &[])
}

#[test]
fn a_ca_in_the_probed_directory_does_not_vouch_for_a_server() {
    let dir = tempfile::tempdir().unwrap();
    let keys = dir.path().join("keys");
    Identity::load_or_generate(&keys).unwrap();
    // A CA that the probe finds, in OpenSSL's hashed directory form.
    let authority_key = key();
    let authority = certificate("probed authority", &authority_key, None);
    let directory = dir.path().join("certs");
    std::fs::create_dir(&directory).unwrap();
    let hash = authority.subject_name_hash();
    std::fs::write(
        directory.join(format!("{hash:08x}.0")),
        authority.to_pem().unwrap(),
    )
    .unwrap();
    // SAFETY: the first statement that can make a curl handle in this
    // process is below, and no other thread runs yet.
    unsafe { std::env::set_var("SSL_CERT_DIR", &directory) };
    // The pin: a self-signed certificate with the server's key.
    let server_key = key();
    let pinned = certificate("pinned server", &server_key, None);
    let pin = dir.path().join("server.pem");
    std::fs::write(&pin, pinned.to_pem().unwrap()).unwrap();

    // The pinned certificate itself is accepted.
    let (port, accepted) = host(pinned, &server_key);
    assert!(
        serverinfo(&keys, &pin, port)
            .unwrap()
            .contains("PairStatus")
    );
    accepted.join().unwrap();

    // The same key in a certificate from the probed CA passes the key pin,
    // and only a CA directory could anchor it. It is refused.
    let vouched = certificate(
        "pinned server",
        &server_key,
        Some((&authority, &authority_key)),
    );
    let (port, refused) = host(vouched, &server_key);
    let result = serverinfo(&keys, &pin, port);
    assert!(
        result.is_err(),
        "a CA directory anchored the server: {result:?}"
    );
    refused.join().unwrap();
}
