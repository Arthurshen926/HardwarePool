use capyio_windows_display::{
    IDD_HANDOFF_ABI_VERSION, IDD_HANDOFF_HEADER_BYTES, IDD_HANDOFF_MAGIC, IddFrameHeader,
    IddHandoffError,
};

fn mapping(width: u32, height: u32, stride: u32) -> Vec<u8> {
    let payload = u64::from(stride) * u64::from(height);
    let mut bytes = vec![0; IDD_HANDOFF_HEADER_BYTES + usize::try_from(payload).unwrap()];
    bytes[0..4].copy_from_slice(&IDD_HANDOFF_MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&IDD_HANDOFF_ABI_VERSION.to_le_bytes());
    bytes[6..8].copy_from_slice(&(IDD_HANDOFF_HEADER_BYTES as u16).to_le_bytes());
    bytes[8..16].copy_from_slice(&1_u64.to_le_bytes());
    bytes[16..24].copy_from_slice(&2_u64.to_le_bytes());
    bytes[24..28].copy_from_slice(&width.to_le_bytes());
    bytes[28..32].copy_from_slice(&height.to_le_bytes());
    bytes[32..36].copy_from_slice(&stride.to_le_bytes());
    bytes[36..40].copy_from_slice(&1_u32.to_le_bytes());
    bytes[40..48].copy_from_slice(&payload.to_le_bytes());
    bytes
}

#[test]
fn accepts_a_stable_bounded_bgra_snapshot() {
    let bytes = mapping(1280, 720, 1280 * 4);
    let header = IddFrameHeader::parse(&bytes).unwrap();
    assert_eq!(header.width, 1280);
    assert_eq!(header.payload_range().len(), 1280 * 720 * 4);
}

#[test]
fn rejects_an_odd_sequence_lock() {
    let mut bytes = mapping(64, 36, 64 * 4);
    bytes[16..24].copy_from_slice(&3_u64.to_le_bytes());
    assert_eq!(
        IddFrameHeader::parse(&bytes),
        Err(IddHandoffError::SnapshotInProgress)
    );
}

#[test]
fn rejects_payload_beyond_the_mapping() {
    let mut bytes = mapping(64, 36, 64 * 4);
    bytes.truncate(IDD_HANDOFF_HEADER_BYTES);
    assert_eq!(
        IddFrameHeader::parse(&bytes),
        Err(IddHandoffError::CapacityExceeded)
    );
}

#[test]
fn rejects_an_unknown_pixel_format() {
    let mut bytes = mapping(64, 36, 64 * 4);
    bytes[36..40].copy_from_slice(&99_u32.to_le_bytes());
    assert_eq!(
        IddFrameHeader::parse(&bytes),
        Err(IddHandoffError::UnsupportedPixelFormat)
    );
}
