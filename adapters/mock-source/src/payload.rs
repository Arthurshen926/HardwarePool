//! Private finite Linux-local fixture IPC, never an IP transport or public wire.
use capyio_adapter_sdk::{DataEndpointDescriptor, RoutePrepareRequest};
use capyio_core::{ProfileId, RouteId, StreamId};
#[cfg(target_os = "linux")]
use capyio_data_plane::{
    BoundedEnvelopeQueue, DataEnvelope, DataPayload, DataPlaneError, PushOutcome,
};
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};

pub const TRANSPORT: &str = "dev.capyio.test.unix-datagram-v1";
pub const MAX_RECORD_BYTES: usize = 2048;
const TOKEN: &str = "finite-source-sample";
// Diagnostic fixture integrity only, not authentication or a cryptographic hash.
fn checksum(bytes: &[u8]) -> String {
    let value = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("fnv1a64:{value:016x}")
}
fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}
#[cfg(target_os = "linux")]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Token {
    test_only: bool,
    token: String,
}
#[cfg(target_os = "linux")]
impl DataPayload for Token {
    fn validate_payload(&self) -> Result<(), DataPlaneError> {
        if !self.test_only || self.token != TOKEN {
            return Err(DataPlaneError::InvalidPayload(
                "invalid fixture token".into(),
            ));
        }
        Ok(())
    }
}
#[cfg(target_os = "linux")]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    route_id: RouteId,
    envelope: DataEnvelope<Token>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub epoch: u64,
    pub received: u64,
    pub payload_bytes: usize,
    pub payload_checksum: String,
}

/// The host owns this fresh directory, removing only its fixed socket and empty directory.
pub struct Endpoint {
    directory: PathBuf,
    pub descriptor: DataEndpointDescriptor,
}
impl Endpoint {
    pub fn create(stream: StreamId) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::DirBuilderExt;
            let directory = std::env::temp_dir().join(format!("capyio-mock-{stream}"));
            std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
            let address = directory.join("sink");
            let Some(address) = address.to_str().filter(|s| s.len() <= 100) else {
                let _ = std::fs::remove_dir(&directory);
                return Err(invalid("fixture socket path too long or not UTF-8"));
            };
            Ok(Self {
                descriptor: DataEndpointDescriptor {
                    transport: TRANSPORT.into(),
                    address: address.into(),
                    metadata: std::collections::BTreeMap::from([(
                        "stream_id".into(),
                        stream.to_string(),
                    )]),
                },
                directory,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = stream;
            Err(invalid("mock payload requires Linux"))
        }
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.directory.join("sink"));
        let _ = std::fs::remove_dir(&self.directory);
    }
}

pub struct Transfer {
    #[cfg(target_os = "linux")]
    route: RouteId,
    #[cfg(target_os = "linux")]
    epoch: u64,
    #[cfg(target_os = "linux")]
    stream: StreamId,
    #[cfg(target_os = "linux")]
    endpoint: DataEndpointDescriptor,
    #[cfg(target_os = "linux")]
    sink: Option<std::os::unix::net::UnixDatagram>,
}
impl Transfer {
    pub fn prepare(request: &RoutePrepareRequest, sink: bool) -> io::Result<Self> {
        let endpoint = request
            .data_endpoint
            .as_ref()
            .ok_or_else(|| invalid("missing endpoint"))?;
        if endpoint.transport != TRANSPORT
            || endpoint.metadata.len() != 1
            || request.profile != ProfileId::new("capyio.test.samples", 1)
            || !request
                .selected_format
                .as_ref()
                .is_some_and(|f| f.id == "utf8-test-token" && f.parameters.is_empty())
            || request.selected_qos != capyio_core::QosMode::Basic
            || request.backend != capyio_core::RouteBackend::CapyDataPlane
            || request.epoch == 0
        {
            return Err(invalid("unsupported fixture binding"));
        }
        let stream = endpoint
            .metadata
            .get("stream_id")
            .ok_or_else(|| invalid("missing stream"))?
            .parse::<StreamId>()
            .map_err(invalid)?;
        if stream.as_uuid().is_nil() {
            return Err(invalid("nil stream"));
        }
        let path = std::path::Path::new(&endpoint.address);
        if !path.is_absolute()
            || path.file_name().and_then(|s| s.to_str()) != Some("sink")
            || endpoint.address.len() > 100
        {
            return Err(invalid("invalid endpoint path"));
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::symlink_metadata(
                path.parent().ok_or_else(|| invalid("missing directory"))?,
            )?;
            if !metadata.is_dir() || metadata.permissions().mode() & 0o777 != 0o700 {
                return Err(invalid("endpoint directory must be private"));
            }
            let socket = if sink {
                Some(std::os::unix::net::UnixDatagram::bind(path)?)
            } else {
                None
            };
            if let Some(socket) = &socket {
                socket.set_read_timeout(Some(std::time::Duration::from_millis(500)))?;
            }
            Ok(Self {
                route: request.route_id,
                epoch: request.epoch,
                stream,
                endpoint: endpoint.clone(),
                sink: socket,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (stream, sink);
            Err(invalid("mock payload requires Linux"))
        }
    }
    #[cfg(target_os = "linux")]
    fn record(&self) -> Record {
        Record {
            version: 1,
            route_id: self.route,
            envelope: DataEnvelope {
                profile: ProfileId::new("capyio.test.samples", 1),
                stream_id: self.stream,
                stream_epoch: self.epoch,
                sequence: 0,
                source_timestamp_nanos: 1,
                receive_timestamp_nanos: 1,
                clock_domain_id: "synthetic-fixture-clock".into(),
                payload: Token {
                    test_only: true,
                    token: TOKEN.into(),
                },
            },
        }
    }
    #[cfg(target_os = "linux")]
    fn consume(&self, bytes: &[u8]) -> io::Result<Receipt> {
        if bytes.len() > MAX_RECORD_BYTES || bytes.first() != Some(&b'{') {
            return Err(invalid("invalid record size/shape"));
        }
        let record: Record = serde_json::from_slice(bytes).map_err(invalid)?;
        if record.version != 1
            || record.route_id != self.route
            || record.envelope.sequence != 0
            || record.envelope.clock_domain_id != "synthetic-fixture-clock"
            || record.envelope.source_timestamp_nanos != 1
            || record.envelope.receive_timestamp_nanos != 1
        {
            return Err(invalid("invalid fixture identity/timing"));
        }
        let mut queue = BoundedEnvelopeQueue::new(
            ProfileId::new("capyio.test.samples", 1),
            self.stream,
            self.epoch,
            1,
        )
        .map_err(invalid)?;
        if queue.push(record.envelope).map_err(invalid)? != (PushOutcome::Enqueued { gap: None }) {
            return Err(invalid("fixture stream/epoch rejected"));
        }
        let delivered = queue.pop().ok_or_else(|| invalid("missing delivery"))?;
        Ok(Receipt {
            route_id: self.route,
            stream_id: self.stream,
            epoch: self.epoch,
            received: 1,
            payload_bytes: delivered.envelope.payload.token.len(),
            payload_checksum: checksum(delivered.envelope.payload.token.as_bytes()),
        })
    }
    pub fn start(&self) -> io::Result<Option<DataEndpointDescriptor>> {
        #[cfg(target_os = "linux")]
        {
            if let Some(socket) = &self.sink {
                let mut bytes = [0u8; MAX_RECORD_BYTES + 1];
                let size = socket.recv(&mut bytes)?;
                let receipt = self.consume(&bytes[..size])?;
                let mut endpoint = self.endpoint.clone();
                endpoint.metadata.insert(
                    "receipt".into(),
                    serde_json::to_string(&receipt).map_err(invalid)?,
                );
                Ok(Some(endpoint))
            } else {
                let bytes = serde_json::to_vec(&self.record()).map_err(invalid)?;
                if bytes.len() > MAX_RECORD_BYTES {
                    return Err(invalid("fixture too large"));
                }
                let socket = std::os::unix::net::UnixDatagram::unbound()?;
                socket.set_write_timeout(Some(std::time::Duration::from_millis(500)))?;
                socket.connect(&self.endpoint.address)?;
                if socket.send(&bytes)? != bytes.len() {
                    return Err(invalid("partial fixture write"));
                }
                Ok(None)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(invalid("mock payload requires Linux"))
        }
    }
}
pub fn validate_receipt(
    endpoint: Option<DataEndpointDescriptor>,
    expected: &DataEndpointDescriptor,
    route: RouteId,
    epoch: u64,
) -> io::Result<Receipt> {
    let endpoint = endpoint.ok_or_else(|| invalid("missing sink receipt"))?;
    if endpoint.transport != expected.transport
        || endpoint.address != expected.address
        || endpoint.metadata.len() != 2
        || endpoint.metadata.get("stream_id") != expected.metadata.get("stream_id")
    {
        return Err(invalid("receipt endpoint mismatch"));
    }
    let receipt: Receipt = serde_json::from_str(
        endpoint
            .metadata
            .get("receipt")
            .ok_or_else(|| invalid("missing receipt"))?,
    )
    .map_err(invalid)?;
    if receipt.route_id != route
        || receipt.epoch != epoch
        || Some(&receipt.stream_id.to_string()) != expected.metadata.get("stream_id")
        || receipt.received != 1
        || receipt.payload_bytes != TOKEN.len()
        || receipt.payload_checksum != checksum(TOKEN.as_bytes())
    {
        return Err(invalid("receipt binding mismatch"));
    }
    Ok(receipt)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use capyio_adapter_sdk::BoundedJsonObject;
    use capyio_core::{FormatDescriptor, PortRef, QosMode, RouteBackend};
    fn setup() -> (Endpoint, RoutePrepareRequest) {
        let endpoint = Endpoint::create(StreamId::new()).unwrap();
        let port = PortRef {
            node_id: capyio_core::NodeId::new(),
            capability_id: capyio_core::CapabilityId::new(),
            port_id: capyio_core::PortId::new(),
        };
        let request = RoutePrepareRequest {
            route_id: RouteId::new(),
            source: port,
            sink: port,
            profile: ProfileId::new("capyio.test.samples", 1),
            selected_format: Some(FormatDescriptor::new("utf8-test-token")),
            selected_qos: QosMode::Basic,
            backend: RouteBackend::CapyDataPlane,
            epoch: 1,
            data_endpoint: Some(endpoint.descriptor.clone()),
            adapter_config: BoundedJsonObject::default(),
        };
        (endpoint, request)
    }
    #[test]
    fn parser_consumes_exact_fixture_and_validates_receipt() {
        let (endpoint, request) = setup();
        let transfer = Transfer::prepare(&request, false).unwrap();
        let receipt = transfer
            .consume(&serde_json::to_vec(&transfer.record()).unwrap())
            .unwrap();
        let mut returned = endpoint.descriptor.clone();
        returned
            .metadata
            .insert("receipt".into(), serde_json::to_string(&receipt).unwrap());
        assert_eq!(
            validate_receipt(
                Some(returned.clone()),
                &endpoint.descriptor,
                request.route_id,
                1
            )
            .unwrap(),
            receipt
        );
        assert!(validate_receipt(None, &endpoint.descriptor, request.route_id, 1).is_err());
        assert!(
            validate_receipt(
                Some(returned.clone()),
                &endpoint.descriptor,
                RouteId::new(),
                1
            )
            .is_err()
        );
        assert!(
            validate_receipt(
                Some(returned.clone()),
                &endpoint.descriptor,
                request.route_id,
                2
            )
            .is_err()
        );
        let mut wrong_checksum = receipt.clone();
        wrong_checksum.payload_checksum = "fnv1a64:0000000000000000".into();
        let mut wrong_endpoint = returned.clone();
        wrong_endpoint.metadata.insert(
            "receipt".into(),
            serde_json::to_string(&wrong_checksum).unwrap(),
        );
        assert!(
            validate_receipt(
                Some(wrong_endpoint),
                &endpoint.descriptor,
                request.route_id,
                1
            )
            .is_err()
        );
        assert_eq!(receipt.payload_checksum, checksum(TOKEN.as_bytes()));
        let mut wrong = receipt.clone();
        wrong.stream_id = StreamId::new();
        returned
            .metadata
            .insert("receipt".into(), serde_json::to_string(&wrong).unwrap());
        assert!(
            validate_receipt(Some(returned), &endpoint.descriptor, request.route_id, 1).is_err()
        );
        assert_eq!(receipt.received, 1);
        assert_eq!(receipt.payload_bytes, TOKEN.len());
    }
    #[test]
    fn parser_rejects_wrong_identity_epoch_profile_and_corruption() {
        let (_endpoint, request) = setup();
        let transfer = Transfer::prepare(&request, false).unwrap();
        for case in 0..12 {
            let mut record = transfer.record();
            match case {
                0 => record.version = 2,
                1 => record.route_id = RouteId::new(),
                2 => record.envelope.stream_id = StreamId::new(),
                3 => record.envelope.stream_epoch = 0,
                4 => record.envelope.stream_epoch = 2,
                5 => record.envelope.profile.major = 2,
                6 => record.envelope.sequence = 1,
                7 => record.envelope.payload.token = "finite-sourcf-sample".into(),
                8 => record.envelope.payload.test_only = false,
                9 => record.envelope.clock_domain_id = "wrong".into(),
                10 => record.envelope.source_timestamp_nanos = 2,
                _ => record.envelope.receive_timestamp_nanos = 2,
            }
            assert!(
                transfer
                    .consume(&serde_json::to_vec(&record).unwrap())
                    .is_err(),
                "case {case}"
            );
        }
        for bytes in [
            vec![],
            b"[]".to_vec(),
            b"{bad".to_vec(),
            vec![b'x'; MAX_RECORD_BYTES + 1],
        ] {
            assert!(transfer.consume(&bytes).is_err());
        }
        let mut next_request = request.clone();
        next_request.epoch = 2;
        let next = Transfer::prepare(&next_request, false).unwrap();
        assert!(
            next.consume(&serde_json::to_vec(&transfer.record()).unwrap())
                .is_err()
        );
        let mut valid = serde_json::to_value(transfer.record()).unwrap();
        valid["extra"] = serde_json::json!(true);
        assert!(
            transfer
                .consume(&serde_json::to_vec(&valid).unwrap())
                .is_err()
        );
    }
    #[test]
    fn endpoint_contract_and_private_directory_are_required() {
        let (endpoint, request) = setup();
        for case in 0..9 {
            let mut request = request.clone();
            match case {
                0 => request.data_endpoint = None,
                1 => request.data_endpoint.as_mut().unwrap().transport = "udp".into(),
                2 => {
                    request
                        .data_endpoint
                        .as_mut()
                        .unwrap()
                        .metadata
                        .insert("extra".into(), "x".into());
                }
                3 => request.data_endpoint.as_mut().unwrap().address = "relative/sink".into(),
                4 => request.profile.major = 2,
                5 => request.selected_format = Some(FormatDescriptor::new("pcm")),
                6 => request.backend = RouteBackend::AdapterManaged,
                7 => {
                    request
                        .selected_format
                        .as_mut()
                        .unwrap()
                        .parameters
                        .insert("extra".into(), "x".into());
                }
                _ => request.epoch = 0,
            }
            assert!(Transfer::prepare(&request, false).is_err());
        }
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&endpoint.directory, std::fs::Permissions::from_mode(0o755))
            .unwrap();
        assert!(Transfer::prepare(&request, false).is_err());
        let path = endpoint.directory.clone();
        drop(endpoint);
        assert!(!path.exists());
    }
    // These IPC tests remain required and are intentionally not marked ignored.
    #[test]
    fn separate_socket_transfer_and_bound_receipt() {
        let (endpoint, request) = setup();
        let sink = Transfer::prepare(&request, true).unwrap();
        let source = Transfer::prepare(&request, false).unwrap();
        assert!(source.start().unwrap().is_none());
        let receipt = validate_receipt(
            sink.start().unwrap(),
            &endpoint.descriptor,
            request.route_id,
            1,
        )
        .unwrap();
        assert_eq!(receipt.received, 1);
        let path = endpoint.directory.clone();
        drop(source);
        drop(sink);
        drop(endpoint);
        assert!(!path.exists());
    }
    #[test]
    fn missing_sink_and_sender_have_bounded_failures() {
        let (_endpoint, request) = setup();
        assert!(Transfer::prepare(&request, false).unwrap().start().is_err());
        let sink = Transfer::prepare(&request, true).unwrap();
        let now = std::time::Instant::now();
        assert!(sink.start().is_err());
        assert!(now.elapsed() < std::time::Duration::from_secs(2));
    }
    #[test]
    fn oversized_datagram_is_not_accepted_after_truncation() {
        let (endpoint, request) = setup();
        let sink = Transfer::prepare(&request, true).unwrap();
        let socket = std::os::unix::net::UnixDatagram::unbound().unwrap();
        let mut oversized = serde_json::to_vec(&sink.record()).unwrap();
        oversized.resize(MAX_RECORD_BYTES + 100, b' ');
        socket
            .send_to(&oversized, &endpoint.descriptor.address)
            .unwrap();
        assert!(sink.start().is_err());
    }
}
