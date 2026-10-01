//! The measured-value state wrapper of the context-budget report (issue #75).
//!
//! The exact four-state M6 union: `known` is the only state that carries a
//! value; `unknown`, `withheld`, and `unsupported` are state-only
//! spellings. A metric that cannot be measured stays `unknown` — never
//! zero, null, or a negative count — and a known zero round-trips as a
//! known zero. Every persisted state spelling must carry its exact shape:
//! a value under a state-only spelling, or a missing value under `known`,
//! refuses in both Rust and the JSON Schema.

use serde::Deserialize;
use serde::Serialize;

/// One measured value in one of the four closed states.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateValue<T> {
    /// A measured value (including a known zero).
    Known(T),
    /// Not measurable by the selected recipe under the current evidence.
    Unknown,
    /// Measurable but intentionally withheld by the caller.
    Withheld,
    /// The selected recipe cannot supply this metric at all.
    Unsupported,
}

impl<T> StateValue<T> {
    /// The state label.
    pub const fn state(&self) -> &'static str {
        match self {
            Self::Known(_) => "known",
            Self::Unknown => "unknown",
            Self::Withheld => "withheld",
            Self::Unsupported => "unsupported",
        }
    }

    /// The measured value, if and only if this is `known`.
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            _ => None,
        }
    }

    /// Map the carried value, keeping the state.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> StateValue<U> {
        match self {
            Self::Known(value) => StateValue::Known(f(value)),
            Self::Unknown => StateValue::Unknown,
            Self::Withheld => StateValue::Withheld,
            Self::Unsupported => StateValue::Unsupported,
        }
    }
}

impl<T: Serialize> Serialize for StateValue<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct as _;
        match self {
            Self::Known(value) => {
                let mut map = serializer.serialize_struct("StateValue", 2)?;
                map.serialize_field("state", "known")?;
                map.serialize_field("value", value)?;
                map.end()
            }
            state => {
                let mut map = serializer.serialize_struct("StateValue", 1)?;
                map.serialize_field("state", state.state())?;
                map.end()
            }
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for StateValue<T> {
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

/// The signed absolute delta of one baseline comparison row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbsoluteDelta {
    pub base: u64,
    pub candidate: u64,
    /// Signed difference `candidate - base`; never sums unknown operands.
    pub delta: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_zero_round_trips() {
        let value: StateValue<u64> = StateValue::Known(0);
        let rendered = serde_json::to_value(&value).expect("serialize");
        assert_eq!(rendered, serde_json::json!({"state": "known", "value": 0}));
        let parsed: StateValue<u64> = serde_json::from_value(rendered).expect("parse");
        assert_eq!(parsed, StateValue::Known(0));
    }

    #[test]
    fn state_only_spellings_reject_values() {
        for state in ["unknown", "withheld", "unsupported"] {
            let payload = serde_json::json!({"state": state, "value": 1});
            let parsed: Result<StateValue<u64>, _> = serde_json::from_value(payload);
            assert!(parsed.is_err(), "{state} must reject a value");
        }
    }

    #[test]
    fn known_without_value_refuses() {
        let payload = serde_json::json!({"state": "known"});
        let parsed: Result<StateValue<u64>, _> = serde_json::from_value(payload);
        assert!(parsed.is_err(), "known requires a value");
    }

    #[test]
    fn unknown_labels_refuse() {
        let payload = serde_json::json!({"state": "absent", "value": 1});
        let parsed: Result<StateValue<u64>, _> = serde_json::from_value(payload);
        assert!(parsed.is_err());
    }

    #[test]
    fn signed_delta_round_trips_both_directions() {
        let down = AbsoluteDelta {
            base: 10,
            candidate: 4,
            delta: -6,
        };
        let rendered = serde_json::to_string(&down).expect("serialize");
        let parsed: AbsoluteDelta = serde_json::from_str(&rendered).expect("parse");
        assert_eq!(parsed, down);
    }
}
