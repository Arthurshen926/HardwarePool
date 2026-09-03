use capyio_data_plane::{ImuComponentTimestampsV1, parse_imu_fixture_jsonl};
use capyio_dsu_adapter::{
    AxisPermutation, DSU_PAD_DATA_PACKET_BYTES, DsuMotionMapping, DsuPacketError,
    MotionProjectionError, SignedSourceAxis, SourceAxis, crc32_ieee, encode_neutral_pad_data,
    project_imu_envelope,
};
use capyio_input::{GamepadButton, GamepadButtons, GamepadControls};

const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

fn read_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn assert_near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_001,
        "{actual} != {expected}"
    );
}

#[test]
fn fixed_imu_fixture_projects_to_six_deterministic_pad_packets() {
    let envelopes = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap();
    let mut packets = Vec::new();
    for (packet_number, envelope) in envelopes.iter().enumerate() {
        let motion = project_imu_envelope(envelope, DsuMotionMapping::identity()).unwrap();
        let packet = encode_neutral_pad_data(
            0x0102_0304,
            0,
            u32::try_from(packet_number).unwrap(),
            motion,
            GamepadControls::neutral(),
        )
        .unwrap();
        packets.push(packet);
    }

    assert_eq!(packets.len(), 6);
    let first = &packets[0];
    assert_eq!(first.len(), DSU_PAD_DATA_PACKET_BYTES);
    assert_eq!(&first[..4], b"DSUS");
    assert_eq!(read_u16(first, 4), 1001);
    assert_eq!(read_u16(first, 6), 84);
    assert_eq!(read_u32(first, 12), 0x0102_0304);
    assert_eq!(read_u32(first, 16), 0x10_0002);
    assert_eq!(&first[36..40], &[0, 0, 0, 0]);
    assert_eq!(&first[40..44], &[128, 128, 128, 128]);
    assert!(first[44..68].iter().all(|value| *value == 0));
    assert_eq!(read_u64(first, 68), 1_000_000);
    assert_near(read_f32(first, 76), (0.01_f64 / 9.806_65) as f32);
    assert_near(read_f32(first, 80), (-0.02_f64 / 9.806_65) as f32);
    assert_near(read_f32(first, 84), (9.8_f64 / 9.806_65) as f32);
    assert_near(read_f32(first, 88), 0.001_f64.to_degrees() as f32);
    assert_near(read_f32(first, 92), 0.002_f64.to_degrees() as f32);
    assert_near(read_f32(first, 96), (-0.001_f64).to_degrees() as f32);

    for packet in packets {
        let stored = read_u32(&packet, 8);
        let mut checksum_input = packet;
        checksum_input[8..12].fill(0);
        assert_eq!(stored, crc32_ieee(&checksum_input));
    }
}

#[test]
fn acceleration_component_timestamp_takes_precedence() {
    let mut envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    envelope.payload.component_timestamps = Some(ImuComponentTimestampsV1 {
        acceleration_nanos: 9_876_543_210,
        angular_velocity_nanos: 9_876_999_999,
        magnetic_field_nanos: Some(9_877_000_000),
    });
    let motion = project_imu_envelope(&envelope, DsuMotionMapping::identity()).unwrap();
    assert_eq!(motion.timestamp_micros(), 9_876_543);
}

#[test]
fn explicit_axis_permutation_and_sign_are_applied() {
    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    let acceleration = AxisPermutation::new(
        SignedSourceAxis::positive(SourceAxis::Z),
        SignedSourceAxis::negative(SourceAxis::X),
        SignedSourceAxis::positive(SourceAxis::Y),
    )
    .unwrap();
    let angular_velocity = AxisPermutation::new(
        SignedSourceAxis::negative(SourceAxis::Y),
        SignedSourceAxis::positive(SourceAxis::Z),
        SignedSourceAxis::positive(SourceAxis::X),
    )
    .unwrap();
    let motion = project_imu_envelope(
        &envelope,
        DsuMotionMapping::new(acceleration, angular_velocity),
    )
    .unwrap();

    let acceleration = motion.acceleration_g();
    assert_near(acceleration[0], (9.8_f64 / 9.806_65) as f32);
    assert_near(acceleration[1], (-0.01_f64 / 9.806_65) as f32);
    assert_near(acceleration[2], (-0.02_f64 / 9.806_65) as f32);
    let gyroscope = motion.gyroscope_degrees_per_second();
    assert_near(gyroscope[0], (-0.002_f64).to_degrees() as f32);
    assert_near(gyroscope[1], (-0.001_f64).to_degrees() as f32);
    assert_near(gyroscope[2], 0.001_f64.to_degrees() as f32);
}

#[test]
fn non_neutral_controls_are_not_silently_discarded() {
    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    let motion = project_imu_envelope(&envelope, DsuMotionMapping::identity()).unwrap();
    let controls = GamepadControls {
        buttons: GamepadButtons::empty().with(GamepadButton::South),
        ..GamepadControls::neutral()
    };
    assert_eq!(
        encode_neutral_pad_data(1, 0, 0, motion, controls),
        Err(DsuPacketError::NonNeutralControlsUnsupported)
    );
}

#[test]
fn finite_f64_values_that_overflow_dsu_f32_are_rejected() {
    let mut envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    envelope.payload.acceleration[0] = f64::MAX;
    assert!(matches!(
        project_imu_envelope(&envelope, DsuMotionMapping::identity()),
        Err(MotionProjectionError::DsuFloatOutOfRange)
    ));
}
