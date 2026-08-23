use std::fs;
use std::path::PathBuf;

use capyio_adapter_sdk::{
    ADAPTER_CONTROL_PROTOCOL_MAJOR, ADAPTER_MANIFEST_SCHEMA_VERSION, AdapterManifest,
    DeploymentMode, ManifestError,
};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest_for(mode: &str, platforms: Value, mode_bindings: Value) -> Value {
    json!({
        "schema_version": 2,
        "id": "dev.capyio.test-adapter",
        "name": "CapyIO Test Adapter",
        "version": "0.1.0",
        "control_protocol": { "major": 1, "minor": 0 },
        "kind": "mock",
        "deployment_modes": [mode],
        "platforms": platforms,
        "mode_bindings": mode_bindings,
        "permissions": [],
        "capability_templates": [{
            "name": "Test Capability",
            "class": "dev.capyio.test.capability",
            "ports": [{
                "name": "Test Source",
                "direction": "source",
                "profile": "capyio.test.samples",
                "profile_major": 1
            }]
        }],
        "integration_mode": "foundation_test",
        "license": {
            "spdx": "Apache-2.0",
            "notice": "Original CapyIO test-only fixture."
        },
        "upstream": null
    })
}

fn parse(value: &Value) -> Result<AdapterManifest, ManifestError> {
    AdapterManifest::from_json(&serde_json::to_vec(value).expect("serialize manifest"))
}

#[test]
fn committed_schema_and_mock_manifests_reject_v2_schema_drift() {
    let schema_path = root().join("protocol/schemas/adapter-manifest.schema.json");
    let schema: Value = serde_json::from_slice(&fs::read(schema_path).expect("read schema"))
        .expect("valid schema JSON");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        ADAPTER_MANIFEST_SCHEMA_VERSION
    );
    assert_eq!(ADAPTER_MANIFEST_SCHEMA_VERSION, 2);
    assert_eq!(
        schema["$id"],
        "https://capyio.dev/schemas/adapter-manifest-v2.json"
    );
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["required"].as_array().is_some_and(|required| {
        [
            "id",
            "control_protocol",
            "deployment_modes",
            "platforms",
            "mode_bindings",
            "capability_templates",
        ]
        .iter()
        .all(|field| required.iter().any(|item| item == field))
    }));
    assert_eq!(
        schema["$defs"]["modeBindings"]["additionalProperties"],
        false
    );
    assert_eq!(schema["allOf"].as_array().map(Vec::len), Some(4));
    assert_eq!(
        schema["properties"]["control_protocol"]["properties"]["major"]["const"],
        ADAPTER_CONTROL_PROTOCOL_MAJOR
    );

    let schema_modes = schema["properties"]["deployment_modes"]["items"]["enum"]
        .as_array()
        .expect("deployment mode enum");
    for (mode, encoded) in [
        (DeploymentMode::InProcess, "in_process"),
        (DeploymentMode::Sidecar, "sidecar"),
        (DeploymentMode::ExternalService, "external_service"),
        (DeploymentMode::DriverBacked, "driver_backed"),
    ] {
        assert_eq!(serde_json::to_value(mode).expect("serialize mode"), encoded);
        assert!(schema_modes.iter().any(|item| item == encoded));
    }

    for definition in [
        "inProcessDeployment",
        "inProcessPlatformBinding",
        "sidecarDeployment",
        "externalServiceDeployment",
        "externalServicePlatformBinding",
        "externalServiceProbe",
        "externalServiceConnection",
        "driverBackedDeployment",
        "driverBackedPlatformBinding",
        "userModeControllerBinding",
        "driverDependencyMetadata",
        "capabilityTemplate",
        "portTemplate",
    ] {
        assert_eq!(
            schema["$defs"][definition]["additionalProperties"], false,
            "{definition} must deny unknown fields"
        );
    }

    for relative in [
        "adapters/mock-source/adapter.json",
        "adapters/mock-sink/adapter.json",
    ] {
        let manifest = AdapterManifest::from_json(
            &fs::read(root().join(relative)).expect("read Mock manifest"),
        )
        .expect("manifest matches Rust v2 contract");
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.deployment_modes.len(), 1);
        assert!(manifest.deployment_modes.contains(&DeploymentMode::Sidecar));
        let sidecar = manifest
            .mode_bindings
            .sidecar
            .expect("Mock has Sidecar binding");
        assert_eq!(sidecar.entrypoints.len(), manifest.platforms.len());
        assert!(
            manifest
                .platforms
                .iter()
                .all(|platform| sidecar.entrypoints.contains_key(platform))
        );
    }
}

#[test]
fn in_process_module_and_library_bindings_are_accepted() {
    let value = manifest_for(
        "in_process",
        json!(["android", "ios"]),
        json!({
            "in_process": {
                "bindings": {
                    "android": { "module": "dev.capyio.adapter.TestAdapter" },
                    "ios": { "library": "CapyIOTestAdapter.framework" }
                }
            }
        }),
    );

    let manifest = parse(&value).expect("valid InProcess manifest");
    assert!(
        manifest
            .deployment_modes
            .contains(&DeploymentMode::InProcess)
    );
}

#[test]
fn external_service_probe_and_connection_are_accepted() {
    let value = manifest_for(
        "external_service",
        json!(["windows"]),
        json!({
            "external_service": {
                "services": {
                    "windows": {
                        "probe": {
                            "kind": "http_get",
                            "target": "http://127.0.0.1:8080/health"
                        },
                        "connection": {
                            "kind": "websocket",
                            "endpoint": "ws://127.0.0.1:8080/events"
                        }
                    }
                }
            }
        }),
    );

    let manifest = parse(&value).expect("valid ExternalService manifest");
    assert!(
        manifest
            .deployment_modes
            .contains(&DeploymentMode::ExternalService)
    );
}

#[test]
fn sidecar_requires_an_entrypoint_for_every_sidecar_platform() {
    let value = manifest_for(
        "sidecar",
        json!(["windows", "linux"]),
        json!({
            "sidecar": {
                "entrypoints": { "windows": "capyio-test.exe" }
            }
        }),
    );

    assert!(matches!(
        parse(&value),
        Err(ManifestError::UnboundPlatform(platform)) if platform == "linux"
    ));
}

#[test]
fn driver_backed_requires_controller_and_dependency_metadata_only() {
    let mut value = manifest_for(
        "driver_backed",
        json!(["windows"]),
        json!({
            "driver_backed": {
                "bindings": {
                    "windows": {
                        "controller": {
                            "entrypoint": "capyio-test-controller.exe",
                            "control_interface": "capyio.test.ipc.v1"
                        },
                        "driver_dependency": {
                            "identifier": "dev.capyio.test-driver",
                            "version_requirement": ">=0.1.0",
                            "interface": "capyio.test.driver.v1"
                        }
                    }
                }
            }
        }),
    );

    parse(&value).expect("valid DriverBacked metadata manifest");

    value["mode_bindings"]["driver_backed"]["bindings"]["windows"]["install_command"] =
        json!("pnputil /add-driver test.inf /install");
    assert!(matches!(parse(&value), Err(ManifestError::Json(_))));
}

#[test]
fn undeclared_mode_bindings_and_unknown_fields_are_rejected() {
    let mut value = manifest_for(
        "sidecar",
        json!(["windows"]),
        json!({
            "sidecar": { "entrypoints": { "windows": "capyio-test.exe" } }
        }),
    );
    value["mode_bindings"]["in_process"] = json!({
        "bindings": { "windows": { "library": "capyio-test.dll" } }
    });
    assert!(matches!(
        parse(&value),
        Err(ManifestError::UndeclaredModeBinding(
            DeploymentMode::InProcess
        ))
    ));

    let mut unknown = manifest_for(
        "sidecar",
        json!(["windows"]),
        json!({
            "sidecar": { "entrypoints": { "windows": "capyio-test.exe" } }
        }),
    );
    unknown["unexpected"] = true.into();
    assert!(matches!(parse(&unknown), Err(ManifestError::Json(_))));

    let mut null_binding = manifest_for(
        "sidecar",
        json!(["windows"]),
        json!({ "sidecar": { "entrypoints": { "windows": "capyio-test.exe" } } }),
    );
    null_binding["mode_bindings"]["sidecar"] = Value::Null;
    assert!(matches!(parse(&null_binding), Err(ManifestError::Json(_))));

    let mut duplicate_mode = manifest_for(
        "sidecar",
        json!(["windows"]),
        json!({ "sidecar": { "entrypoints": { "windows": "capyio-test.exe" } } }),
    );
    duplicate_mode["deployment_modes"] = json!(["sidecar", "sidecar"]);
    assert!(matches!(
        parse(&duplicate_mode),
        Err(ManifestError::Json(_))
    ));
}

#[test]
fn unsupported_manifest_version_fails_explicitly() {
    let bytes = fs::read(root().join("adapters/mock-source/adapter.json")).expect("manifest");
    let mut value: Value = serde_json::from_slice(&bytes).expect("JSON");
    value["schema_version"] = 3.into();
    assert!(matches!(
        parse(&value),
        Err(ManifestError::UnsupportedSchemaVersion(3))
    ));

    let legacy_shape = json!({
        "schema_version": 1,
        "entrypoints": { "windows": "legacy-sidecar.exe" }
    });
    assert!(matches!(
        parse(&legacy_shape),
        Err(ManifestError::UnsupportedSchemaVersion(1))
    ));
}
