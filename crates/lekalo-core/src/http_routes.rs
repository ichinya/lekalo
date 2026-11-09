//! Source-bound HTTP registration facts, independent of canonical Endpoint intent.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteSource {
    pub path: String,
    pub source_digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpRouteEvidence {
    pub provider: String,
    pub rules_revision: String,
    pub policy_digest: String,
    pub method: String,
    pub path: String,
    pub application_native: String,
    pub handler_native: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub operation_id: Option<String>,
    pub registration_path: String,
    pub registration_line: u64,
    pub sources: Vec<RouteSource>,
    pub confidence: String,
}

pub const METHODS: [&str; 8] = [
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE",
];

pub fn printable(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.chars().count() <= limit && !value.chars().any(char::is_control)
}

impl HttpRouteEvidence {
    pub fn valid(&self) -> bool {
        let path = |p: &str| {
            p.len() <= 512
                && crate::target_protocol::scopes::is_logical_path(p)
                && crate::project_fs::path_violation(p).is_none()
        };
        self.provider.len() <= 64
            && self
                .provider
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_lowercase)
            && self
                .provider
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && printable(&self.rules_revision, 64)
            && crate::observed::version::is_sha256(&self.policy_digest)
            && METHODS.contains(&self.method.as_str())
            && self.path.starts_with('/')
            && printable(&self.path, 2048)
            && printable(&self.application_native, 256)
            && printable(&self.handler_native, 256)
            && self
                .operation_id
                .as_ref()
                .is_none_or(|id| printable(id, 256))
            && path(&self.registration_path)
            && (1..=1_000_000).contains(&self.registration_line)
            && !self.sources.is_empty()
            && self.sources.len() <= 64
            && self.sources.iter().all(|source| {
                path(&source.path) && crate::observed::version::is_sha256(&source.source_digest)
            })
            && self
                .sources
                .windows(2)
                .all(|pair| pair[0].path < pair[1].path)
            && self
                .sources
                .iter()
                .any(|source| source.path == self.registration_path)
            && matches!(
                self.confidence.as_str(),
                "exact" | "high" | "medium" | "low" | "unknown"
            )
    }

    pub fn stable_id(&self, module: &str) -> String {
        let key = serde_json::to_vec(&(
            &self.provider,
            &self.application_native,
            &self.handler_native,
            &self.method,
            &self.path,
            &self.operation_id,
        ))
        .unwrap_or_default();
        format!("{module}.http:{}", crate::digest::sha256_hex(&key))
    }
}

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Bind a producer's claimed provider/policy to the user-owned launch argv.
pub fn authorized_policy(args: &[String], provider: &str, expected: &str) -> bool {
    let matches: Vec<usize> = args
        .iter()
        .enumerate()
        .filter_map(|(i, arg)| (arg == "--lekalo-framework-policy-json").then_some(i))
        .collect();
    if matches.len() != 1 {
        return false;
    }
    let Some(raw) = args
        .get(matches[0] + 1)
        .filter(|s| !s.is_empty() && s.len() <= 8192)
    else {
        return false;
    };
    let Ok(value) = crate::classification::parse_checked(raw) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.len() != 3 || value["schema"] != "lekalo/framework-policy" || value["version"] != 1 {
        return false;
    }
    let Some(providers) = value["providers"].as_array().filter(|p| p.len() <= 16) else {
        return false;
    };
    let mut names = std::collections::BTreeSet::new();
    let mut enabled = false;
    for row in providers {
        let Some(row) = row.as_object().filter(|r| r.len() == 2) else {
            return false;
        };
        let Some(id) = row.get("id").and_then(serde_json::Value::as_str) else {
            return false;
        };
        let Some(state) = row.get("state").and_then(serde_json::Value::as_str) else {
            return false;
        };
        if id.len() > 64
            || !id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !names.insert(id)
            || !matches!(state, "enabled" | "disabled")
        {
            return false;
        }
        enabled |= id == provider && state == "enabled";
    }
    let Ok(bytes) = serde_json::to_vec(&serde_json::json!({"providers": providers})) else {
        return false;
    };
    enabled && expected == format!("sha256:{}", crate::digest::sha256_hex(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn route() -> Value {
        json!({"provider":"fastapi","rules_revision":"fastapi-static-v1","policy_digest":format!("sha256:{}","a".repeat(64)),
            "method":"GET","path":"/items","application_native":"app","handler_native":"handler","registration_path":"src/main.py","registration_line":3,
            "sources":[{"path":"src/handler.py","source_digest":format!("sha256:{}","b".repeat(64))},{"path":"src/main.py","source_digest":format!("sha256:{}","c".repeat(64))}],"confidence":"high"})
    }
    #[test]
    fn route_metadata_is_closed_bounded_and_source_bound() {
        let valid = route();
        assert!(serde_json::from_value::<HttpRouteEvidence>(valid.clone())
            .unwrap()
            .valid());
        let mut unicode = valid.clone();
        let path = format!("src/{}.py", "界".repeat(180));
        unicode["registration_path"] = json!(path);
        unicode["sources"] =
            json!([{"path": path,"source_digest":format!("sha256:{}","b".repeat(64))}]);
        assert!(!serde_json::from_value::<HttpRouteEvidence>(unicode)
            .unwrap()
            .valid());
        for (key, value) in [("operation_id", Value::Null), ("unknown", json!(true))] {
            let mut v = valid.clone();
            v[key] = value;
            assert!(serde_json::from_value::<HttpRouteEvidence>(v).is_err());
        }
        for (key, value) in [
            ("method", json!("CONNECT")),
            ("path", json!("relative")),
            ("registration_path", json!("../outside.py")),
            ("registration_line", json!(0)),
            ("provider", json!("FastAPI")),
        ] {
            let mut v = valid.clone();
            v[key] = value;
            assert!(!serde_json::from_value::<HttpRouteEvidence>(v)
                .unwrap()
                .valid());
        }
        for sources in [
            json!([]),
            json!([valid["sources"][0].clone()]),
            json!([valid["sources"][1].clone(), valid["sources"][0].clone()]),
            json!([valid["sources"][0].clone(), valid["sources"][0].clone()]),
        ] {
            let mut v = valid.clone();
            v["sources"] = sources;
            assert!(!serde_json::from_value::<HttpRouteEvidence>(v)
                .unwrap()
                .valid());
        }
        let mut op = serde_json::from_value::<HttpRouteEvidence>(valid).unwrap();
        let id = op.stable_id("shop");
        op.operation_id = Some("list_items".into());
        assert_ne!(op.stable_id("shop"), id);
    }
    #[test]
    fn producer_policy_claim_requires_exact_user_owned_enabled_configuration() {
        let policy = json!({"schema":"lekalo/framework-policy","version":1,"providers":[{"id":"fastapi","state":"enabled"}]});
        let digest = format!(
            "sha256:{}",
            crate::digest::sha256_hex(
                serde_json::to_string(&json!({"providers":policy["providers"]}))
                    .unwrap()
                    .as_bytes()
            )
        );
        let args = vec!["--lekalo-framework-policy-json".into(), policy.to_string()];
        assert!(authorized_policy(&args, "fastapi", &digest));
        assert!(!authorized_policy(&[], "fastapi", &digest));
        assert!(!authorized_policy(&args, "other", &digest));
        assert!(!authorized_policy(&args, "fastapi", "sha256:wrong"));
        let mut duplicate = args.clone();
        duplicate.extend(args.clone());
        assert!(!authorized_policy(&duplicate, "fastapi", &digest));
        for raw in [
            policy.to_string().replace("enabled", "disabled"),
            policy.to_string().replace("\"version\":1", "\"version\":2"),
            r#"{"providers":[],"providers":[],"schema":"lekalo/framework-policy","version":1}"#.to_owned(),
            json!({"schema":"lekalo/framework-policy","version":1,"providers":[{"id":"fastapi","state":"enabled","extra":1}]}).to_string(),
        ] {
            assert!(!authorized_policy(&[args[0].clone(), raw], "fastapi", &digest));
        }
    }
    #[test]
    fn http_successor_rejects_route_facts_on_predecessors_and_non_handlers() {
        let signature = format!("sha256:{}", "d".repeat(64));
        let native = json!({"native":"handler","qualified_name":"shop.items","family":"function","path":"src/handler.py","line":1,"end_line":5,"slot":"static","declaration_only":false,"signature":signature});
        let response = json!({"protocol":"lekalo.target/v1","protocol_version":"1.2.0","operation":"scan","status":"ok","request_id":format!("req-{}","0".repeat(64)),
            "evidence":{"adapter":{"id":"python","version":"0.1.0","digest":format!("sha256:{}","0".repeat(64))},"scan_profile_digest":format!("sha256:{}","1".repeat(64)),"native_uncertainty":[]},
            "result":{"entries":[{"path":"src/handler.py","kind":"endpoint","detail":json!({"s":"shop.items","n":"handler"}).to_string(),"evidence":{"signature":signature,"source_digest":format!("sha256:{}","b".repeat(64)),"native_symbols":[native],"http_routes":[route()]}}],"truncated":false}});
        let decode = |v: &Value| {
            crate::target_protocol::wire::decode_response(&serde_json::to_vec(v).unwrap())
        };
        assert!(decode(&response).is_ok());
        for version in ["0.9.1", "1.1.0"] {
            let mut v = response.clone();
            v["protocol_version"] = json!(version);
            assert!(decode(&v).is_err());
        }
        let mut v = response.clone();
        v["result"]["entries"][0]["kind"] = json!("entity");
        assert!(decode(&v).is_err());
        let mut v = response.clone();
        v["result"]["entries"][0]["evidence"]["http_routes"][0]["handler_native"] =
            json!("some-other-function");
        assert!(decode(&v).is_err());
        let mut v = response.clone();
        v["result"]["entries"][0]["evidence"]["http_routes"][0]["operation_id"] = Value::Null;
        assert!(decode(&v).is_err());
        let mut v = response.clone();
        v["evidence"]["native_uncertainty"] = json!([{"path":"src/main.py","kind":"framework-route","detail":"dynamic-prefix","line":3,"source_digest":format!("sha256:{}","c".repeat(64))}]);
        assert!(decode(&v).is_ok());
    }
}
