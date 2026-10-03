use std::future::Future;

use serde::Serialize;

pub trait Action {
    const SUMMARY: &'static str;

    type Context: ?Sized + Sync;

    type Output: Serialize;

    fn run(self, context: &Self::Context) -> impl Future<Output = Result<Self::Output, Failure>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error(transparent)]
    Broker(#[from] ws_broker::Error),

    #[error(transparent)]
    Policy(#[from] ws_policy::Error),

    #[error(transparent)]
    Domain(#[from] ws_core::Error),

    #[error("{0}")]
    Invalid(&'static str),

    #[error("{0}")]
    Unresolved(String),

    #[error("could not encode the result: {0}")]
    Encode(#[from] serde_json::Error),
}

pub async fn perform<A: Action>(action: A, context: &A::Context) -> Result<String, Failure> {
    let output = action.run(context).await?;
    Ok(serde_json::to_string_pretty(&output)?)
}
