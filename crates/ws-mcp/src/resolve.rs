use ws_broker::Reader;
use ws_core::{Account, AccountId, Security, SecurityId};

use crate::action::Failure;

/// An account by id, nickname, or type such as `tfsa`. A name that fits more than one account is refused.
pub async fn account(reader: &dyn Reader, name: &str) -> Result<AccountId, Failure> {
    let accounts = reader.accounts().await?;
    if let Some(account) = accounts.iter().find(|a| a.id().as_str() == name) {
        return Ok(account.id().clone());
    }
    let wanted = normalize(name);
    let named: Vec<&Account> =
        accounts.iter().filter(|a| a.nickname().is_some_and(|n| normalize(n) == wanted)).collect();
    let matches = if named.is_empty() {
        accounts.iter().filter(|a| normalize(&a.registration().to_string()) == wanted).collect()
    } else {
        named
    };
    match matches.as_slice() {
        [one] => Ok(one.id().clone()),
        [] => Err(Failure::Unresolved(format!("no account called {name}. Accounts are {}", listed(accounts.iter())))),
        many => Err(Failure::Unresolved(format!(
            "{name} fits more than one account: {}. Use a nickname or the id",
            listed(many.iter().copied())
        ))),
    }
}

pub struct Pick {
    pub id: SecurityId,
    /// Like `SHOP:TSX`, when the security was found by ticker.
    pub listing: Option<String>,
}

/// A security by id, ticker such as `XEQT`, or ticker and exchange such as `SHOP:TSX`. A ticker listed on more
/// than one exchange is refused.
pub async fn security(reader: &dyn Reader, name: &str) -> Result<Pick, Failure> {
    if name.starts_with("sec-") {
        return Ok(Pick { id: SecurityId::parse(name)?, listing: None });
    }
    let (symbol, exchange) = name.split_once(':').map_or((name, None), |(s, e)| (s, Some(e)));
    let found = reader.search(symbol).await?;
    let matches: Vec<&Security> = found
        .iter()
        .filter(|s| s.symbol().eq_ignore_ascii_case(symbol))
        .filter(|s| exchange.is_none_or(|e| s.exchange().is_some_and(|x| x.eq_ignore_ascii_case(e))))
        .collect();
    match matches.as_slice() {
        [one] => Ok(Pick { id: one.id().clone(), listing: Some(listing(one)) }),
        [] => Err(Failure::Unresolved(format!("no security with the ticker {name}. Try `search`"))),
        many => {
            let listings = many.iter().map(|s| format!("{} ({})", listing(s), s.name())).collect::<Vec<_>>().join(", ");
            Err(Failure::Unresolved(format!("{name} is listed more than once: {listings}. Use one of those")))
        }
    }
}

/// Like `Main (TFSA, CAD)`.
pub fn label(account: &Account) -> String {
    let kind = account.registration();
    match account.nickname() {
        Some(name) => format!("{name} ({kind}, {})", account.currency()),
        None => format!("{kind} ({})", account.currency()),
    }
}

fn listed<'a>(accounts: impl Iterator<Item = &'a Account>) -> String {
    accounts.map(|a| format!("{} [{}]", label(a), a.id())).collect::<Vec<_>>().join(", ")
}

fn listing(security: &Security) -> String {
    match security.exchange() {
        Some(exchange) => format!("{}:{exchange}", security.symbol()),
        None => security.symbol().to_owned(),
    }
}

fn normalize(name: &str) -> String {
    name.trim().to_lowercase().replace([' ', '-'], "_")
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use ws_broker::Error;
    use ws_core::{Activity, Currency, IdempotencyKey, Management, Placed, Position, Quote, Registration, Report};

    use super::*;

    struct Book;

    fn account(id: &str, registration: Registration, nickname: Option<&str>) -> Account {
        let id = AccountId::parse(id).unwrap_or_else(|e| panic!("{e}"));
        let account = Account::new(id, registration, Management::SelfDirected, Currency::Cad);
        match nickname {
            Some(name) => account.named(name),
            None => account,
        }
    }

    fn listed_on(id: &str, symbol: &str, exchange: &str) -> Security {
        Security::new(SecurityId::parse(id).unwrap_or_else(|e| panic!("{e}")), symbol, "Some Co").on(exchange)
    }

    #[async_trait]
    impl Reader for Book {
        async fn accounts(&self) -> Result<Vec<Account>, Error> {
            Ok(vec![
                account("tfsa-1", Registration::Tfsa, Some("Main")),
                account("rrsp-1", Registration::Rrsp, None),
                account("rrsp-2", Registration::Rrsp, Some("Spare")),
                account("nr-1", Registration::NonRegistered, None),
            ])
        }

        async fn positions(&self, _: &AccountId) -> Result<Vec<Position>, Error> {
            Ok(Vec::new())
        }

        async fn activities(&self, _: &AccountId, _: usize) -> Result<Vec<Activity>, Error> {
            Ok(Vec::new())
        }

        async fn search(&self, query: &str) -> Result<Vec<Security>, Error> {
            Ok(match query.to_uppercase().as_str() {
                "XEQT" => vec![listed_on("sec-s-xeqt", "XEQT", "TSX"), listed_on("sec-s-xeqtx", "XEQTX", "TSX")],
                "SHOP" => vec![listed_on("sec-s-shopt", "SHOP", "TSX"), listed_on("sec-s-shopn", "SHOP", "NYSE")],
                _ => Vec::new(),
            })
        }

        async fn quote(&self, security: &SecurityId) -> Result<Quote, Error> {
            Err(Error::NotFound(security.to_string()))
        }

        async fn pending_orders(&self, _: &AccountId) -> Result<Vec<Placed>, Error> {
            Ok(Vec::new())
        }

        async fn order(&self, key: &IdempotencyKey) -> Result<Report, Error> {
            Err(Error::NotFound(key.to_string()))
        }
    }

    async fn account_named(name: &str) -> Result<String, String> {
        super::account(&Book, name).await.map(|id| id.to_string()).map_err(|e| e.to_string())
    }

    async fn security_named(name: &str) -> Result<String, String> {
        super::security(&Book, name).await.map(|pick| pick.id.to_string()).map_err(|e| e.to_string())
    }

    #[tokio::test]
    async fn finds_accounts_by_id_nickname_or_unique_type() {
        assert_eq!(account_named("rrsp-1").await, Ok("rrsp-1".into()));
        assert_eq!(account_named("main").await, Ok("tfsa-1".into()));
        assert_eq!(account_named("TFSA").await, Ok("tfsa-1".into()));
        assert_eq!(account_named("non-registered").await, Ok("nr-1".into()));
        assert!(account_named("rrsp").await.is_err_and(|e| e.contains("more than one") && e.contains("[rrsp-2]")));
        assert!(account_named("fhsa").await.is_err_and(|e| e.contains("no account called fhsa")));
    }

    #[tokio::test]
    async fn finds_securities_by_exact_ticker_and_exchange() {
        assert_eq!(security_named("xeqt").await, Ok("sec-s-xeqt".into()));
        assert_eq!(security_named("sec-s-anything").await, Ok("sec-s-anything".into()));
        assert!(security_named("SHOP").await.is_err_and(|e| e.contains("SHOP:TSX") && e.contains("SHOP:NYSE")));
        assert_eq!(security_named("shop:nyse").await, Ok("sec-s-shopn".into()));
        assert_eq!(super::security(&Book, "xeqt").await.ok().and_then(|pick| pick.listing), Some("XEQT:TSX".into()));
        assert!(security_named("NOPE").await.is_err_and(|e| e.contains("no security")));
    }
}
