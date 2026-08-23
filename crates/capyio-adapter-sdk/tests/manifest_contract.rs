use std::fs;
use std::path::PathBuf;

use capyio_adapter_sdk::{AdapterManifest, ManifestError};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn committed_schema_and_mock_manifests_match_v1_contract() {
    let schema_path = root().join("protocol/schemas/adapter-manifest.schema.json");
    let schema: serde_json::Value =
        serde_json::from_slice(&fs::read(schema_path).expect("read schema"))
            .expect("valid schema JSON");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["properties"]["schema_version"]["const"], 1);
    assert!(schema["required"].as_array().is_some_and(|required| {
        [
            "id",
            "control_protocol",
            "entrypoints",
            "capability_templates",
        ]
        .iter()
        .all(|field| required.iter().any(|item| item == field))
    }));

    for relative in [
        "adapters/mock-source/adapter.json",
        "adapters/mock-sink/adapter.json",
    ] {
        let manifest = AdapterManifest::from_json(
            &fs::read(root().join(relative)).expect("read Mock manifest"),
        )
        .expect("manifest matches Rust v1 contract");
        assert_eq!(manifest.schema_version, 1);
    }
}

#[test]
fn unsupported_manifest_version_fails_explicitly() {
    let bytes = fs::read(root().join("adapters/mock-source/adapter.json")).expect("manifest");
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON");
    value["schema_version"] = 2.into();
    assert!(matches!(
        AdapterManifest::from_json(&serde_json::to_vec(&value).expect("JSON")),
        Err(ManifestError::UnsupportedSchemaVersion(2))
    ));
}
