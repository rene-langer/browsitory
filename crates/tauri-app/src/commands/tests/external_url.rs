use super::super::web_url;

#[test]
fn opens_http_and_https_urls() {
    assert_eq!(
        web_url("https://example.com/owner/repo/issues/1"),
        Ok("https://example.com/owner/repo/issues/1".to_string())
    );
    assert_eq!(
        web_url("HTTP://example.com"),
        Ok("http://example.com/".to_string())
    );
}

#[test]
fn refuses_other_schemes_and_non_urls() {
    for url in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "ssh://example.com/repo",
        "mailto:someone@example.com",
        "example.com",
        "",
    ] {
        let error = web_url(url).expect_err(url);
        assert_eq!(
            error.message,
            format!("Only http and https links can be opened: {url}")
        );
    }
}
