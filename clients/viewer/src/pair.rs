// SPDX-License-Identifier: GPL-3.0-or-later
//! The existing GameStream PIN exchange. No product certificate provisioning.
use crate::session::{Identity, Transport, required};
use anyhow::{Result, ensure};
use openssl::{
    hash::{MessageDigest, hash},
    sign::{Signer, Verifier},
    symm::{Cipher, Crypter, Mode},
    x509::X509,
};
use std::io::Write;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Result<Vec<u8>> {
    ensure!(
        s.len().is_multiple_of(2) && s.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid hex in pairing response"
    );
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(Into::into))
        .collect()
}
fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    openssl::rand::rand_bytes(&mut bytes)?;
    Ok(bytes)
}
fn crypt(data: &[u8], key: &[u8], mode: Mode) -> Result<Vec<u8>> {
    let mut c = Crypter::new(Cipher::aes_128_ecb(), mode, &key[..16], None)?;
    c.pad(false);
    let mut out = vec![0; data.len() + 16];
    let n = c.update(data, &mut out)?;
    let end = c.finalize(&mut out[n..])?;
    out.truncate(n + end);
    Ok(out)
}
fn step(t: &mut Transport, params: &[(&str, String)], https: bool) -> Result<String> {
    let mut params = params.to_vec();
    params.extend([("devicename", "roth".into()), ("updateState", "1".into())]);
    let xml = t.get(https, "pair", &params)?;
    ensure!(required(&xml, "paired")? == "1", "pairing refused");
    Ok(xml)
}
pub fn pair(t: &mut Transport, identity: &Identity) -> Result<()> {
    let pin = format!("{:04}", u16::from_be_bytes(random::<2>()?) % 10000);
    println!("Please enter the following PIN on the target PC: {pin}");
    std::io::stdout().flush()?;
    let salt = random::<16>()?;
    let xml = step(
        t,
        &[
            ("phrase", "getservercert".into()),
            ("salt", hex(&salt)),
            ("clientcert", hex(&identity.cert.to_pem()?)),
        ],
        false,
    )?;
    let server = X509::from_pem(&unhex(&required(&xml, "plaincert")?)?)?;
    let mut salt_pin = salt.to_vec();
    salt_pin.extend(pin.as_bytes());
    // The supported Sunshine producer uses v7 / SHA256 pairing, as the
    // plugin's gs_pair does for serverMajorVersion >= 7.
    let key = hash(MessageDigest::sha256(), &salt_pin)?;
    let challenge = random::<16>()?;
    let xml = step(
        t,
        &[(
            "clientchallenge",
            hex(&crypt(&challenge, &key, Mode::Encrypt)?),
        )],
        false,
    )?;
    let response = crypt(
        &unhex(&required(&xml, "challengeresponse")?)?,
        &key,
        Mode::Decrypt,
    )?;
    ensure!(response.len() >= 48, "short challenge response");
    let secret = random::<16>()?;
    let mut answer = response[32..48].to_vec();
    answer.extend(identity.cert.signature().as_slice());
    answer.extend(secret);
    let answer_hash = hash(MessageDigest::sha256(), &answer)?;
    let xml = step(
        t,
        &[(
            "serverchallengeresp",
            hex(&crypt(&answer_hash, &key, Mode::Encrypt)?),
        )],
        false,
    )?;
    let server_secret = unhex(&required(&xml, "pairingsecret")?)?;
    ensure!(server_secret.len() > 16, "short pairing secret");
    let server_key = server.public_key()?;
    let mut verifier = Verifier::new(MessageDigest::sha256(), &server_key)?;
    verifier.update(&server_secret[..16])?;
    ensure!(
        verifier.verify(&server_secret[16..])?,
        "server pairing signature mismatch"
    );
    let mut proof = challenge.to_vec();
    proof.extend(server.signature().as_slice());
    proof.extend(&server_secret[..16]);
    ensure!(
        openssl::memcmp::eq(&hash(MessageDigest::sha256(), &proof)?, &response[..32]),
        "server pairing challenge mismatch"
    );
    let mut signer = Signer::new(MessageDigest::sha256(), &identity.key)?;
    signer.update(&secret)?;
    let mut client_secret = secret.to_vec();
    client_secret.extend(signer.sign_to_vec()?);
    step(t, &[("clientpairingsecret", hex(&client_secret))], false)?;
    // TLS here checks the certificate already supplied by the fixture/broker,
    // not a certificate accepted from the unauthenticated HTTP exchange.
    step(t, &[("phrase", "pairchallenge".into())], true)?;
    println!("Successfully paired");
    Ok(())
}
