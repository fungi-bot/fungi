use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use reqwest::Client;

static HTTP_CLIENT: OnceCell<Client> = OnceCell::new();

pub(crate) fn http_client() -> Result<&'static Client> {
    HTTP_CLIENT.get_or_try_init(build_http_client)
}

fn build_http_client() -> Result<Client> {
    let builder = Client::builder();

    // The daemon runs as a standalone native process on Android, so it has no JVM or Android
    // Context with which to initialize reqwest 0.13's default rustls-platform-verifier. Use an
    // explicit WebPKI configuration there. Other platforms retain reqwest's platform verifier.
    #[cfg(target_os = "android")]
    let builder = builder.tls_backend_preconfigured(android_tls_config()?);

    builder.build().context("failed to build HTTP client")
}

#[cfg(target_os = "android")]
fn android_tls_config() -> Result<rustls::ClientConfig> {
    use std::sync::Arc;

    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let provider = rustls::crypto::aws_lc_rs::default_provider();

    Ok(
        rustls::ClientConfig::builder_with_provider(Arc::new(provider))
            .with_safe_default_protocol_versions()
            .context("failed to configure Android TLS protocol versions")?
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_shared_http_client() {
        let first = http_client().unwrap();
        let second = http_client().unwrap();
        assert!(std::ptr::eq(first, second));
    }

    #[cfg(target_os = "android")]
    #[tokio::test]
    async fn android_client_verifies_public_https_without_jvm() {
        let response = http_client()
            .unwrap()
            .get("https://github.com")
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
    }
}
