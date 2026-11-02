//! Its own test binary, so its own process: no rustls crypto provider is installed when this
//! test starts. reqwest is built with `rustls-no-provider`, so building a client with nothing
//! installed would panic. `ReqwestForgeApi` must install ring itself before its first build.

use repo_service::pull_requests::{
    ForgeApi, ForgeApiError, ForgeHttpMethod, ForgeHttpRequest, ReqwestForgeApi,
};

#[test]
fn the_forge_client_installs_a_crypto_provider_before_its_first_request() {
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());

    // Port 1 on loopback: nothing listens there, so the request fails fast after the client
    // was built. Plain HTTP, but reqwest still sets up its TLS config when building the client.
    let result = ReqwestForgeApi::new().send(ForgeHttpRequest {
        method: ForgeHttpMethod::Get,
        url: "http://127.0.0.1:1/".to_string(),
        headers: Vec::new(),
        json_body: None,
    });

    assert_eq!(result, Err(ForgeApiError::Transport));
    assert!(rustls::crypto::CryptoProvider::get_default().is_some());
}
