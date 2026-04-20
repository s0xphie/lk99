/// Extract human-readable strings from a BF program by simulating its output.
///
/// Two modes:
/// 1. Static: compile + run with given inputs, collect output, split into strings.
/// 2. Scenario: run multiple input scenarios to surface strings from different
///    game states (e.g., late-game text like "evil mage").

use crate::interpreter::Program;
use std::collections::BTreeMap;

/// A located string: the text and where it first appeared in the output.
#[derive(Debug, Clone)]
pub struct GameString {
    pub text: String,
    pub output_offset: usize,
    pub scenario: String,
}

/// Extract strings from a single run.
pub fn extract_from_run(prog: &Program, input: &[u8], scenario_name: &str) -> Vec<GameString> {
    let result = prog.run(input, false);
    split_output_to_strings(&result.output, scenario_name)
}

/// Extract strings from multiple scenarios, deduplicating.
pub fn extract_multi(prog: &Program, scenarios: &[(&str, Vec<u8>)]) -> Vec<GameString> {
    let mut seen = BTreeMap::new();
    for (name, input) in scenarios {
        let strings = extract_from_run(prog, input, name);
        for s in strings {
            seen.entry(s.text.clone()).or_insert(s);
        }
    }
    seen.into_values().collect()
}

/// Default scenarios for Lost Kingdom discovery.
pub fn lk_default_scenarios() -> Vec<(&'static str, Vec<u8>)> {
    let scenarios: &[(&str, &[&str])] = &[
        ("start", &["N", "$", "N"]),
        ("explore_surface", &[
            "N", "T2", "N", "T1", "S", "E", "S", "S", "T3",
            "N", "E", "W", "S", "F1", "?", "!", "$", "N",
        ]),
        ("forest", &[
            "N", "T2", "N", "T1", "S", "E", "S", "S", "T3",
            "N", "E", "W", "S", "F1", "S", "W", "B1", "D2",
            "S", "N", "S", "T0", "S", "W", "N", "W", "N", "W", "F0",
            "T5", "$", "N",
        ]),
        ("catacombs", &[
            "N", "T2", "N", "T1", "S", "E", "S", "S", "T3",
            "N", "E", "W", "S", "F1", "S", "W", "B1", "D2",
            "S", "N", "S", "T0", "S", "W", "N", "W", "N", "W", "F0",
            "T5", "E", "S", "E", "N", "E", "S", "E",
            "H5", "S", "T8", "E", "N", "S", "E", "N", "E",
            "P", "D8", "T6", "$", "N",
        ]),
        ("endgame", &[
            "N", "T2", "N", "T1", "S", "E", "S", "S", "T3",
            "N", "E", "W", "S", "F1", "S", "W", "B1", "D2",
            "S", "N", "S", "T0", "S", "W", "N", "W", "N", "W", "F0",
            "T5", "E", "S", "E", "N", "E", "S", "E",
            "H5", "S", "T8", "E", "N", "S", "E", "N", "E",
            "P", "D8", "T6", "W", "S", "W", "E", "N", "W",
            "H0", "E", "N", "N", "E", "W", "W", "W", "N", "W",
            "S", "S", "S", "N", "E", "W", "S", "S", "T4",
            "N", "N", "W", "N", "N",
            "E", "S", "E", "N", "E", "S", "E",
            "S", "E", "N", "S", "E", "N", "E",
            "D3", "T8", "W", "S", "W", "E", "N", "W", "S",
            "K4", "D4", "T9", "R9", "D1", "T7", "X7", "X9", "R8",
            "I", "$", "N",
        ]),
    ];

    scenarios
        .iter()
        .map(|(name, cmds)| {
            let input_bytes = cmds.join("\n").into_bytes();
            (*name, input_bytes)
        })
        .collect()
}

fn split_output_to_strings(output: &[u8], scenario: &str) -> Vec<GameString> {
    let text = String::from_utf8_lossy(output);
    let mut strings = Vec::new();
    let mut offset = 0;

    for line in text.split('\n') {
        let trimmed = line.trim();
        // Filter out prompts and very short lines.
        if trimmed.len() >= 3 && trimmed != ">" {
            let clean = trimmed.trim_start_matches('>').trim();
            if !clean.is_empty() {
                strings.push(GameString {
                    text: clean.to_string(),
                    output_offset: offset,
                    scenario: scenario.to_string(),
                });
            }
        }
        offset += line.len() + 1;
    }

    strings
}

/// Search extracted strings for a pattern (case-insensitive substring).
pub fn find_string<'a>(strings: &'a [GameString], pattern: &str) -> Vec<&'a GameString> {
    let pat = pattern.to_lowercase();
    strings
        .iter()
        .filter(|s| s.text.to_lowercase().contains(&pat))
        .collect()
}
