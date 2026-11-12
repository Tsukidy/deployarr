use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
    println!("cipher suites = {}", config.crypto_provider().cipher_suites.len());
    Ok(())
}
