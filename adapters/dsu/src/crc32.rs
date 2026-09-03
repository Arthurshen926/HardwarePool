/// Computes the reflected CRC-32/ISO-HDLC checksum used by DSU v1001.
///
/// DSU packet callers must zero header bytes 8 through 11 before invoking this
/// function. This table-free implementation has constant auxiliary space and
/// adds no production dependency.
#[must_use]
pub fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let polynomial = 0xedb8_8320 & 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ polynomial;
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::crc32_ieee;

    #[test]
    fn matches_standard_check_value() {
        assert_eq!(crc32_ieee(b"123456789"), 0xcbf4_3926);
    }
}
