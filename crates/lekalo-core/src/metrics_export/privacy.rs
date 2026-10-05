//! #119 remains the only evaluator, subject projector and leak scanner.
use super::{canon, digest, Error, Result};
use crate::privacy::{
    context::TrustedContext, evaluate, export::DestinationSpec, input, refs, types, vocab,
};
use serde_json::{json, Value};

pub(crate) fn decision_template(
    payload: &str,
    projection: &Value,
    labels: Vec<vocab::DataSensitivity>,
    destination: DestinationSpec,
    supplied: Option<&Value>,
) -> Result<Value> {
    let ctx = TrustedContext::embedded().map_err(|_| Error::Privacy)?;
    let projection_digest = digest(canon(projection).as_bytes());
    let resolved = destination.resolve(
        &format!("repo-sha256:{}", &projection_digest[7..]),
        &format!(
            "repo-sha256:{}",
            &digest(format!("consumer\n{projection_digest}").as_bytes())[7..]
        ),
    );
    let classification = |artifact: &str| {
        types::ClassificationDecisionRef::new(
            refs::CLASSIFICATION_CONTRACT_ID.to_owned(),
            refs::DECISION_FAMILY_VERSION.to_owned(),
            format!("sha256:{}", refs::CLASSIFICATION_CONTRACT_RAW_SHA256),
            format!(
                "classification-sha256:{}",
                &digest(format!("classification\n{artifact}").as_bytes())[7..]
            ),
            artifact.to_owned(),
        )
    };
    let artifact_digest = digest(payload.as_bytes());
    // Only an explicit issuer request can plan removal of the internal floor.
    // Other sensitivity classes are never lowered by this v1 adapter. The
    // untouched evaluator still requires fresh verified purpose-bound approval;
    // this metadata plan does not authorize anything by itself.
    let declassify = labels.contains(&vocab::DataSensitivity::Internal)
        && labels.iter().all(|v| {
            matches!(
                v,
                vocab::DataSensitivity::Public | vocab::DataSensitivity::Internal
            )
        })
        && supplied.is_some_and(|v| {
            v["dataSensitivity"] == json!(["public"])
                && v.pointer("/derivedArtifact/declassificationDecision/removedSensitivities")
                    == Some(&json!(["internal"]))
        });
    let provenance = input::Provenance::new(
        vocab::ProvenanceOrigin::Derived,
        resolved.source.repository_role(),
        resolved.provenance_ref,
        false,
        true,
        classification(&artifact_digest),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let base = input::ExportDecisionInput::with_frozen_refs(
        "aggregate.artifact".to_owned(),
        format!("artifact-sha256:{}", &artifact_digest[7..]),
        resolved.operation,
        resolved.source,
        resolved.destination,
        resolved.audience,
        if declassify {
            vec![vocab::DataSensitivity::Public]
        } else {
            labels.clone()
        },
        vocab::ExportDisposition::PublicAggregate,
        provenance,
        // This is the content-addressed logical resource of the new aggregate,
        // never a recorder/source/native path. #119 requires a known safe
        // resource path at publish boundaries, even for public aggregates.
        types::ResourcePath::known(format!("aggregates/{}/payload.json", &artifact_digest[7..])),
        None,
        input::ConflictResolution::new(vocab::ConflictState::None, None),
        Vec::new(),
        None,
    );
    let mut wire = serde_json::to_value(base).map_err(|_| Error::Invalid)?;
    wire["derivedArtifact"] = json!({
        "sourceArtifacts":[{"sourceRef":format!("source-sha256:{}",&projection_digest[7..]),"artifactKind":"aggregate.decision","authorityRef":wire["authorityRef"],"policyRef":wire["policyRef"],"classificationRef":classification(&projection_digest),"dataSensitivity":labels,"exportDisposition":"local-private"}],
        "appliedTransforms":[{"transformId":"aggregate-no-source-rows","version":refs::DECISION_FAMILY_VERSION,"evidenceDigest":projection_digest}],
        "declassificationDecision":null,
        "aggregationDecision":{"policyRef":wire["policyRef"],"decisionRef":null,"version":refs::DECISION_FAMILY_VERSION,"outcome":"approved","removesSourceRows":true,"removesSourceIdentities":true},
        "containsSourceRows":false,"containsSourceIdentities":false,"reevaluated":true
    });
    if declassify {
        wire["derivedArtifact"]["declassificationDecision"] = json!({"policyRef":wire["policyRef"],"decisionRef":null,"version":refs::DECISION_FAMILY_VERSION,"outcome":"approved","removedSensitivities":["internal"]});
    }
    // A template is explicitly non-authorizing. Its null evidence position
    // is never accepted as ExportDecisionInput or passed as an allow result.
    let _ = ctx;
    Ok(wire)
}

pub(crate) fn subject(wire: &Value) -> Result<String> {
    let ctx = TrustedContext::embedded().map_err(|_| Error::Privacy)?;
    Ok(evaluate::authorization_subject_digest(
        wire,
        ctx.authorization_subject_profile(),
    ))
}

pub(crate) fn authorize(template: &Value, supplied: Option<&Value>) -> Result<Value> {
    let Some(wire) = supplied else {
        return Ok(
            json!({"decision":"deny","reasonCodes":["derived.aggregation-decision-required"]}),
        );
    };
    let ctx = TrustedContext::embedded().map_err(|_| Error::Privacy)?;
    if evaluate::validate_decision_input(wire, ctx).is_some()
        || subject(wire)? != subject(template)?
    {
        return Err(Error::Privacy);
    }
    let evaluated = evaluate::evaluate_decision(wire, ctx);
    serde_json::to_value(evaluated.output).map_err(|_| Error::Privacy)
}

pub(crate) fn scan(payload: &str) -> Result<(Value, Value)> {
    // #119 scans unstructured text. Contract IDs contain slashes and exact
    // aggregate numerals can resemble phone numbers. Recognize only the
    // public typed grammar before applying that unchanged scanner to strings.
    // This representation is explicit on the wire; the release bytes are not
    // redacted or silently changed to make a scan pass.
    let value = super::storage::parse(payload.as_bytes())?;
    let mut strings = Vec::new();
    scan_strings(&value, &mut strings);
    let text = strings.join("\n");
    let redacted = crate::privacy::redact::redact(&crate::privacy::redact::RedactionRequest {
        payload: &text,
        transforms: &[],
        labels: &[],
        subject: crate::privacy::redact::RedactionSubject {
            repository: None,
            protected_terms: &[],
        },
    });
    // Projection errors cannot be repaired by changing a metric to a marker.
    if redacted.payload() != text
        || !redacted.findings().is_empty()
        || !redacted.residuals().is_empty()
    {
        return Err(Error::Leak);
    }
    Ok((
        json!({"appliedTransforms":[],"findings":redacted.findings()}),
        json!({"scannerVersion":"119/1","representation":"typed-public-string-values/1","residuals":redacted.residuals()}),
    ))
}

fn scan_strings<'a>(value: &'a Value, strings: &mut Vec<&'a str>) {
    match value {
        Value::Array(values) => values.iter().for_each(|v| scan_strings(v, strings)),
        Value::Object(values) => values.values().for_each(|v| scan_strings(v, strings)),
        Value::String(s) => {
            let fixed = [
                "lekalo/public-metrics/v0.6.4",
                "dev.lekalo.public-metrics@0.6.4",
                "lekalo/metrics-export-manifest/v0.6.4",
                "dev.lekalo.metrics-export-manifest@0.6.4",
                "dev.lekalo.metrics-aggregation-definition@0.6.4",
                "framework-lift-paired-trials/1",
                "sorted-utf8-compact-json-lf/1",
                "119/1",
                "typed-public-string-values/1",
                "live-history-and-current-privacy/1",
                refs::POLICY_ID,
                refs::AUTHORITY_CONTRACT_ID,
                refs::CLASSIFICATION_CONTRACT_ID,
                refs::AUTHORIZING_EVIDENCE_CONTRACT_ID,
                refs::AUTHORIZATION_SUBJECT_PROFILE_ID,
                refs::INPUT_SCHEMA_ID,
                refs::OUTPUT_SCHEMA_ID,
            ]
            .contains(&s.as_str());
            let numeral = !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || b == b'.' || b == b'-')
                && s.bytes().any(|b| b.is_ascii_digit());
            let hash = s.strip_prefix("sha256:").is_some_and(|v| {
                v.len() == 64
                    && v.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            });
            if !fixed && !numeral && !hash {
                strings.push(s);
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grammar_is_not_a_scanner_exception_for_private_text() {
        for text in [
            "https://private.invalid/repo",
            "/home/private/code.rs",
            "person@example.com",
            "tenant-private-id",
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "C:\\private\\code.rs",
        ] {
            assert!(matches!(
                scan(&canon(&json!({"value":text}))),
                Err(Error::Leak)
            ));
        }
        assert!(scan(&canon(
            &json!({"schema_version":"lekalo/public-metrics/v0.6.4","count":"123456789012"})
        ))
        .is_ok());
    }
}
