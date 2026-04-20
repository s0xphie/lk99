/// TernLSB steganographic encoding/decoding.
///
/// Encodes a BF program (and optional save data) into the least significant
/// bits of PNG image pixels using mod-9 ternary values.
///
/// Encoding scheme:
///   - Each pixel byte mod 9 maps to: 0-7 = BF instructions +-<>.,[] ; 8 = terminator/marker
///   - Save data follows the program terminator: 4 marker bytes (%9==8),
///     then 3 pixel bytes per save character (base-9 encoded), then NUL terminator.

use std::path::Path;

const BF_CHARS: [u8; 8] = [b'+', b'-', b',', b'.', b'<', b'>', b'[', b']'];

fn bf_to_mod(c: u8) -> Option<u8> {
    BF_CHARS.iter().position(|&x| x == c).map(|i| i as u8)
}

fn mod_to_bf(m: u8) -> Option<u8> {
    if (m as usize) < BF_CHARS.len() {
        Some(BF_CHARS[m as usize])
    } else {
        None
    }
}

/// Adjust a pixel byte so that `byte % 9 == target_mod`, with minimal change.
fn adjust_byte(byte: u8, target_mod: u8) -> u8 {
    let current = byte % 9;
    if current == target_mod {
        return byte;
    }
    let diff_up = (target_mod as i16 - current as i16 + 9) % 9;
    let diff_down = (current as i16 - target_mod as i16 + 9) % 9;
    if diff_up <= diff_down {
        let new_val = byte as i16 + diff_up;
        if new_val <= 255 { new_val as u8 } else { (byte as i16 - diff_down) as u8 }
    } else {
        let new_val = byte as i16 - diff_down;
        if new_val >= 0 { new_val as u8 } else { (byte as i16 + diff_up) as u8 }
    }
}

/// Encode a BF program and optional save text into a PNG image.
pub fn encode(
    source_png: &Path,
    bf_source: &[u8],
    save_text: Option<&str>,
    output_png: &Path,
) -> Result<(), String> {
    // Read source PNG.
    let file = std::fs::File::open(source_png)
        .map_err(|e| format!("Cannot open source PNG: {}", e))?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info()
        .map_err(|e| format!("PNG decode error: {}", e))?;

    let info = reader.info().clone();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf)
        .map_err(|e| format!("PNG frame error: {}", e))?;
    let mut pixels = buf[..frame.buffer_size()].to_vec();

    // Build the mod-9 sequence to encode.
    let mut mods: Vec<u8> = Vec::new();

    // BF program.
    for &c in bf_source {
        if let Some(m) = bf_to_mod(c) {
            mods.push(m);
        }
        // Skip non-BF chars.
    }
    // Program terminator.
    mods.push(8);

    // Save data (if any).
    if let Some(save) = save_text {
        // 4 marker bytes (mod 8).
        for _ in 0..4 {
            mods.push(8);
        }
        // Base-9 encode each character: 3 bytes per char (little-endian: low digit first).
        for &c in save.as_bytes() {
            let val = c as u32;
            mods.push((val % 9) as u8);         // digit 0 (low)
            mods.push(((val / 9) % 9) as u8);   // digit 1 (mid)
            mods.push((val / 81) as u8);         // digit 2 (high)
        }
        // NUL terminator (3 bytes of 0).
        mods.push(0);
        mods.push(0);
        mods.push(0);
    }

    if mods.len() > pixels.len() {
        return Err(format!(
            "Image too small: need {} pixels but image has {}",
            mods.len(),
            pixels.len()
        ));
    }

    // Apply encoding.
    for (i, &target_mod) in mods.iter().enumerate() {
        pixels[i] = adjust_byte(pixels[i], target_mod);
    }

    // Write output PNG.
    let out_file = std::fs::File::create(output_png)
        .map_err(|e| format!("Cannot create output PNG: {}", e))?;
    let mut encoder = png::Encoder::new(out_file, info.width, info.height);
    encoder.set_color(info.color_type);
    encoder.set_depth(info.bit_depth);
    let mut writer = encoder.write_header()
        .map_err(|e| format!("PNG write header error: {}", e))?;
    writer.write_image_data(&pixels)
        .map_err(|e| format!("PNG write data error: {}", e))?;

    Ok(())
}

/// Decode a BF program and optional save text from a TernLSB PNG.
pub fn decode(encoded_png: &Path) -> Result<(Vec<u8>, Option<String>), String> {
    let file = std::fs::File::open(encoded_png)
        .map_err(|e| format!("Cannot open PNG: {}", e))?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info()
        .map_err(|e| format!("PNG decode error: {}", e))?;

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf)
        .map_err(|e| format!("PNG frame error: {}", e))?;
    let pixels = &buf[..frame.buffer_size()];

    // Decode BF program.
    let mut bf_source = Vec::new();
    let mut i = 0;

    while i < pixels.len() {
        let m = pixels[i] % 9;
        if m == 8 {
            break; // Program terminator.
        }
        if let Some(c) = mod_to_bf(m) {
            bf_source.push(c);
        }
        i += 1;
    }

    // Check for save data after terminator.
    let save_text = decode_save_data(pixels, i);

    Ok((bf_source, save_text))
}

fn decode_save_data(pixels: &[u8], term_pos: usize) -> Option<String> {
    let mut i = term_pos + 1; // Skip terminator.

    // Check for 4 marker bytes (%9 == 8).
    let mut markers = 0;
    while i < pixels.len() && pixels[i] % 9 == 8 {
        markers += 1;
        i += 1;
    }
    if markers < 4 {
        return None;
    }

    // Decode base-9 characters (3 bytes each, little-endian: low digit first).
    let mut save = Vec::new();
    while i + 2 < pixels.len() {
        let d0 = (pixels[i] % 9) as u32;     // low
        let d1 = (pixels[i + 1] % 9) as u32; // mid
        let d2 = (pixels[i + 2] % 9) as u32; // high
        let val = d0 + d1 * 9 + d2 * 81;
        if val == 0 {
            break; // NUL terminator.
        }
        save.push(val as u8);
        i += 3;
    }

    if save.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&save).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjust_byte_roundtrip() {
        for byte in 0..=255u8 {
            for target in 0..9u8 {
                let adjusted = adjust_byte(byte, target);
                assert_eq!(adjusted % 9, target, "byte={byte} target={target}");
                let diff = (adjusted as i16 - byte as i16).unsigned_abs();
                assert!(diff <= 4, "byte={byte} target={target} diff={diff}");
            }
        }
    }

    #[test]
    fn bf_mod_roundtrip() {
        for &c in &BF_CHARS {
            let m = bf_to_mod(c).unwrap();
            let back = mod_to_bf(m).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn save_roundtrip() {
        // Verify encode/decode of save data matches ternlsb.py byte order.
        let test = "Hello\nWorld\n";
        let mut mods: Vec<u8> = Vec::new();
        // Simulate encoding
        mods.push(8); // terminator
        for _ in 0..4 { mods.push(8); } // marker
        for &c in test.as_bytes() {
            let val = c as u32;
            mods.push((val % 9) as u8);
            mods.push(((val / 9) % 9) as u8);
            mods.push((val / 81) as u8);
        }
        mods.push(0); mods.push(0); mods.push(0); // NUL

        // Build fake pixel data
        let pixels: Vec<u8> = mods.iter().map(|&m| m).collect();
        let decoded = decode_save_data(&pixels, 0);
        assert_eq!(decoded, Some(test.to_string()));
    }
}

// =====================================================================
// bfsteg format support (original encoding used by lk.png)
// =====================================================================
//
// bfsteg stores BF instructions in the low 3 bits of pixel bytes.
// Instruction mapping: 0=+  1=-  2=>  3=<  4=.  5=,  6=[  7=]
// Program length is a 32-bit integer encoded in the last few pixel bytes
// (2 bits per channel, read from the bottom-right corner).

const BFSTEG_CHARS: [u8; 8] = [b'+', b'-', b'>', b'<', b'.', b',', b'[', b']'];

/// Decode a BF program from a bfsteg-format PNG (low 3 bits).
pub fn bfsteg_decode(encoded_png: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(encoded_png)
        .map_err(|e| format!("Cannot open PNG: {}", e))?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info()
        .map_err(|e| format!("PNG decode error: {}", e))?;

    let info = reader.info().clone();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf)
        .map_err(|e| format!("PNG frame error: {}", e))?;
    let pixels = &buf[..frame.buffer_size()];

    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(format!("Unsupported color type: {:?}", other)),
    };

    // Read program length from last pixels (2 bits per channel, 32 bits total).
    let width = info.width as usize;
    let height = info.height as usize;
    let row_bytes = width * channels;
    let mut program_size: u32 = 0;
    let mut bits_read: u32 = 0;
    let mut px = width - 1;
    let mut py = height - 1;

    while bits_read < 32 {
        let base = py * row_bytes + px * channels;
        for ch in 0..channels {
            if bits_read >= 32 { break; }
            program_size |= ((pixels[base + ch] & 3) as u32) << bits_read;
            bits_read += 2;
        }
        if px == 0 {
            px = width - 1;
            py = py.saturating_sub(1);
        } else {
            px -= 1;
        }
    }

    if program_size as usize > pixels.len() {
        return Err(format!(
            "Invalid program size {} (image has {} bytes)",
            program_size, pixels.len()
        ));
    }

    // Decode BF instructions from low 3 bits.
    let mut bf_source = Vec::with_capacity(program_size as usize);
    let mut byte_idx = 0;
    let mut count = 0u32;
    while count < program_size && byte_idx < pixels.len() {
        let val = (pixels[byte_idx] & 7) as usize;
        bf_source.push(BFSTEG_CHARS[val]);
        byte_idx += 1;
        count += 1;
    }

    Ok(bf_source)
}

/// Encode a BF program into a bfsteg-format PNG (low 3 bits).
pub fn bfsteg_encode(
    source_png: &Path,
    bf_source: &[u8],
    output_png: &Path,
) -> Result<(), String> {
    let file = std::fs::File::open(source_png)
        .map_err(|e| format!("Cannot open source PNG: {}", e))?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info()
        .map_err(|e| format!("PNG decode error: {}", e))?;

    let info = reader.info().clone();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf)
        .map_err(|e| format!("PNG frame error: {}", e))?;
    let mut pixels = buf[..frame.buffer_size()].to_vec();

    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(format!("Unsupported color type: {:?}", other)),
    };

    // Filter to BF-only chars and map to bfsteg indices.
    let bf_indices: Vec<u8> = bf_source
        .iter()
        .filter_map(|&c| BFSTEG_CHARS.iter().position(|&x| x == c).map(|i| i as u8))
        .collect();

    let width = info.width as usize;
    let height = info.height as usize;
    let row_bytes = width * channels;
    let reserved = ((32 + channels * 2 - 1) / (channels * 2)) * channels;

    if bf_indices.len() + reserved > pixels.len() {
        return Err(format!(
            "Image too small: need {} + {} bytes, have {}",
            bf_indices.len(), reserved, pixels.len()
        ));
    }

    // Encode BF instructions into low 3 bits.
    for (i, &idx) in bf_indices.iter().enumerate() {
        pixels[i] = (pixels[i] & 0xf8) | idx;
    }

    // Encode program length into last pixels (2 bits per channel).
    let mut code_length = bf_indices.len() as u32;
    let mut px = width - 1;
    let mut py = height - 1;
    let mut bits_written: u32 = 0;

    while bits_written < 32 {
        let base = py * row_bytes + px * channels;
        for ch in 0..channels {
            if bits_written >= 32 { break; }
            pixels[base + ch] = (pixels[base + ch] & 0xfc) | (code_length & 3) as u8;
            code_length >>= 2;
            bits_written += 2;
        }
        if px == 0 {
            px = width - 1;
            py = py.saturating_sub(1);
        } else {
            px -= 1;
        }
    }

    // Write output PNG.
    let out_file = std::fs::File::create(output_png)
        .map_err(|e| format!("Cannot create output PNG: {}", e))?;
    let mut encoder = png::Encoder::new(out_file, info.width, info.height);
    encoder.set_color(info.color_type);
    encoder.set_depth(info.bit_depth);
    let mut writer = encoder.write_header()
        .map_err(|e| format!("PNG write header error: {}", e))?;
    writer.write_image_data(&pixels)
        .map_err(|e| format!("PNG write data error: {}", e))?;

    Ok(())
}

/// Auto-detect whether a PNG is bfsteg or ternlsb format and decode it.
pub fn auto_decode(encoded_png: &Path) -> Result<(Vec<u8>, Option<String>, &'static str), String> {
    // Try ternlsb first (check if first terminator makes sense).
    match decode(encoded_png) {
        Ok((bf, save)) if bf.len() > 100 => return Ok((bf, save, "ternlsb")),
        _ => {}
    }
    // Fall back to bfsteg.
    let bf = bfsteg_decode(encoded_png)?;
    if bf.is_empty() {
        return Err("Could not decode PNG as either ternlsb or bfsteg format".to_string());
    }
    Ok((bf, None, "bfsteg"))
}

/// Read raw pixel bytes and image info from a PNG file.
fn read_png_pixels(path: &Path) -> Result<(Vec<u8>, png::OutputInfo, png::ColorType, png::BitDepth), String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Cannot open PNG: {}", e))?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info()
        .map_err(|e| format!("PNG decode error: {}", e))?;
    let info = reader.info().clone();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf)
        .map_err(|e| format!("PNG frame error: {}", e))?;
    let pixels = buf[..frame.buffer_size()].to_vec();
    Ok((pixels, frame, info.color_type, info.bit_depth))
}

/// Write raw pixel bytes to a PNG file.
fn write_png(
    path: &Path,
    pixels: &[u8],
    width: u32,
    height: u32,
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
) -> Result<(), String> {
    let out_file = std::fs::File::create(path)
        .map_err(|e| format!("Cannot create PNG: {}", e))?;
    let mut encoder = png::Encoder::new(out_file, width, height);
    encoder.set_color(color_type);
    encoder.set_depth(bit_depth);
    let mut writer = encoder.write_header()
        .map_err(|e| format!("PNG write header error: {}", e))?;
    writer.write_image_data(pixels)
        .map_err(|e| format!("PNG write data error: {}", e))?;
    Ok(())
}

/// Build the mod-9 value sequence for a BF source (ternlsb encoding).
fn bf_to_mods(bf_source: &[u8]) -> Vec<u8> {
    let mut mods: Vec<u8> = Vec::new();
    for &c in bf_source {
        if let Some(m) = bf_to_mod(c) {
            mods.push(m);
        }
    }
    mods.push(8); // terminator
    mods
}

/// Encode mod-9 values into a pixel array (mutates in place).
fn apply_mods(pixels: &mut [u8], mods: &[u8]) {
    for (i, &target_mod) in mods.iter().enumerate() {
        if i < pixels.len() {
            pixels[i] = adjust_byte(pixels[i], target_mod);
        }
    }
}

/// Databend: XOR two ternlsb encodings of different BF sources into the
/// same carrier image.
///
/// The output is `encode(carrier, original) XOR encode(carrier, modded)`.
///
/// Properties:
/// - Deterministic: same inputs → same output, always.
/// - Zero (black) at every byte where the two encodings agree.
/// - Non-zero exactly where the mod changes the encoded pixel value.
/// - Insertion patches shift all downstream BF→pixel mappings, creating
///   a visual avalanche/tear from each patch point to end-of-program.
///
/// `mode` controls the output:
/// - `"xor"` — raw XOR (mostly black with bright diff regions)
/// - `"blend"` — carrier image with XOR difference composited into it
/// - `"amplify"` — XOR values amplified into visible range
pub fn databend(
    source_png: &Path,
    original_bf: &[u8],
    modded_bf: &[u8],
    output_png: &Path,
    mode: &str,
) -> Result<DatabendStats, String> {
    let (carrier, frame, color_type, bit_depth) = read_png_pixels(source_png)?;

    // Encode both into copies of the carrier.
    let mut enc_orig = carrier.clone();
    let mut enc_mod = carrier.clone();

    let mods_orig = bf_to_mods(original_bf);
    let mods_mod = bf_to_mods(modded_bf);

    apply_mods(&mut enc_orig, &mods_orig);
    apply_mods(&mut enc_mod, &mods_mod);

    // XOR.
    let xor: Vec<u8> = enc_orig.iter()
        .zip(enc_mod.iter())
        .map(|(&a, &b)| a ^ b)
        .collect();

    // Stats.
    let nonzero_count = xor.iter().filter(|&&b| b != 0).count();
    let total = xor.len();

    // Find first and last nonzero byte.
    let first_diff = xor.iter().position(|&b| b != 0);
    let last_diff = xor.iter().rposition(|&b| b != 0);

    let output: Vec<u8> = match mode {
        "xor" => xor.clone(),
        "amplify" => {
            // Map each XOR byte: 0 stays 0, nonzero gets amplified.
            // Since ternlsb only adjusts ±4 per byte, XOR values are small (0-15 typical).
            // Amplify: multiply by 17 (maps 0-15 → 0-255 nicely).
            xor.iter().map(|&b| {
                if b == 0 { 0 } else { b.saturating_mul(17) }
            }).collect()
        }
        "blend" => {
            // Composite: carrier where XOR=0, XOR-tinted where different.
            // Non-zero XOR pixels get their R boosted and G/B dimmed.
            let channels = match color_type {
                png::ColorType::Rgba => 4,
                png::ColorType::Rgb => 3,
                _ => 3,
            };
            let mut out = carrier.clone();
            for i in 0..total {
                if xor[i] != 0 {
                    let pixel_base = (i / channels) * channels;
                    // Tint: boost the XOR magnitude into the red channel,
                    // desaturate green/blue.
                    let mag = (xor[i] as u16).saturating_mul(17).min(255) as u8;
                    if pixel_base + 2 < total {
                        out[pixel_base] = out[pixel_base].saturating_add(mag);      // R
                        out[pixel_base + 1] = out[pixel_base + 1].saturating_sub(mag / 2); // G
                        out[pixel_base + 2] = out[pixel_base + 2].saturating_sub(mag / 2); // B
                    }
                }
            }
            out
        }
        other => return Err(format!("Unknown mode: '{}'. Use xor, amplify, or blend.", other)),
    };

    write_png(output_png, &output, frame.width, frame.height, color_type, bit_depth)?;

    Ok(DatabendStats {
        total_bytes: total,
        diff_bytes: nonzero_count,
        first_diff_byte: first_diff,
        last_diff_byte: last_diff,
        original_bf_len: mods_orig.len(),
        modded_bf_len: mods_mod.len(),
        insertion_delta: mods_mod.len() as i64 - mods_orig.len() as i64,
    })
}

/// Statistics from a databend operation.
pub struct DatabendStats {
    pub total_bytes: usize,
    pub diff_bytes: usize,
    pub first_diff_byte: Option<usize>,
    pub last_diff_byte: Option<usize>,
    pub original_bf_len: usize,
    pub modded_bf_len: usize,
    pub insertion_delta: i64,
}

/// Apply a databend XOR patch: `original_encoded XOR xor_patch = modded_encoded`.
///
/// Takes the original encoded PNG and a raw XOR databend PNG, XORs their pixel
/// arrays, and writes the result — which is the fully playable modded PNG.
/// The modded PNG can then be decoded normally to extract the modded BF source.
pub fn apply_databend(
    original_encoded_png: &Path,
    xor_patch_png: &Path,
    output_png: &Path,
) -> Result<usize, String> {
    let (orig_pixels, frame, color_type, bit_depth) = read_png_pixels(original_encoded_png)?;
    let (xor_pixels, _, _, _) = read_png_pixels(xor_patch_png)?;

    if orig_pixels.len() != xor_pixels.len() {
        return Err(format!(
            "Pixel array size mismatch: original={} xor_patch={}",
            orig_pixels.len(), xor_pixels.len()
        ));
    }

    let diff_count = xor_pixels.iter().filter(|&&b| b != 0).count();

    let result: Vec<u8> = orig_pixels.iter()
        .zip(xor_pixels.iter())
        .map(|(&a, &b)| a ^ b)
        .collect();

    write_png(output_png, &result, frame.width, frame.height, color_type, bit_depth)?;

    Ok(diff_count)
}
