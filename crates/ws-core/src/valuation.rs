use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Money;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Valuation {
    value: Money,
    net_deposits: Option<Money>,
    gain: Option<Money>,
    rate: Option<Decimal>,
}

impl Valuation {
    #[must_use]
    pub fn new(value: Money) -> Self {
        Self { value, net_deposits: None, gain: None, rate: None }
    }

    #[must_use]
    pub fn with_net_deposits(mut self, net_deposits: Money) -> Self {
        self.net_deposits = Some(net_deposits);
        self
    }

    #[must_use]
    pub fn with_return(mut self, gain: Money, rate: Option<Decimal>) -> Self {
        self.gain = Some(gain);
        self.rate = rate;
        self
    }

    #[must_use]
    pub fn value(&self) -> Money {
        self.value
    }

    #[must_use]
    pub fn net_deposits(&self) -> Option<Money> {
        self.net_deposits
    }

    #[must_use]
    pub fn gain(&self) -> Option<Money> {
        self.gain
    }

    #[must_use]
    pub fn rate(&self) -> Option<Decimal> {
        self.rate
    }
}
