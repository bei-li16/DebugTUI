use super::*;
use crate::registers::Sample;
use serde_json::json;

fn sample() -> Sample {
    serde_json::from_value(json!({
        "id":"r0","state":"valid","implementation":"unknown","reason":"unknown","detail":"",
        "value":{"bits":32,"hex":"0x12345678"},"owner":"core:core0",
        "context":{"session":1,"generation":2,"core":"core0","frame":0},
        "timestamp_ms":23,"source":"gdb:r0","view":"selected_frame"
    }))
    .unwrap()
}
fn source(sample: &Sample, endpoint: Option<&str>, time: u64) -> Provenance {
    let mut provenance = Provenance::declared(&Reader::Gdb { name: "r0".into() });
    provenance.access = Some(Access {
        completed_ms: None,
        timer: None,
        pmu: None,
        route: Route::GdbRegister {
            endpoint: endpoint.map(str::to_owned),
            configured_endpoint: "configured:3333".into(),
            name: "r0".into(),
            index: 17,
        },
        phase: Phase::Responded,
        command: "-data-list-register-values r 17".into(),
        context: sample.context.clone(),
        timestamp_ms: time,
    });
    provenance
}
#[test]
fn legacy_register_source_labels_never_infer_an_executed_route_or_connected_endpoint() {
    let mut sample = sample();
    assert!(sample.provenance.is_none());
    assert!(sample.value_provenance().is_none());
    let old = serde_json::to_value(&sample).unwrap();
    assert!(old.get("provenance").is_none() && old.get("last_value_provenance").is_none());
    sample.provenance = Some(source(&sample, None, 25));
    let encoded = serde_json::to_value(&sample).unwrap();
    assert!(encoded["provenance"]["access"]["route"]["endpoint"].is_null());
    assert_eq!(
        encoded["provenance"]["access"]["route"]["configured_endpoint"],
        "configured:3333"
    );
    assert_eq!(encoded["provenance"]["access"]["route"]["index"], 17);
    let decoded: Sample = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
}
#[test]
fn retained_register_values_keep_the_last_valid_origin_across_failures_and_legacy_unknowns() {
    let mut original = sample();
    original.provenance = Some(source(&original, Some("selected:3333"), 23));
    let expected = serde_json::to_value(original.value_provenance()).unwrap();
    let mut failed = sample();
    failed.state = crate::registers::State::Error;
    failed.provenance = Some(source(&failed, Some("different:9999"), 29));
    failed.inherit_value_origin(&original);
    assert_eq!(
        serde_json::to_value(failed.value_provenance()).unwrap(),
        expected
    );
    assert_ne!(serde_json::to_value(&failed.provenance).unwrap(), expected);
    let mut again = failed.clone();
    again.provenance = Some(source(&again, None, 35));
    again.inherit_value_origin(&failed);
    assert_eq!(
        serde_json::to_value(again.value_provenance()).unwrap(),
        expected
    );
    let legacy = sample();
    again.inherit_value_origin(&legacy);
    assert!(again.value_provenance().is_none());
    let encoded = serde_json::to_value(&again).unwrap();
    assert_eq!(encoded["last_value_provenance"]["status"], "unknown");
    let decoded: Sample = serde_json::from_value(encoded).unwrap();
    assert!(
        decoded.value_provenance().is_none(),
        "an unknown old origin must not become the latest attempt"
    );
}
