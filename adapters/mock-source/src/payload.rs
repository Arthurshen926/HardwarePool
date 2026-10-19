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
pub const BATCH_TRANSPORT: &str = "dev.capyio.test.unix-batch-v1";
pub const BATCH_COUNT: u64 = 4;
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
        if !self.test_only
            || (self.token != TOKEN
                && !(0..BATCH_COUNT).any(|sequence| self.token == batch_token(sequence)))
        {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sequence: Option<u64>,
}

/// The host owns this fresh directory, removing only its fixed socket and empty directory.
pub struct Endpoint {
    directory: PathBuf,
    pub descriptor: DataEndpointDescriptor,
}
impl Endpoint {
    pub fn create_batch(stream: StreamId) -> io::Result<Self> {
        let mut endpoint = Self::create(stream)?;
        endpoint.descriptor.transport = BATCH_TRANSPORT.into();
        Ok(endpoint)
    }
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
        if !matches!(endpoint.transport.as_str(), TRANSPORT | BATCH_TRANSPORT)
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
        self.record_at(0)
    }
    #[cfg(target_os = "linux")]
    fn record_at(&self, sequence: u64) -> Record {
        let batch = self.endpoint.transport == BATCH_TRANSPORT;
        Record {
            version: 1,
            route_id: self.route,
            envelope: DataEnvelope {
                profile: ProfileId::new("capyio.test.samples", 1),
                stream_id: self.stream,
                stream_epoch: self.epoch,
                sequence,
                source_timestamp_nanos: sequence + 1,
                receive_timestamp_nanos: sequence + 1,
                clock_domain_id: "synthetic-fixture-clock".into(),
                payload: Token {
                    test_only: true,
                    token: if batch {
                        batch_token(sequence)
                    } else {
                        TOKEN.into()
                    },
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
            || record.envelope.payload.token != TOKEN
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
            first_sequence: None,
            last_sequence: None,
        })
    }
    pub fn start(&self) -> io::Result<Option<DataEndpointDescriptor>> {
        #[cfg(target_os = "linux")]
        {
            if self.endpoint.transport == BATCH_TRANSPORT {
                return self.start_batch();
            }
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
fn batch_token(sequence: u64) -> String {
    format!("finite-batch-sample-{sequence}")
}
fn batch_digest() -> Vec<u8> {
    let mut bytes = Vec::new();
    for sequence in 0..BATCH_COUNT {
        bytes.extend_from_slice(&sequence.to_le_bytes());
        bytes.extend_from_slice(batch_token(sequence).as_bytes());
    }
    bytes
}
fn batch_payload_bytes() -> usize {
    (0..BATCH_COUNT)
        .map(|sequence| batch_token(sequence).len())
        .sum()
}
#[cfg(target_os = "linux")]
fn remaining(deadline: std::time::Instant) -> io::Result<std::time::Duration> {
    deadline
        .checked_duration_since(std::time::Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "fixture batch deadline"))
}

#[cfg(target_os = "linux")]
struct BatchConsumer {
    route: RouteId,
    stream: StreamId,
    epoch: u64,
    next: u64,
    consumed: u64,
    bytes: usize,
    digest: Vec<u8>,
    queue: BoundedEnvelopeQueue<Token>,
}
#[cfg(target_os = "linux")]
impl BatchConsumer {
    fn new(transfer: &Transfer) -> io::Result<Self> {
        Ok(Self {
            route: transfer.route,
            stream: transfer.stream,
            epoch: transfer.epoch,
            next: 0,
            consumed: 0,
            bytes: 0,
            digest: Vec::with_capacity(batch_digest().len()),
            queue: BoundedEnvelopeQueue::new(
                ProfileId::new("capyio.test.samples", 1),
                transfer.stream,
                transfer.epoch,
                2,
            )
            .map_err(invalid)?,
        })
    }
    fn push(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > MAX_RECORD_BYTES || bytes.first() != Some(&b'{') {
            return Err(invalid("invalid batch record size/shape"));
        }
        let record: Record = serde_json::from_slice(bytes).map_err(invalid)?;
        let envelope = &record.envelope;
        if self.next >= BATCH_COUNT
            || record.version != 1
            || record.route_id != self.route
            || envelope.sequence != self.next
            || envelope.source_timestamp_nanos != self.next + 1
            || envelope.receive_timestamp_nanos != self.next + 1
            || envelope.clock_domain_id != "synthetic-fixture-clock"
            || envelope.payload.token != batch_token(self.next)
        {
            return Err(invalid("invalid batch identity/sequence/timing"));
        }
        if self.queue.push(record.envelope).map_err(invalid)?
            != (PushOutcome::Enqueued { gap: None })
        {
            return Err(invalid("batch queue rejected record"));
        }
        self.next += 1;
        Ok(())
    }
    fn consume_one(&mut self) -> io::Result<()> {
        let delivery = self
            .queue
            .pop()
            .ok_or_else(|| invalid("missing batch delivery"))?;
        self.digest
            .extend_from_slice(&delivery.envelope.sequence.to_le_bytes());
        self.digest
            .extend_from_slice(delivery.envelope.payload.token.as_bytes());
        self.bytes += delivery.envelope.payload.token.len();
        self.consumed += 1;
        Ok(())
    }
    fn finish(self) -> io::Result<Receipt> {
        if self.next != BATCH_COUNT || self.consumed != BATCH_COUNT {
            return Err(invalid("incomplete batch"));
        }
        Ok(Receipt {
            route_id: self.route,
            stream_id: self.stream,
            epoch: self.epoch,
            received: self.consumed,
            payload_bytes: self.bytes,
            payload_checksum: checksum(&self.digest),
            first_sequence: Some(0),
            last_sequence: Some(BATCH_COUNT - 1),
        })
    }
}
#[cfg(target_os = "linux")]
impl Transfer {
    fn start_batch(&self) -> io::Result<Option<DataEndpointDescriptor>> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        if let Some(socket) = &self.sink {
            let mut consumer = BatchConsumer::new(self)?;
            let mut bytes = [0u8; MAX_RECORD_BYTES + 1];
            for _ in 0..BATCH_COUNT {
                socket.set_read_timeout(Some(remaining(deadline)?))?;
                let size = socket.recv(&mut bytes)?;
                remaining(deadline)?;
                consumer.push(&bytes[..size])?;
                consumer.consume_one()?;
            }
            let receipt = consumer.finish()?;
            remaining(deadline)?;
            let mut endpoint = self.endpoint.clone();
            endpoint.metadata.insert(
                "receipt".into(),
                serde_json::to_string(&receipt).map_err(invalid)?,
            );
            Ok(Some(endpoint))
        } else {
            let socket = std::os::unix::net::UnixDatagram::unbound()?;
            socket.connect(&self.endpoint.address)?;
            for sequence in 0..BATCH_COUNT {
                let bytes = serde_json::to_vec(&self.record_at(sequence)).map_err(invalid)?;
                if bytes.len() > MAX_RECORD_BYTES {
                    return Err(invalid("batch record too large"));
                }
                socket.set_write_timeout(Some(remaining(deadline)?))?;
                if socket.send(&bytes)? != bytes.len() {
                    return Err(invalid("partial batch write"));
                }
                remaining(deadline)?;
            }
            Ok(None)
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
        || if expected.transport == BATCH_TRANSPORT {
            receipt.received != BATCH_COUNT
                || receipt.payload_bytes != batch_payload_bytes()
                || receipt.payload_checksum != checksum(&batch_digest())
                || receipt.first_sequence != Some(0)
                || receipt.last_sequence != Some(BATCH_COUNT - 1)
        } else {
            expected.transport != TRANSPORT
                || receipt.received != 1
                || receipt.payload_bytes != TOKEN.len()
                || receipt.payload_checksum != checksum(TOKEN.as_bytes())
                || receipt.first_sequence.is_some()
                || receipt.last_sequence.is_some()
        }
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
    fn setup_batch() -> (Endpoint, RoutePrepareRequest) {
        let (mut endpoint, mut request) = setup();
        endpoint.descriptor.transport = BATCH_TRANSPORT.into();
        request.data_endpoint = Some(endpoint.descriptor.clone());
        (endpoint, request)
    }
    fn batch_bytes(transfer: &Transfer, sequence: u64) -> Vec<u8> {
        serde_json::to_vec(&transfer.record_at(sequence)).unwrap()
    }
    #[test]
    fn batch_parser_orders_all_records_and_binds_complete_receipt() {
        let (endpoint, request) = setup_batch();
        let transfer = Transfer::prepare(&request, false).unwrap();
        let mut consumer = BatchConsumer::new(&transfer).unwrap();
        for sequence in 0..BATCH_COUNT {
            consumer.push(&batch_bytes(&transfer, sequence)).unwrap();
            consumer.consume_one().unwrap();
        }
        assert!(consumer.push(&batch_bytes(&transfer, BATCH_COUNT)).is_err());
        let receipt = consumer.finish().unwrap();
        assert_eq!(receipt.received, BATCH_COUNT);
        assert_eq!(receipt.payload_bytes, 84);
        assert_eq!(receipt.payload_checksum, "fnv1a64:d5953109b0ac8ffd");
        assert_eq!(receipt.payload_checksum, checksum(&batch_digest()));
        let mut reversed = Vec::new();
        for sequence in (0..BATCH_COUNT).rev() {
            reversed.extend_from_slice(&sequence.to_le_bytes());
            reversed.extend_from_slice(batch_token(sequence).as_bytes());
        }
        assert_ne!(checksum(&reversed), receipt.payload_checksum);
        for case in 0..7 {
            let mut changed = receipt.clone();
            match case {
                0 => {}
                1 => changed.received = 3,
                2 => changed.first_sequence = Some(1),
                3 => changed.last_sequence = Some(2),
                4 => changed.payload_checksum = checksum(&reversed),
                5 => changed.payload_bytes -= 1,
                _ => changed.epoch += 1,
            }
            let mut returned = endpoint.descriptor.clone();
            returned
                .metadata
                .insert("receipt".into(), serde_json::to_string(&changed).unwrap());
            assert_eq!(
                validate_receipt(Some(returned), &endpoint.descriptor, request.route_id, 1).is_ok(),
                case == 0
            );
        }
    }
    #[test]
    fn batch_parser_rejects_gaps_replay_binding_timing_and_partial_completion() {
        let (_endpoint, request) = setup_batch();
        let transfer = Transfer::prepare(&request, false).unwrap();
        for case in 0..15 {
            let mut consumer = BatchConsumer::new(&transfer).unwrap();
            consumer.push(&batch_bytes(&transfer, 0)).unwrap();
            consumer.consume_one().unwrap();
            let mut record = transfer.record_at(1);
            match case {
                0 => record.envelope.sequence = 0,
                1 => record.envelope.sequence = 2,
                2 => record.route_id = RouteId::new(),
                3 => record.envelope.stream_id = StreamId::new(),
                4 => record.envelope.stream_epoch = 0,
                5 => record.envelope.stream_epoch = 2,
                6 => record.envelope.profile.major = 2,
                7 => record.envelope.source_timestamp_nanos = 1,
                8 => record.envelope.receive_timestamp_nanos = 1,
                9 => record.envelope.clock_domain_id = "wrong".into(),
                10 => record.envelope.payload.token = batch_token(2),
                11 => record.envelope.payload.test_only = false,
                12 => record.version = 2,
                13 => record.envelope.payload.token = TOKEN.into(),
                _ => record.envelope.payload.token = "finite-batch-samplf-1".into(),
            }
            assert!(
                consumer
                    .push(&serde_json::to_vec(&record).unwrap())
                    .is_err(),
                "case {case}"
            );
            assert_eq!(consumer.next, 1);
            consumer.push(&batch_bytes(&transfer, 1)).unwrap();
            consumer.consume_one().unwrap();
            assert!(consumer.finish().is_err());
        }
        let mut consumer = BatchConsumer::new(&transfer).unwrap();
        assert!(consumer.push(&batch_bytes(&transfer, 1)).is_err());
        for bytes in [
            vec![],
            b"[]".to_vec(),
            b"{bad".to_vec(),
            vec![b'x'; MAX_RECORD_BYTES + 1],
        ] {
            assert!(consumer.push(&bytes).is_err());
        }
        let mut extra = serde_json::to_value(transfer.record_at(0)).unwrap();
        extra["extra"] = serde_json::json!(true);
        assert!(consumer.push(&serde_json::to_vec(&extra).unwrap()).is_err());
        assert!(
            remaining(std::time::Instant::now() - std::time::Duration::from_millis(1)).is_err()
        );
    }
    #[test]
    fn batch_queue_overflow_is_transactional_without_eviction() {
        let (_endpoint, request) = setup_batch();
        let transfer = Transfer::prepare(&request, false).unwrap();
        let mut consumer = BatchConsumer::new(&transfer).unwrap();
        consumer.push(&batch_bytes(&transfer, 0)).unwrap();
        consumer.push(&batch_bytes(&transfer, 1)).unwrap();
        assert!(consumer.push(&batch_bytes(&transfer, 2)).is_err());
        assert_eq!(consumer.next, 2);
        consumer.consume_one().unwrap();
        consumer.push(&batch_bytes(&transfer, 2)).unwrap();
        consumer.consume_one().unwrap();
        consumer.push(&batch_bytes(&transfer, 3)).unwrap();
        consumer.consume_one().unwrap();
        consumer.consume_one().unwrap();
        assert_eq!(
            consumer.finish().unwrap().payload_checksum,
            checksum(&batch_digest())
        );
    }
    #[test]
    fn batch_separate_socket_transfer_and_complete_receipt() {
        let (endpoint, request) = setup_batch();
        let sink = Transfer::prepare(&request, true).unwrap();
        let source = Transfer::prepare(&request, false).unwrap();
        source.start().unwrap();
        let receipt = validate_receipt(
            sink.start().unwrap(),
            &endpoint.descriptor,
            request.route_id,
            1,
        )
        .unwrap();
        assert_eq!(receipt.received, BATCH_COUNT);
        assert_eq!(receipt.first_sequence, Some(0));
        assert_eq!(receipt.last_sequence, Some(3));
        let path = endpoint.directory.clone();
        drop(source);
        drop(sink);
        drop(endpoint);
        assert!(!path.exists());
    }
    #[test]
    fn batch_paced_partial_sender_cannot_renew_whole_transfer_budget() {
        let (endpoint, request) = setup_batch();
        let sink = Transfer::prepare(&request, true).unwrap();
        let socket = std::os::unix::net::UnixDatagram::unbound().unwrap();
        socket
            .set_write_timeout(Some(std::time::Duration::from_millis(100)))
            .unwrap();
        socket
            .send_to(&batch_bytes(&sink, 0), &endpoint.descriptor.address)
            .unwrap();
        let second = batch_bytes(&sink, 1);
        let address = endpoint.descriptor.address.clone();
        // Test-only pacing, not a production worker. Exactly one bounded send;
        // always join before asserting or releasing the owned socket path.
        let sender = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(350));
            socket.send_to(&second, address)
        });
        let now = std::time::Instant::now();
        let result = sink.start();
        let elapsed = now.elapsed();
        let sent = sender.join();
        let path = endpoint.directory.clone();
        drop(sink);
        drop(endpoint);
        assert!(!path.exists());
        assert!(sent.unwrap().is_ok());
        assert!(result.is_err());
        // A renewed timeout would expire around 850 ms; the shared budget is
        // 500 ms. Allow 250 ms scheduler jitter without accepting that renewal.
        assert!(
            elapsed < std::time::Duration::from_millis(750),
            "batch elapsed: {elapsed:?}"
        );
    }
    #[test]
    fn batch_saturated_kernel_queue_bounds_source_failure_and_cleans_endpoint() {
        let (endpoint, request) = setup_batch();
        let sink = Transfer::prepare(&request, true).unwrap();
        let source = Transfer::prepare(&request, false).unwrap();
        let filler = std::os::unix::net::UnixDatagram::unbound().unwrap();
        filler.connect(&endpoint.descriptor.address).unwrap();
        filler.set_nonblocking(true).unwrap();
        // Discover saturation instead of assuming a host's max_dgram_qlen.
        // Both attempt count and elapsed setup time are bounded. The Sink never
        // receives, so no concurrent consumer can free space during this test.
        let setup_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut saturated = false;
        for _ in 0..4096 {
            assert!(
                std::time::Instant::now() < setup_deadline,
                "kernel queue saturation setup exceeded its time bound"
            );
            match filler.send(b"x") {
                Ok(1) => {}
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    saturated = true;
                    break;
                }
                result => panic!("unexpected queue prefill result: {result:?}"),
            }
        }
        assert!(
            saturated,
            "kernel queue not saturated within 4096 datagrams"
        );
        // A fresh sender must also be blocked: exhaustion of the filler's own
        // send buffer alone is not evidence that the receiver queue is full.
        let probe = std::os::unix::net::UnixDatagram::unbound().unwrap();
        probe.connect(&endpoint.descriptor.address).unwrap();
        probe.set_nonblocking(true).unwrap();
        assert_eq!(
            probe.send(b"x").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        let now = std::time::Instant::now();
        let result = source.start();
        let elapsed = now.elapsed();
        let path = endpoint.directory.clone();
        let socket_path = std::path::PathBuf::from(&endpoint.descriptor.address);
        // Preserve owner order even when the operation failed; no Sink Start or
        // successful receipt is produced, and all sockets close before unlink.
        drop(probe);
        drop(filler);
        drop(source);
        drop(sink);
        drop(endpoint);
        assert!(!socket_path.exists());
        assert!(!path.exists());
        assert!(matches!(
            result.unwrap_err().kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ));
        // The production I/O budget is 500 ms; retain the existing timeout
        // tests' scheduler tolerance rather than requiring precise wall time.
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "source elapsed: {elapsed:?}"
        );
    }

    #[test]
    fn batch_missing_partial_and_oversized_input_fail_within_budget() {
        let (endpoint, request) = setup_batch();
        assert!(Transfer::prepare(&request, false).unwrap().start().is_err());
        let sink = Transfer::prepare(&request, true).unwrap();
        let now = std::time::Instant::now();
        assert!(sink.start().is_err());
        assert!(now.elapsed() < std::time::Duration::from_secs(2));
        let socket = std::os::unix::net::UnixDatagram::unbound().unwrap();
        socket
            .send_to(&batch_bytes(&sink, 0), &endpoint.descriptor.address)
            .unwrap();
        let now = std::time::Instant::now();
        assert!(sink.start().is_err());
        assert!(now.elapsed() < std::time::Duration::from_secs(2));
        let mut bytes = batch_bytes(&sink, 0);
        bytes.resize(MAX_RECORD_BYTES + 100, b' ');
        socket
            .send_to(&bytes, &endpoint.descriptor.address)
            .unwrap();
        assert!(sink.start().is_err());
        let path = endpoint.directory.clone();
        drop(socket);
        drop(sink);
        drop(endpoint);
        assert!(!path.exists());
    }
}
