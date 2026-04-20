/// Extract and print all output string regions from a BF source file.
pub fn print_output_regions(source: &[u8]) {
    // Simulate the program and collect dot positions and output bytes.
    let prog = crate::interpreter::Program::compile(source);
    let result = prog.run(b"", false);
    let output = &result.output;
    let dot_map = trace_dot_positions(source, b"");

    // Group consecutive dots into regions (strings).
    let mut regions = Vec::new();
    let mut cur_region = Vec::new();
    let mut cur_start = 0;
    for (i, &(src_pos, _ptr)) in dot_map.iter().enumerate() {
        if cur_region.is_empty() {
            cur_start = src_pos;
            cur_region.push((src_pos, output[i]));
        } else {
            // If this dot is consecutive in source, extend region.
            if src_pos == dot_map[i - 1].0 + 1 {
                cur_region.push((src_pos, output[i]));
            } else {
                // End previous region.
                regions.push((cur_start, cur_region.clone()));
                cur_region.clear();
                cur_start = src_pos;
                cur_region.push((src_pos, output[i]));
            }
        }
    }
    if !cur_region.is_empty() {
        regions.push((cur_start, cur_region));
    }

    println!("Found {} output string regions:", regions.len());
    for (start, region) in regions {
        let end = region.last().unwrap().0;
        let s: String = region.iter().map(|&(_, c)| c as char).collect();
        println!("raw[{}..{}]  {}", start, end + 1, s.escape_default());
    }
}
/// Patch strings in BF source code.
///
/// BF strings are built by arithmetic (+/-) then output with `.`.
/// To patch: simulate the BF program, find where the target string is output,
/// then modify the arithmetic to produce the replacement string instead.
///
/// Constraint: replacement must be <= original length (pad with spaces).

use crate::interpreter::Program;

/// Result of locating a string in BF source.
#[derive(Debug, Clone)]
pub struct StringLocation {
    /// Byte offset of each character's output (`.`) instruction in the raw BF source.
    pub dot_positions: Vec<usize>,
    /// The character values that were output.
    pub chars: Vec<u8>,
    /// Tape pointer position at each output instruction (needed for correct patching
    /// when multiple dots share the same cell).
    pub ptr_positions: Vec<usize>,
}

/// Find where a target string is output in the BF source.
/// Returns the source positions of the `.` instructions that produce each char.
pub fn locate_string(source: &[u8], input: &[u8], target: &str) -> Option<StringLocation> {
    let target_bytes = target.as_bytes();
    if target_bytes.is_empty() {
        return None;
    }

    // Build a map of BF-only chars with source positions.
    let _bf: Vec<(usize, u8)> = source
        .iter()
        .enumerate()
        .filter(|(_, &c)| b"+-<>.,[]".contains(&c))
        .map(|(i, &c)| (i, c))
        .collect();

    // We need to simulate execution and track which `.` instruction produced which
    // output byte, then find the target substring in the output.
    // Use a custom simulation that records (output_byte, src_position_of_dot).

    let prog = Program::compile(source);
    let result = prog.run(input, false);
    let output = &result.output;

    // Now we need to map output bytes to source positions.
    // Re-run with a tracer that records dot positions.
    let dot_map = trace_dot_positions(source, input);

    // Find target in output.
    if let Some(start) = find_subsequence(output, target_bytes) {
        let end = start + target_bytes.len();
        if end <= dot_map.len() {
            return Some(StringLocation {
                dot_positions: dot_map[start..end].iter().map(|&(s, _)| s).collect(),
                chars: output[start..end].to_vec(),
                ptr_positions: dot_map[start..end].iter().map(|&(_, p)| p).collect(),
            });
        }
    }

    None
}

/// Trace which raw source position and tape pointer each output byte came from.
fn trace_dot_positions(source: &[u8], input: &[u8]) -> Vec<(usize, usize)> {
    let bf: Vec<(usize, u8)> = source
        .iter()
        .enumerate()
        .filter(|(_, &c)| b"+-<>.,[]".contains(&c))
        .map(|(i, &c)| (i, c))
        .collect();

    // Simulate directly with the raw BF to track per-output source position
    // and tape pointer (needed for correct patching of shared cells).

    let tape_size = 131_072usize;
    let mut tape = vec![0u8; tape_size];
    let mut ptr = tape_size / 2;
    let mut ip = 0usize;
    let mut input_pos = 0usize;
    let mut dot_positions = Vec::new();

    while ip < bf.len() {
        let (src_pos, ch) = bf[ip];
        match ch {
            b'+' => {
                tape[ptr] = tape[ptr].wrapping_add(1);
                ip += 1;
            }
            b'-' => {
                tape[ptr] = tape[ptr].wrapping_sub(1);
                ip += 1;
            }
            b'>' => {
                ptr += 1;
                ip += 1;
            }
            b'<' => {
                if ptr > 0 { ptr -= 1; }
                ip += 1;
            }
            b'.' => {
                dot_positions.push((src_pos, ptr));
                ip += 1;
            }
            b',' => {
                if input_pos < input.len() {
                    tape[ptr] = input[input_pos];
                    input_pos += 1;
                } else {
                    break;
                }
                ip += 1;
            }
            b'[' => {
                if tape[ptr] == 0 {
                    // Find matching ]
                    let mut depth = 1;
                    ip += 1;
                    while ip < bf.len() && depth > 0 {
                        if bf[ip].1 == b'[' { depth += 1; }
                        if bf[ip].1 == b']' { depth -= 1; }
                        if depth > 0 { ip += 1; }
                    }
                    ip += 1;
                } else {
                    ip += 1;
                }
            }
            b']' => {
                if tape[ptr] != 0 {
                    // Find matching [
                    let mut depth = 1;
                    if ip == 0 { break; }
                    ip -= 1;
                    while depth > 0 {
                        if bf[ip].1 == b']' { depth += 1; }
                        if bf[ip].1 == b'[' { depth -= 1; }
                        if depth > 0 {
                            if ip == 0 { break; }
                            ip -= 1;
                        }
                    }
                    ip += 1;
                } else {
                    ip += 1;
                }
            }
            _ => { ip += 1; }
        }
    }

    dot_positions
}

/// Patch the BF source so that a located string outputs `replacement` instead.
///
/// For each character position, we insert `+` or `-` instructions right before
/// the `.` to adjust the cell value from old_char to new_char.
///
/// The replacement must be `<= original.len()`. Shorter strings are padded with spaces.
pub fn patch_string(
    source: &mut Vec<u8>,
    location: &StringLocation,
    replacement: &str,
) -> Result<(), String> {
    let rep_bytes = replacement.as_bytes();
    if rep_bytes.len() > location.chars.len() {
        return Err(format!(
            "Replacement ({} chars) longer than original ({} chars)",
            rep_bytes.len(),
            location.chars.len()
        ));
    }

    // Pad with spaces.
    let mut new_chars: Vec<u8> = rep_bytes.to_vec();
    while new_chars.len() < location.chars.len() {
        new_chars.push(b' ');
    }

    // BF string builders reuse tape cells across multiple '.' outputs.
    // Inserting +/- before one '.' shifts that cell permanently, affecting all
    // subsequent outputs from the same cell. We track cumulative adjustments
    // per tape cell so each patch compensates for prior shifts.
    let mut patches: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut cumulative: std::collections::HashMap<usize, i32> = std::collections::HashMap::new();
    let mut last_dot_per_cell: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (i, &dot_pos) in location.dot_positions.iter().enumerate() {
        let old_val = location.chars[i] as i32;
        let new_val = new_chars[i] as i32;
        let cell = location.ptr_positions[i];
        let cum = *cumulative.get(&cell).unwrap_or(&0);
        let adjustment = new_val - old_val - cum;
        if adjustment != 0 {
            let adj_bytes: Vec<u8> = if adjustment > 0 {
                vec![b'+'; adjustment as usize]
            } else {
                vec![b'-'; (-adjustment) as usize]
            };
            patches.push((dot_pos, adj_bytes));
        }
        *cumulative.entry(cell).or_insert(0) += adjustment;
        last_dot_per_cell.insert(cell, dot_pos);
    }

    // Insert reversal adjustments AFTER the last target dot on each cell.
    // This restores the cell to its original value trajectory so that
    // subsequent outputs on the same cell (outside our target) are not corrupted.
    for (&cell, &cum) in &cumulative {
        if cum != 0 {
            if let Some(&last_dot) = last_dot_per_cell.get(&cell) {
                let reversal: Vec<u8> = if cum > 0 {
                    vec![b'-'; cum as usize]
                } else {
                    vec![b'+'; (-cum) as usize]
                };
                // Insert right after the '.' instruction
                patches.push((last_dot + 1, reversal));
            }
        }
    }

    // Apply patches in reverse source order.
    patches.sort_by(|a, b| b.0.cmp(&a.0));
    for (pos, adj) in patches {
        source.splice(pos..pos, adj);
    }

    Ok(())
}

/// Raw patch: replace bytes at a given position.
pub fn patch_raw(source: &mut Vec<u8>, offset: usize, replacement: &[u8]) -> Result<(), String> {
    if offset + replacement.len() > source.len() {
        return Err("Patch extends beyond source length".to_string());
    }
    source[offset..offset + replacement.len()].copy_from_slice(replacement);
    Ok(())
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_finds_string() {
        // Program that outputs "Hi"
        let src = b"+++++++++[>++++++++<-]>."    // 'H' = 72
                  as &[u8];
        // This only outputs 'H'. Let's test with a simpler one.
        let src2 = b"++++++++++[>+++++++>++++++++++>+++>+<<<<-]>++.>+.+++++++..+++.";
        let prog = Program::compile(src2);
        let result = prog.run(b"", false);
        let output = String::from_utf8_lossy(&result.output);
        // Just verify the locate function works for whatever it outputs.
        if output.len() >= 2 {
            let target = &output[..2];
            let loc = locate_string(src2, b"", target);
            assert!(loc.is_some(), "Should find the first 2 chars");
            let loc = loc.unwrap();
            assert_eq!(loc.chars.len(), 2);
        }
    }
}
