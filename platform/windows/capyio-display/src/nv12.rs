use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum BgraToNv12Error {
    #[error("NV12 conversion requires positive even dimensions")]
    InvalidDimensions,
    #[error("BGRA payload length does not match width times height times four")]
    InvalidPayload,
    #[error("NV12 output size overflowed")]
    SizeOverflow,
}

/// Converts packed BGRA8/sRGB into bounded BT.709 limited-range NV12.
pub fn bgra8_to_nv12(
    bgra: &[u8],
    width: u32,
    height: u32,
    output: &mut Vec<u8>,
) -> Result<(), BgraToNv12Error> {
    if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(BgraToNv12Error::InvalidDimensions);
    }
    let width_usize = usize::try_from(width).map_err(|_| BgraToNv12Error::SizeOverflow)?;
    let height_usize = usize::try_from(height).map_err(|_| BgraToNv12Error::SizeOverflow)?;
    let pixels = width_usize
        .checked_mul(height_usize)
        .ok_or(BgraToNv12Error::SizeOverflow)?;
    let expected_bgra = pixels.checked_mul(4).ok_or(BgraToNv12Error::SizeOverflow)?;
    if bgra.len() != expected_bgra {
        return Err(BgraToNv12Error::InvalidPayload);
    }
    let nv12_len = pixels
        .checked_add(pixels / 2)
        .ok_or(BgraToNv12Error::SizeOverflow)?;
    output.clear();
    output.resize(nv12_len, 0);

    for row in 0..height_usize {
        for column in 0..width_usize {
            let source = (row * width_usize + column) * 4;
            let b = i32::from(bgra[source]);
            let g = i32::from(bgra[source + 1]);
            let r = i32::from(bgra[source + 2]);
            output[row * width_usize + column] =
                clamp_u8(16 + ((47 * r + 157 * g + 16 * b + 128) >> 8));
        }
    }

    let uv_start = pixels;
    for row in (0..height_usize).step_by(2) {
        for column in (0..width_usize).step_by(2) {
            let mut r = 0_i32;
            let mut g = 0_i32;
            let mut b = 0_i32;
            for y in 0..2 {
                for x in 0..2 {
                    let source = ((row + y) * width_usize + column + x) * 4;
                    b += i32::from(bgra[source]);
                    g += i32::from(bgra[source + 1]);
                    r += i32::from(bgra[source + 2]);
                }
            }
            r = (r + 2) / 4;
            g = (g + 2) / 4;
            b = (b + 2) / 4;
            let uv = uv_start + (row / 2) * width_usize + column;
            output[uv] = clamp_u8(128 + ((-26 * r - 87 * g + 113 * b + 128) >> 8));
            output[uv + 1] = clamp_u8(128 + ((112 * r - 102 * g - 10 * b + 128) >> 8));
        }
    }
    Ok(())
}

fn clamp_u8(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_and_white_map_to_limited_range() {
        let mut output = Vec::new();
        bgra8_to_nv12(&[0; 16], 2, 2, &mut output).unwrap();
        assert_eq!(output, [16, 16, 16, 16, 128, 128]);

        bgra8_to_nv12(&[255; 16], 2, 2, &mut output).unwrap();
        assert_eq!(output, [235, 235, 235, 235, 128, 128]);
    }

    #[test]
    fn red_block_has_bt709_luma_and_chroma() {
        let red = [0_u8, 0, 255, 255].repeat(4);
        let mut output = Vec::new();
        bgra8_to_nv12(&red, 2, 2, &mut output).unwrap();
        assert_eq!(output, [63, 63, 63, 63, 102, 240]);
    }

    #[test]
    fn rejects_odd_dimensions_and_mismatched_payloads() {
        let mut output = Vec::new();
        assert_eq!(
            bgra8_to_nv12(&[], 3, 2, &mut output),
            Err(BgraToNv12Error::InvalidDimensions)
        );
        assert_eq!(
            bgra8_to_nv12(&[], 2, 2, &mut output),
            Err(BgraToNv12Error::InvalidPayload)
        );
    }
}
