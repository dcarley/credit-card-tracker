use crate::truelayer::types::{TrueLayerCard, TrueLayerProvider};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub provider: Provider,
}

impl From<TrueLayerCard> for Card {
    fn from(tl: TrueLayerCard) -> Self {
        Card {
            id: tl.account_id,
            name: tl.display_name,
            provider: tl.provider.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    pub id: String,
    pub name: String,
}

impl From<TrueLayerProvider> for Provider {
    fn from(tl: TrueLayerProvider) -> Self {
        Provider {
            id: tl.provider_id,
            name: tl.display_name,
        }
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;

    pub(crate) fn mock_card() -> Card {
        Card {
            id: "acc_123".to_string(),
            name: "Amex Card".to_string(),
            provider: Provider {
                id: "amex".to_string(),
                name: "American Express".to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_serialization() {
        let card = test_helpers::mock_card();
        let json = serde_json::to_string(&card).unwrap();
        let deserialized: Card = serde_json::from_str(&json).unwrap();

        assert_eq!(card, deserialized);
    }
}
