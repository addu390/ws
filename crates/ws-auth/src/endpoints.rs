//! Where the login page and OAuth API live.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    login: String,
    oauth: String,
}

impl Endpoints {
    #[must_use]
    pub fn production() -> Self {
        Self {
            login: "https://my.wealthsimple.com/app/login".to_owned(),
            oauth: "https://api.production.wealthsimple.com/v1/oauth/v2".to_owned(),
        }
    }

    /// Everything under one base URL, for tests against a mock server.
    #[must_use]
    pub fn at(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self { login: format!("{base}/app/login"), oauth: format!("{base}/oauth") }
    }

    #[must_use]
    pub fn login(&self) -> &str {
        &self.login
    }

    #[must_use]
    pub fn token(&self) -> String {
        format!("{}/token", self.oauth)
    }

    #[must_use]
    pub fn token_info(&self) -> String {
        format!("{}/token/info", self.oauth)
    }

    /// Resolves a script `src` found on the login page, which may be relative.
    #[must_use]
    pub fn resolve(&self, src: &str) -> String {
        if src.starts_with("http://") || src.starts_with("https://") {
            return src.to_owned();
        }
        let origin_end = self.login.find("://").map_or(0, |i| i + 3);
        let origin = self.login[origin_end..].find('/').map_or(self.login.as_str(), |i| &self.login[..origin_end + i]);
        if let Some(rest) = src.strip_prefix("//") {
            let scheme = &self.login[..origin_end];
            format!("{scheme}{rest}")
        } else {
            format!("{origin}/{}", src.trim_start_matches('/'))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_script_sources() {
        let endpoints = Endpoints::production();
        assert_eq!(endpoints.resolve("https://cdn.example/app-1.js"), "https://cdn.example/app-1.js");
        assert_eq!(endpoints.resolve("//cdn.example/app-1.js"), "https://cdn.example/app-1.js");
        assert_eq!(endpoints.resolve("/assets/app-1.js"), "https://my.wealthsimple.com/assets/app-1.js");
    }

    #[test]
    fn builds_oauth_urls() {
        let endpoints = Endpoints::at("http://127.0.0.1:9/");
        assert_eq!(endpoints.token(), "http://127.0.0.1:9/oauth/token");
        assert_eq!(endpoints.token_info(), "http://127.0.0.1:9/oauth/token/info");
    }
}
