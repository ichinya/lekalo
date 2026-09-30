//! The measured-value state wrapper of the run history (issue #121).
//!
//! `known` is the only state that carries a value; `unknown`,
//! `withheld`, and `unsupported` are state-only spellings. A missing
//! harness observation normalizes to `unknown` before persistence —
//! never to zero, null, or an empty string — and a known zero
//! round-trips as a known zero. The state vocabulary reuses the #120
//! [`SensitivityState`] labels; the wrapper itself is independent of
//! the privacy decision machinery.

use serde::Deserialize;
use serde::Serialize;

pub use crate::privacy::vocab::SensitivityState;

/// One measured value in one of the four closed states.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Vs<T> {
    /// A measured value (including a known zero).
    Known(T),
    /// Not observed by any source.
    Unknown,
    /// Observed but intentionally withheld by the harness.
    Withheld,
    /// The source cannot supply this metric at all.
    Unsupported,
}

impl<T> Vs<T> {
    /// The state label.
    pub(crate) const fn state(&self) -> SensitivityState {
        match self {
            Self::Known(_) => SensitivityState::Known,
            Self::Unknown => SensitivityState::Unknown,
            Self::Withheld => SensitivityState::Withheld,
            Self::Unsupported => SensitivityState::Unsupported,
        }
    }

    /// Normalize an optional input leaf: `None` means not observed.
    pub(crate) fn normalize(value: Option<Self>) -> Self {
        value.unwrap_or(Self::Unknown)
    }
}

impl<T: Serialize> Serialize for Vs<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct as _;
        match self {
            Self::Known(value) => {
                let mut map = serializer.serialize_struct("Vs", 2)?;
                map.serialize_field("state", "known")?;
                map.serialize_field("value", value)?;
                map.end()
            }
            state => {
                let mut map = serializer.serialize_struct("Vs", 1)?;
                map.serialize_field("state", state.state().as_str())?;
                map.end()
            }
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Vs<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw<T2> {
            state: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            value: Option<T2>,
        }
        let raw = Raw::<T>::deserialize(deserializer)?;
        match raw.state.as_str() {
            "known" => match raw.value {
                Some(value) => Ok(Self::Known(value)),
                None => Err(serde::de::Error::custom("known state requires a value")),
            },
            "unknown" | "withheld" | "unsupported" => {
                if raw.value.is_some() {
                    return Err(serde::de::Error::custom(
                        "state-only spelling carries no value",
                    ));
                }
                Ok(match raw.state.as_str() {
                    "unknown" => Self::Unknown,
                    "withheld" => Self::Withheld,
                    _ => Self::Unsupported,
                })
            }
            _ => Err(serde::de::Error::custom("unknown value-state label")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn known_zero_round_trips() {
        let value: Vs<u64> = Vs::Known(0);
        let rendered = serde_json::to_value(&value).expect("serialize");
        assert_eq!(rendered, json!({"state": "known", "value": 0}));
        let parsed: Vs<u64> = serde_json::from_value(rendered).expect("parse");
        assert_eq!(parsed, Vs::Known(0));
    }

    #[test]
    fn state_only_spellings_reject_values() {
        for state in ["unknown", "withheld", "unsupported"] {
            let payload = json!({"state": state, "value": 1});
            let parsed: Result<Vs<u64>, _> = serde_json::from_value(payload);
            assert!(parsed.is_err(), "{state} must reject a value");
        }
    }

    #[test]
    fn known_requires_a_value_and_unknown_labels_refuse() {
        let missing: Result<Vs<u64>, _> = serde_json::from_value(json!({"state": "known"}));
        assert!(missing.is_err());
        let label: Result<Vs<u64>, _> = serde_json::from_value(json!({"state": "absent"}));
        assert!(label.is_err());
    }

    #[test]
    fn optional_input_normalizes_to_unknown() {
        let none: Option<Vs<u64>> = None;
        assert_eq!(Vs::normalize(none), Vs::<u64>::Unknown);
    }
}
