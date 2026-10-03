use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Error;

macro_rules! id {
    ($name:ident, $kind:literal, $valid:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn parse(raw: impl Into<String>) -> Result<Self, Error> {
                let raw = raw.into();
                let valid: fn(&str) -> bool = $valid;
                if !raw.is_empty() && !raw.contains(char::is_whitespace) && valid(&raw) {
                    Ok(Self(raw))
                } else {
                    Err(Error::InvalidId { kind: $kind, value: raw })
                }
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::str::FromStr for $name {
            type Err = Error;

            fn from_str(raw: &str) -> Result<Self, Error> {
                Self::parse(raw)
            }
        }

        impl TryFrom<String> for $name {
            type Error = Error;

            fn try_from(raw: String) -> Result<Self, Error> {
                Self::parse(raw)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id!(AccountId, "account", |_| true);
id!(SecurityId, "security", |raw| raw.starts_with("sec-"));
id!(OrderId, "order", |_| true);
id!(IdentityId, "identity", |raw| raw.starts_with("identity-"));
id!(IdempotencyKey, "idempotency key", |raw| raw.starts_with("order-"));

impl IdempotencyKey {
    /// Sent to Wealthsimple as the order's `externalId`, so a retry with the same key is a no-op.
    #[must_use]
    pub fn fresh() -> Self {
        Self(format!("order-{}", Uuid::new_v4()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_id_requires_prefix() {
        assert!(SecurityId::parse("sec-s-76a7155242e8477880cbb43269235cb6").is_ok());
        assert!(SecurityId::parse("XEQT").is_err());
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert!(AccountId::parse("").is_err());
        assert!(AccountId::parse("tfsa 123").is_err());
    }

    #[test]
    fn fresh_keys_are_unique_and_valid() {
        let a = IdempotencyKey::fresh();
        let b = IdempotencyKey::fresh();
        assert_ne!(a, b);
        assert!(IdempotencyKey::parse(a.as_str()).is_ok());
    }

    #[test]
    fn deserialization_validates() {
        assert!(serde_json::from_str::<SecurityId>(r#""nope""#).is_err());
    }
}
