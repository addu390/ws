use wreq::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};

use crate::Error;

#[derive(Debug, Clone, Default)]
pub struct Headers {
    pairs: Vec<(String, String)>,
}

impl Headers {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.pairs.push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn session(self, id: &str) -> Self {
        self.with("x-ws-session-id", id)
    }

    #[must_use]
    pub fn device(self, id: &str) -> Self {
        self.with("x-ws-device-id", id)
    }

    #[must_use]
    pub fn bearer(self, token: &str) -> Self {
        self.with(AUTHORIZATION.as_str(), format!("Bearer {token}"))
    }

    pub(crate) fn to_map(&self) -> Result<HeaderMap, Error> {
        let mut map = HeaderMap::with_capacity(self.pairs.len());
        for (name, value) in &self.pairs {
            let invalid = || Error::Header { name: name.clone() };
            let key = HeaderName::try_from(name.as_str()).map_err(|_| invalid())?;
            let mut value = HeaderValue::try_from(value.as_str()).map_err(|_| invalid())?;
            if key == AUTHORIZATION {
                value.set_sensitive(true);
            }
            map.insert(key, value);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_wealthsimple_headers() {
        let map = Headers::new().session("s-1").device("d-1").bearer("tok").to_map();
        let map = map.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(map["x-ws-session-id"], "s-1");
        assert_eq!(map["x-ws-device-id"], "d-1");
        assert_eq!(map[AUTHORIZATION], "Bearer tok");
        assert!(map[AUTHORIZATION].is_sensitive());
    }

    #[test]
    fn later_values_replace_earlier_ones() {
        let map = Headers::new().with("x-ws-profile", "invest").with("x-ws-profile", "trade").to_map();
        assert_eq!(map.unwrap_or_else(|e| panic!("{e}"))["x-ws-profile"], "trade");
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(matches!(Headers::new().with("x-ok", "bad\nvalue").to_map(), Err(Error::Header { .. })));
    }
}
