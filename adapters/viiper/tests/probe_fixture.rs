use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};

use capyio_viiper_adapter::{
    CompatibleViiperVersion, EXPERIMENTAL_VIIPER_DS4WINDOWS_V012_VERSION,
    EXPERIMENTAL_VIIPER_DS4WINDOWS_VERSION, EXPERIMENTAL_VIIPER_URB_FIX_VERSION,
    MAX_VIIPER_CONNECTION_TIMEOUT, MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES, PINNED_VIIPER_SERVER,
    PINNED_VIIPER_VERSION, ViiperClientError, ViiperLoopbackClient, ViiperLoopbackConfig,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(2);

#[test]
fn config_rejects_implicit_or_unbounded_network_scope() {
    let public = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 3242);
    assert!(matches!(
        ViiperLoopbackConfig::new(public, TEST_TIMEOUT, TEST_TIMEOUT, 128),
        Err(ViiperClientError::NonLoopbackAddress(address)) if address == public
    ));
    assert_eq!(
        ViiperLoopbackConfig::new(
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            TEST_TIMEOUT,
            TEST_TIMEOUT,
            128,
        ),
        Err(ViiperClientError::InvalidPort)
    );

    let endpoint = SocketAddr::from((Ipv4Addr::LOCALHOST, 3242));
    for (connect_timeout, io_timeout) in [
        (Duration::ZERO, TEST_TIMEOUT),
        (TEST_TIMEOUT, Duration::ZERO),
        (
            MAX_VIIPER_CONNECTION_TIMEOUT + Duration::from_nanos(1),
            TEST_TIMEOUT,
        ),
        (
            TEST_TIMEOUT,
            MAX_VIIPER_CONNECTION_TIMEOUT + Duration::from_nanos(1),
        ),
    ] {
        assert_eq!(
            ViiperLoopbackConfig::new(endpoint, connect_timeout, io_timeout, 128),
            Err(ViiperClientError::InvalidTimeout)
        );
    }
    assert!(matches!(
        ViiperLoopbackConfig::new(endpoint, TEST_TIMEOUT, TEST_TIMEOUT, 0),
        Err(ViiperClientError::InvalidResponseLimit { actual: 0, .. })
    ));
    assert!(matches!(
        ViiperLoopbackConfig::new(
            endpoint,
            TEST_TIMEOUT,
            TEST_TIMEOUT,
            MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES + 1,
        ),
        Err(ViiperClientError::InvalidResponseLimit { .. })
    ));
}

#[test]
fn probe_sends_exact_nul_request_and_requires_pinned_identity() {
    let response = compatible_response();
    let fixture = Fixture::respond(response.clone());
    let client = client(fixture.address, response.len(), TEST_TIMEOUT);

    let probe = client.probe().unwrap();

    assert_eq!(probe.address(), fixture.address);
    assert_eq!(probe.server(), PINNED_VIIPER_SERVER);
    assert_eq!(probe.version(), PINNED_VIIPER_VERSION);
    assert_eq!(probe.compatibility(), CompatibleViiperVersion::ReleaseV070);
    assert!(!probe.compatibility().is_experimental());
    fixture.finish(b"ping\0");
}

#[test]
fn probe_accepts_only_the_explicit_experimental_urb_fix_identity() {
    let response = format!(
        "{{\"server\":\"VIIPER\",\"version\":\"{EXPERIMENTAL_VIIPER_URB_FIX_VERSION}\"}}\n"
    )
    .into_bytes();
    let fixture = Fixture::respond(response.clone());

    let probe = client(fixture.address, response.len(), TEST_TIMEOUT)
        .probe()
        .unwrap();

    assert_eq!(probe.version(), EXPERIMENTAL_VIIPER_URB_FIX_VERSION);
    assert_eq!(
        probe.compatibility(),
        CompatibleViiperVersion::ExperimentalUrbFix88f66f1
    );
    assert!(probe.compatibility().is_experimental());
    fixture.finish(b"ping\0");

    let other_experimental =
        Fixture::respond(br#"{"server":"VIIPER","version":"0.7.0-capyio-other"}"#.to_vec());
    assert_eq!(
        client(other_experimental.address, 128, TEST_TIMEOUT).probe(),
        Err(ViiperClientError::UnsupportedVersion(
            "0.7.0-capyio-other".into()
        ))
    );
    other_experimental.finish(b"ping\0");
}

#[test]
fn probe_accepts_only_the_explicit_ds4windows_experimental_identity() {
    let response = format!(
        "{{\"server\":\"VIIPER\",\"version\":\"{EXPERIMENTAL_VIIPER_DS4WINDOWS_VERSION}\"}}\n"
    )
    .into_bytes();
    let fixture = Fixture::respond(response.clone());

    let probe = client(fixture.address, response.len(), TEST_TIMEOUT)
        .probe()
        .unwrap();

    assert_eq!(probe.version(), EXPERIMENTAL_VIIPER_DS4WINDOWS_VERSION);
    assert_eq!(
        probe.compatibility(),
        CompatibleViiperVersion::ExperimentalDs4WindowsFd298a0
    );
    assert!(probe.compatibility().is_experimental());
    fixture.finish(b"ping\0");

    let other_fork_build =
        Fixture::respond(br#"{"server":"VIIPER","version":"0.1.0-capyio-other"}"#.to_vec());
    assert_eq!(
        client(other_fork_build.address, 128, TEST_TIMEOUT).probe(),
        Err(ViiperClientError::UnsupportedVersion(
            "0.1.0-capyio-other".into()
        ))
    );
    other_fork_build.finish(b"ping\0");
}

#[test]
fn probe_accepts_only_the_reviewed_ds4windows_v012_identity() {
    let response = format!(
        "{{\"server\":\"VIIPER\",\"version\":\"{EXPERIMENTAL_VIIPER_DS4WINDOWS_V012_VERSION}\"}}\n"
    )
    .into_bytes();
    let fixture = Fixture::respond(response.clone());

    let probe = client(fixture.address, response.len(), TEST_TIMEOUT)
        .probe()
        .unwrap();

    assert_eq!(probe.version(), EXPERIMENTAL_VIIPER_DS4WINDOWS_V012_VERSION);
    assert_eq!(
        probe.compatibility(),
        CompatibleViiperVersion::ExperimentalDs4WindowsV012
    );
    assert!(probe.compatibility().is_experimental());
    fixture.finish(b"ping\0");
}

#[test]
fn probe_rejects_wrong_server_and_unpinned_version() {
    let wrong_server = Fixture::respond(br#"{"server":"NOT-VIIPER","version":"0.7.0"}"#.to_vec());
    assert_eq!(
        client(wrong_server.address, 128, TEST_TIMEOUT).probe(),
        Err(ViiperClientError::UnexpectedServer("NOT-VIIPER".into()))
    );
    wrong_server.finish(b"ping\0");

    let wrong_version = Fixture::respond(br#"{"server":"VIIPER","version":"0.7.1"}"#.to_vec());
    assert_eq!(
        client(wrong_version.address, 128, TEST_TIMEOUT).probe(),
        Err(ViiperClientError::UnsupportedVersion("0.7.1".into()))
    );
    wrong_version.finish(b"ping\0");
}

#[test]
fn probe_classifies_problem_empty_and_malformed_responses() {
    let problem =
        Fixture::respond(br#"{"status":409,"title":"Conflict","detail":"fixture only"}"#.to_vec());
    assert_eq!(
        client(problem.address, 128, TEST_TIMEOUT).probe(),
        Err(ViiperClientError::RemoteProblem {
            status: 409,
            title: "Conflict".into(),
            detail: "fixture only".into(),
        })
    );
    problem.finish(b"ping\0");

    let empty = Fixture::respond(b" \r\n\t".to_vec());
    assert_eq!(
        client(empty.address, 128, TEST_TIMEOUT).probe(),
        Err(ViiperClientError::EmptyResponse)
    );
    empty.finish(b"ping\0");

    let malformed =
        Fixture::respond(br#"{"server":"VIIPER","version":"0.7.0"}{"second":true}"#.to_vec());
    assert!(matches!(
        client(malformed.address, 128, TEST_TIMEOUT).probe(),
        Err(ViiperClientError::InvalidJson(_))
    ));
    malformed.finish(b"ping\0");
}

#[test]
fn response_limit_accepts_exact_size_and_rejects_one_extra_byte() {
    let response = compatible_response();
    let exact = Fixture::respond(response.clone());
    assert!(
        client(exact.address, response.len(), TEST_TIMEOUT)
            .probe()
            .is_ok()
    );
    exact.finish(b"ping\0");

    let oversized = Fixture::respond(response.clone());
    assert_eq!(
        client(oversized.address, response.len() - 1, TEST_TIMEOUT).probe(),
        Err(ViiperClientError::ResponseTooLarge {
            actual_at_least: response.len(),
            maximum: response.len() - 1,
        })
    );
    oversized.finish(b"ping\0");
}

#[test]
fn complete_json_without_eof_times_out_instead_of_accepting_a_prefix() {
    let (fixture, release) = HeldOpenFixture::respond(compatible_response());
    let result = client(fixture.address, 128, Duration::from_millis(50)).probe();
    assert_eq!(result, Err(ViiperClientError::ResponseTimedOut));
    release.send(()).unwrap();
    fixture.finish(b"ping\0");
}

fn compatible_response() -> Vec<u8> {
    br#"{"server":"VIIPER","version":"0.7.0"}
"#
    .to_vec()
}

fn client(
    address: SocketAddr,
    response_limit: usize,
    io_timeout: Duration,
) -> ViiperLoopbackClient {
    let config =
        ViiperLoopbackConfig::new(address, TEST_TIMEOUT, io_timeout, response_limit).unwrap();
    ViiperLoopbackClient::new(config)
}

struct Fixture {
    address: SocketAddr,
    request: Receiver<Vec<u8>>,
    server: JoinHandle<()>,
}

impl Fixture {
    fn respond(response: Vec<u8>) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (request_tx, request) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, peer) = listener.accept().unwrap();
            assert!(peer.ip().is_loopback());
            stream.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
            stream.set_write_timeout(Some(TEST_TIMEOUT)).unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            request_tx.send(bytes).unwrap();
            stream.write_all(&response).unwrap();
        });
        Self {
            address,
            request,
            server,
        }
    }

    fn finish(self, expected_request: &[u8]) {
        assert_eq!(
            self.request.recv_timeout(TEST_TIMEOUT).unwrap(),
            expected_request
        );
        self.server.join().unwrap();
    }
}

struct HeldOpenFixture {
    address: SocketAddr,
    request: Receiver<Vec<u8>>,
    server: JoinHandle<()>,
}

impl HeldOpenFixture {
    fn respond(response: Vec<u8>) -> (Self, mpsc::Sender<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (request_tx, request) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, peer) = listener.accept().unwrap();
            assert!(peer.ip().is_loopback());
            stream.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
            stream.set_write_timeout(Some(TEST_TIMEOUT)).unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            request_tx.send(bytes).unwrap();
            stream.write_all(&response).unwrap();
            release_rx.recv_timeout(TEST_TIMEOUT).unwrap();
        });
        (
            Self {
                address,
                request,
                server,
            },
            release,
        )
    }

    fn finish(self, expected_request: &[u8]) {
        assert_eq!(
            self.request.recv_timeout(TEST_TIMEOUT).unwrap(),
            expected_request
        );
        self.server.join().unwrap();
    }
}
