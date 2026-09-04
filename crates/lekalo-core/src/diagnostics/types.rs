//! Closed diagnostic classification and data vocabulary (issue #11).

use std::collections::BTreeMap;

use serde::Serialize;

/// The closed data payload: registry-declared keys with typed values.
pub type DataObject = BTreeMap<String, DataValue>;

/// Defensive caps fixed for diagnostic v1. Over-limit producer sets are an
/// invariant failure; over-limit untrusted provider input collapses into one
/// bounded `adapter.diagnostic-invalid` diagnostic.
pub mod limits {
    /// Maximum diagnostics in one result set.
    pub const DIAGNOSTICS_PER_RESULT: usize = 256;
    /// Maximum related locations per diagnostic.
    pub const RELATED_PER_ITEM: usize = 32;
    /// Maximum causes per diagnostic.
    pub const CAUSES_PER_ITEM: usize = 8;
    /// Maximum fixes per diagnostic.
    pub const FIXES_PER_ITEM: usize = 16;

    /// Maximum provider namespaces per diagnostic.
    pub const PROVIDER_NAMESPACES_PER_ITEM: usize = 8;
    /// Maximum data fields per diagnostic.
    pub const DATA_FIELDS_PER_ITEM: usize = 16;
    /// Maximum members per list data value.
    pub const LIST_MEMBERS: usize = 64;
    /// Maximum bytes of one provider original code.
    pub const ORIGINAL_CODE_BYTES: usize = 128;
    /// Maximum bytes of one bounded token.
    pub const TOKEN_BYTES: usize = 256;
}

/// The closed severity scale.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// Parse the wire spelling (registry and tests only).
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// The closed rule classification, registered per rule and never inferred
/// from the code prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Model,
    Semantic,
    Compatibility,
    Adapter,
    Infrastructure,
    Security,
}

impl Category {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Semantic => "semantic",
            Self::Compatibility => "compatibility",
            Self::Adapter => "adapter",
            Self::Infrastructure => "infrastructure",
            Self::Security => "security",
        }
    }

    /// Parse the wire spelling (registry and tests only).
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "model" => Some(Self::Model),
            "semantic" => Some(Self::Semantic),
            "compatibility" => Some(Self::Compatibility),
            "adapter" => Some(Self::Adapter),
            "infrastructure" => Some(Self::Infrastructure),
            "security" => Some(Self::Security),
            _ => None,
        }
    }
}

/// The closed fix applicability scale. Fixes are inert advice: `safe` means
/// eligible to offer, never permission to mutate; execution belongs to later
/// issues (#73/#96).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FixApplicability {
    Safe,
    Unsafe,
    Breaking,
}

impl FixApplicability {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Unsafe => "unsafe",
            Self::Breaking => "breaking",
        }
    }

    /// The deterministic applicability rank (safe first).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn rank(self) -> u8 {
        match self {
            Self::Safe => 0,
            Self::Unsafe => 1,
            Self::Breaking => 2,
        }
    }
}

/// The registered location guarantee of a rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum LocationRequirement {
    /// No location is guaranteed; any present location is still validated.
    None,
    /// A logical path is guaranteed.
    Path,
    /// A source range is guaranteed (path optional in v1 producer seams).
    Span,
}

impl LocationRequirement {
    /// Parse the wire spelling (registry and tests only).
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "path" => Some(Self::Path),
            "span" => Some(Self::Span),
            _ => None,
        }
    }
}

/// The closed data field types a registry entry may declare.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum DataFieldType {
    /// Bounded UTF-8 token (at most [`limits::TOKEN_BYTES`] bytes).
    Token,
    /// Non-negative integer count.
    Count,
    /// Boolean flag.
    Flag,
    /// Bounded homogeneous list of scalars.
    List,
    /// Bounded list of flat scalar-only records.
    Records,
    /// One flat scalar-only record.
    Record,
}

impl DataFieldType {
    /// Parse the wire spelling (registry and tests only).
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "token" => Some(Self::Token),
            "count" => Some(Self::Count),
            "flag" => Some(Self::Flag),
            "list" => Some(Self::List),
            "records" => Some(Self::Records),
            "record" => Some(Self::Record),
            _ => None,
        }
    }
}

/// One scalar data value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum Scalar {
    /// Bounded token.
    Token(String),
    /// Non-negative integer.
    Count(u64),
    /// Boolean.
    Flag(bool),
}

/// One value inside a flat record: a scalar or exactly one more record of
/// scalars (the accepted #7 span shape nests exactly two levels).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum NestedValue {
    /// A scalar.
    Scalar(Scalar),
    /// A record of scalars.
    Record(BTreeMap<String, Scalar>),
}

/// One typed data value; arbitrary `serde_json::Value` is never accepted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DataValue {
    /// Bounded token.
    Token(String),
    /// Non-negative integer.
    Count(u64),
    /// Boolean.
    Flag(bool),
    /// Bounded homogeneous scalar list.
    List(Vec<Scalar>),
    /// Bounded list of flat scalar-only records.
    Records(Vec<BTreeMap<String, NestedValue>>),
    /// One flat scalar-or-nested record.
    Record(BTreeMap<String, NestedValue>),
}

impl DataValue {
    /// The declared field type of this value, for registry validation.
    pub fn field_type(&self) -> DataFieldType {
        match self {
            Self::Token(_) => DataFieldType::Token,
            Self::Count(_) => DataFieldType::Count,
            Self::Flag(_) => DataFieldType::Flag,
            Self::List(_) => DataFieldType::List,
            Self::Records(_) => DataFieldType::Records,
            Self::Record(_) => DataFieldType::Record,
        }
    }
}

/// Sort scalars by unsigned UTF-8 bytes, then by value.
pub(crate) fn scalar_sort_key(scalar: &Scalar) -> (Vec<u8>, String) {
    match scalar {
        Scalar::Token(text) => (vec![0], text.clone()),
        Scalar::Count(count) => (vec![1], count.to_string()),
        Scalar::Flag(flag) => (vec![if *flag { 2 } else { 1 }], String::new()),
    }
}

/// Normalize list members by unsigned UTF-8 bytes.
pub(crate) fn normalize_scalars(values: &mut [Scalar]) {
    values.sort_by_key(scalar_sort_key);
}

/// Collapse control characters to spaces inside an echoed token.
pub(crate) fn clean_token(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// Bound a token to [`limits::TOKEN_BYTES`] bytes on a char boundary after
/// cleaning; hostile input never scales the diagnostic envelope.
pub(crate) fn bound_token(text: &str) -> String {
    let cleaned = clean_token(text);
    if cleaned.len() <= limits::TOKEN_BYTES {
        return cleaned;
    }
    let mut bound = limits::TOKEN_BYTES;
    while !cleaned.is_char_boundary(bound) {
        bound -= 1;
    }
    cleaned[..bound].to_owned()
}

/// One `DataValue::Token` that honors the bounded-token invariant: `text`
/// is cleaned and capped to [`limits::TOKEN_BYTES`] bytes before it may
/// enter a wire item, whatever its (possibly attacker-controlled) origin.
pub(crate) fn token_value(text: &str) -> DataValue {
    DataValue::Token(bound_token(text))
}

fn scalar_within_bound(value: &serde_json::Value, strict: bool) -> Option<Scalar> {
    match value {
        serde_json::Value::String(text) => {
            if text.chars().any(|character| character.is_control())
                || text.len() > limits::TOKEN_BYTES
            {
                return (!strict).then(|| Scalar::Token(bound_token(text)));
            }
            Some(Scalar::Token(text.clone()))
        }
        serde_json::Value::Number(number) => Some(Scalar::Count(number.as_u64()?)),
        serde_json::Value::Bool(flag) => Some(Scalar::Flag(*flag)),
        _ => None,
    }
}

fn nested_value(value: &serde_json::Value, strict: bool) -> Option<NestedValue> {
    if let serde_json::Value::Object(fields) = value {
        if fields.len() > limits::DATA_FIELDS_PER_ITEM {
            return None;
        }
        let mut record = BTreeMap::new();
        for (name, field) in fields {
            if name.len() > 32 {
                return None;
            }
            record.insert(name.clone(), scalar_within_bound(field, strict)?);
        }
        return Some(NestedValue::Record(record));
    }
    scalar_within_bound(value, strict).map(NestedValue::Scalar)
}

fn data_value(value: &serde_json::Value, strict: bool) -> Option<DataValue> {
    match value {
        serde_json::Value::Object(fields) => {
            if fields.len() > limits::DATA_FIELDS_PER_ITEM {
                return None;
            }
            let mut record = BTreeMap::new();
            for (name, field) in fields {
                if name.len() > 32 {
                    return None;
                }
                record.insert(name.clone(), nested_value(field, strict)?);
            }
            Some(DataValue::Record(record))
        }
        serde_json::Value::Array(members) => {
            if members.is_empty() || members.len() > limits::LIST_MEMBERS {
                return None;
            }
            if members.iter().all(|member| member.is_object()) {
                let mut records = Vec::with_capacity(members.len());
                for member in members {
                    let serde_json::Value::Object(fields) = member else {
                        return None;
                    };
                    if fields.len() > limits::DATA_FIELDS_PER_ITEM {
                        return None;
                    }
                    let mut record = BTreeMap::new();
                    for (name, field) in fields {
                        if name.len() > 32 {
                            return None;
                        }
                        record.insert(name.clone(), nested_value(field, strict)?);
                    }
                    records.push(record);
                }
                return Some(DataValue::Records(records));
            }
            let mut list = Vec::with_capacity(members.len());
            for member in members {
                list.push(scalar_within_bound(member, strict)?);
            }
            Some(DataValue::List(list))
        }
        other => scalar_within_bound(other, strict).map(|scalar| match scalar {
            Scalar::Token(text) => DataValue::Token(text),
            Scalar::Count(count) => DataValue::Count(count),
            Scalar::Flag(flag) => DataValue::Flag(flag),
        }),
    }
}

/// Convert an untrusted JSON object into a typed data object, bounding every
/// token. Returns `None` for wrong shapes, negative or fractional numbers,
/// and over-limit keys or lists.
pub(crate) fn data_object_bounded(value: &serde_json::Value) -> Option<DataObject> {
    data_object(value, false)
}

/// Convert an untrusted JSON object into a typed data object, refusing
/// over-limit tokens instead of bounding them (provider seam).
pub(crate) fn data_object_strict(value: &serde_json::Value) -> Option<DataObject> {
    data_object(value, true)
}

fn data_object(value: &serde_json::Value, strict: bool) -> Option<DataObject> {
    let serde_json::Value::Object(fields) = value else {
        return None;
    };
    if fields.len() > limits::DATA_FIELDS_PER_ITEM {
        return None;
    }
    let mut data = DataObject::new();
    for (name, field) in fields {
        if name.len() > 32 {
            return None;
        }
        data.insert(name.clone(), data_value(field, strict)?);
    }
    Some(data)
}
