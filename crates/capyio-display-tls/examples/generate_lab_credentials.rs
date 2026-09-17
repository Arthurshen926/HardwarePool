use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let output = PathBuf::from(arguments.next().ok_or(
        "usage: generate_lab_credentials <existing-empty-output-directory> <server-ip-or-dns>",
    )?);
    let server_name = arguments
        .next()
        .ok_or("missing server IP address or DNS name")?
        .into_string()
        .map_err(|_| "server name must be valid Unicode")?;
    if arguments.next().is_some() || server_name.is_empty() || server_name.len() > 253 {
        return Err("invalid arguments".into());
    }
    if !output.is_dir() || fs::read_dir(&output)?.next().is_some() {
        return Err("output directory must already exist and be empty".into());
    }

    let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
    ca_params.distinguished_name = distinguished_name("CapyIO Display Lab CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let ca_key = KeyPair::generate()?;
    let ca_cert = ca_params.self_signed(&ca_key)?;
    let issuer = Issuer::new(ca_params, ca_key);

    let (server_cert, server_key) =
        leaf(&issuer, &server_name, ExtendedKeyUsagePurpose::ServerAuth)?;
    let (client_cert, client_key) = leaf(
        &issuer,
        "capyio-android-lab",
        ExtendedKeyUsagePurpose::ClientAuth,
    )?;

    write_new(&output.join("ca.der"), ca_cert.der())?;
    write_new(&output.join("server-cert.der"), &server_cert)?;
    write_new(&output.join("server-key.pk8"), &server_key)?;
    write_new(&output.join("client-cert.der"), &client_cert)?;
    write_new(&output.join("client-key.pk8"), &client_key)?;
    println!("generated lab-only credentials for {server_name}");
    Ok(())
}

fn leaf(
    issuer: &Issuer<'static, KeyPair>,
    name: &str,
    usage: ExtendedKeyUsagePurpose,
) -> Result<(Vec<u8>, Vec<u8>), rcgen::Error> {
    let mut params = CertificateParams::new(vec![name.to_owned()])?;
    params.distinguished_name = distinguished_name(match usage {
        ExtendedKeyUsagePurpose::ServerAuth => "CapyIO Display Lab Server",
        ExtendedKeyUsagePurpose::ClientAuth => "CapyIO Android Lab Client",
        _ => "CapyIO Display Lab Leaf",
    });
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![usage];
    let key = KeyPair::generate()?;
    let cert = params.signed_by(&key, issuer)?;
    Ok((cert.der().to_vec(), key.serialize_der()))
}

fn distinguished_name(common_name: &str) -> DistinguishedName {
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, common_name);
    name
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    Ok(())
}
