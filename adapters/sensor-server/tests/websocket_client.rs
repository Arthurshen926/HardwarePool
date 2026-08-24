use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

use capyio_sensor_server_adapter::{
    MAX_CONNECTION_TIMEOUT, MAX_SENSOR_SERVER_MESSAGE_BYTES, SensorKind, SensorServerClientState,
    SensorServerConnectionConfig, SensorServerControlFrame, SensorServerEndpoint,
    SensorServerError, SensorServerReadOutcome, SensorServerWebSocketClient,
};
use tungstenite::{
    Message, WebSocket, accept,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

const ACCELEROMETER: &str = include_str!("../../../fixtures/sensor-server/accelerometer.json");

fn config(io_timeout: Duration) -> SensorServerConnectionConfig {
    SensorServerConnectionConfig::new(Duration::from_secs(2), io_timeout).unwrap()
}

fn spawn_mock_server(
    handler: impl FnOnce(WebSocket<TcpStream>) + Send + 'static,
) -> (SensorServerEndpoint, JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        handler(accept(stream).unwrap());
    });
    (
        SensorServerEndpoint::new(address.ip(), address.port()).unwrap(),
        handle,
    )
}

#[test]
fn endpoint_is_ip_literal_fixed_path_and_bounded_configuration() {
    let ipv4 =
        SensorServerEndpoint::new(IpAddr::V4(Ipv4Addr::new(100, 66, 157, 119)), 8080).unwrap();
    assert_eq!(
        ipv4.url_for(SensorKind::Accelerometer),
        "ws://100.66.157.119:8080/sensor/connect?type=android.sensor.accelerometer"
    );
    let ipv6 = SensorServerEndpoint::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 8080).unwrap();
    assert_eq!(
        ipv6.url_for(SensorKind::Gyroscope),
        "ws://[::1]:8080/sensor/connect?type=android.sensor.gyroscope"
    );
    assert_eq!(
        SensorServerEndpoint::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        Err(SensorServerError::InvalidPort)
    );
    assert_eq!(
        SensorServerConnectionConfig::new(Duration::ZERO, Duration::from_secs(1)),
        Err(SensorServerError::InvalidConnectionTimeout)
    );
    assert_eq!(
        SensorServerConnectionConfig::new(
            Duration::from_secs(1),
            MAX_CONNECTION_TIMEOUT + Duration::from_nanos(1)
        ),
        Err(SensorServerError::InvalidConnectionTimeout)
    );
}

#[test]
fn valid_text_frame_yields_exact_validated_reading() {
    let (endpoint, server) = spawn_mock_server(|mut socket| {
        socket.send(Message::Text(ACCELEROMETER.into())).unwrap();
    });
    let mut client = SensorServerWebSocketClient::connect(
        endpoint,
        SensorKind::Accelerometer,
        config(Duration::from_secs(2)),
    )
    .unwrap();
    let SensorServerReadOutcome::Reading(reading) = client.read().unwrap() else {
        panic!("expected a validated reading")
    };
    assert_eq!(reading.timestamp_nanos, 3_925_657_519_043_709);
    assert_eq!(reading.values, [0.31892395, -0.97802734, 10.049896]);
    server.join().unwrap();
}

#[test]
fn ping_is_serviced_and_pong_is_observed_by_server() {
    let (endpoint, server) = spawn_mock_server(|mut socket| {
        socket.send(Message::Ping(vec![1, 2, 3].into())).unwrap();
        assert_eq!(socket.read().unwrap(), Message::Pong(vec![1, 2, 3].into()));
    });
    let mut client = SensorServerWebSocketClient::connect(
        endpoint,
        SensorKind::Accelerometer,
        config(Duration::from_secs(2)),
    )
    .unwrap();
    assert_eq!(
        client.read().unwrap(),
        SensorServerReadOutcome::ControlHandled(SensorServerControlFrame::Ping)
    );
    server.join().unwrap();
}

#[test]
fn close_code_is_explicit_and_reply_is_flushed() {
    let (endpoint, server) = spawn_mock_server(|mut socket| {
        socket
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: "done".into(),
            })))
            .unwrap();
        assert!(matches!(socket.read(), Ok(Message::Close(_))));
        assert!(matches!(
            socket.read(),
            Err(tungstenite::Error::ConnectionClosed)
        ));
    });
    let mut client = SensorServerWebSocketClient::connect(
        endpoint,
        SensorKind::Accelerometer,
        config(Duration::from_secs(2)),
    )
    .unwrap();
    assert_eq!(
        client.read().unwrap(),
        SensorServerReadOutcome::Closed { code: Some(1000) }
    );
    assert_eq!(client.state(), SensorServerClientState::Closed);
    assert_eq!(
        client.read(),
        Err(SensorServerError::ClientNotOpen {
            state: SensorServerClientState::Closed
        })
    );
    server.join().unwrap();
}

#[test]
fn caller_close_is_sent_and_prevents_client_reuse() {
    let (endpoint, server) = spawn_mock_server(|mut socket| {
        assert!(matches!(socket.read(), Ok(Message::Close(None))));
    });
    let mut client = SensorServerWebSocketClient::connect(
        endpoint,
        SensorKind::Accelerometer,
        config(Duration::from_secs(2)),
    )
    .unwrap();
    client.close().unwrap();
    assert_eq!(client.state(), SensorServerClientState::Closed);
    assert_eq!(
        client.close(),
        Err(SensorServerError::ClientNotOpen {
            state: SensorServerClientState::Closed
        })
    );
    server.join().unwrap();
}

#[test]
fn binary_and_oversized_text_are_rejected_before_sensor_mapping() {
    let (binary_endpoint, binary_server) = spawn_mock_server(|mut socket| {
        socket.send(Message::Binary(vec![0, 1, 2].into())).unwrap();
    });
    let mut binary_client = SensorServerWebSocketClient::connect(
        binary_endpoint,
        SensorKind::Accelerometer,
        config(Duration::from_secs(2)),
    )
    .unwrap();
    assert_eq!(
        binary_client.read(),
        Err(SensorServerError::UnsupportedBinaryMessage { actual: 3 })
    );
    binary_server.join().unwrap();

    let (oversized_endpoint, oversized_server) = spawn_mock_server(|mut socket| {
        socket
            .send(Message::Text(
                "x".repeat(MAX_SENSOR_SERVER_MESSAGE_BYTES + 1).into(),
            ))
            .unwrap();
    });
    let mut oversized_client = SensorServerWebSocketClient::connect(
        oversized_endpoint,
        SensorKind::Accelerometer,
        config(Duration::from_secs(2)),
    )
    .unwrap();
    assert_eq!(
        oversized_client.read(),
        Err(SensorServerError::WebSocketCapacityExceeded)
    );
    assert_eq!(oversized_client.state(), SensorServerClientState::Failed);
    assert_eq!(
        oversized_client.read(),
        Err(SensorServerError::ClientNotOpen {
            state: SensorServerClientState::Failed
        })
    );
    oversized_server.join().unwrap();
}

#[test]
fn malformed_text_and_read_timeout_are_distinct() {
    let (malformed_endpoint, malformed_server) = spawn_mock_server(|mut socket| {
        socket.send(Message::Text("not-json".into())).unwrap();
    });
    let mut malformed_client = SensorServerWebSocketClient::connect(
        malformed_endpoint,
        SensorKind::Accelerometer,
        config(Duration::from_secs(2)),
    )
    .unwrap();
    assert!(matches!(
        malformed_client.read(),
        Err(SensorServerError::InvalidJson(_))
    ));
    malformed_server.join().unwrap();

    let (release_sender, release_receiver) = mpsc::channel();
    let (timeout_endpoint, timeout_server) = spawn_mock_server(move |_socket| {
        release_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
    });
    let mut timeout_client = SensorServerWebSocketClient::connect(
        timeout_endpoint,
        SensorKind::Accelerometer,
        config(Duration::from_millis(50)),
    )
    .unwrap();
    assert_eq!(
        timeout_client.read().unwrap(),
        SensorServerReadOutcome::TimedOut
    );
    release_sender.send(()).unwrap();
    timeout_server.join().unwrap();
}

#[test]
fn oversized_handshake_is_rejected_by_tungstenite_attack_limit() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = [0u8; 2048];
        let _ = stream.read(&mut request).unwrap();
        let mut response = b"HTTP/1.1 101 Switching Protocols\r\nX-Fill: ".to_vec();
        response.extend(vec![b'a'; 66 * 1024]);
        response.extend_from_slice(b"\r\n\r\n");
        let _ = stream.write_all(&response);
    });
    let endpoint = SensorServerEndpoint::new(address.ip(), address.port()).unwrap();
    assert!(matches!(
        SensorServerWebSocketClient::connect(
            endpoint,
            SensorKind::Accelerometer,
            config(Duration::from_secs(2)),
        ),
        Err(SensorServerError::HandshakeFailed(_))
    ));
    server.join().unwrap();
}
