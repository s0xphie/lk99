/// Run the Lost Kingdom game with save commands and analyze the result.
///
/// - `run_game`: execute with command list, return full output.
/// - `test_save`: run a save file and parse score, inventory, rank.

use crate::interpreter::Program;
use std::fmt;

#[derive(Debug)]
pub struct ScoreEvent {
    pub points: u32,
    pub total_after: u32,
    pub context: String,
}

#[derive(Debug)]
pub struct GameResult {
    pub output: String,
    pub score: Option<u32>,
    pub max_score: Option<u32>,
    pub rank: Option<String>,
    pub inventory: Vec<String>,
    pub score_events: Vec<ScoreEvent>,
    pub alive: bool,
    pub commands_run: usize,
}

impl fmt::Display for GameResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Score: {}/{}",
            self.score.map_or("?".to_string(), |s| s.to_string()),
            self.max_score.map_or("?".to_string(), |s| s.to_string()),
        )?;
        if let Some(ref rank) = self.rank {
            writeln!(f, "Rank: {}", rank)?;
        }
        writeln!(f, "Alive: {}", self.alive)?;
        writeln!(f, "Commands: {}", self.commands_run)?;
        if !self.inventory.is_empty() {
            writeln!(f, "Inventory: {}", self.inventory.join(", "))?;
        }
        if !self.score_events.is_empty() {
            writeln!(f, "Score breakdown:")?;
            for ev in &self.score_events {
                writeln!(f, "  +{}: {} (total: {})", ev.points, ev.context, ev.total_after)?;
            }
        }
        Ok(())
    }
}

/// Run the game with a list of commands. First command should be Y/N for descriptions.
pub fn run_game(prog: &Program, commands: &[&str]) -> String {
    let input = commands.join("\n") + "\n";
    let result = prog.run(input.as_bytes(), false);
    String::from_utf8_lossy(&result.output).to_string()
}

/// Run a save file (one command per line) and parse the game result.
pub fn test_save(prog: &Program, save_text: &str) -> GameResult {
    let commands: Vec<&str> = save_text.lines().collect();
    let cmd_count = commands.len();

    // Append $ (score check) and N (don't play again).
    let mut full_commands = commands;
    full_commands.push("$");
    full_commands.push("N");

    let output = run_game(prog, &full_commands);
    parse_game_output(&output, cmd_count)
}

fn parse_game_output(output: &str, commands_run: usize) -> GameResult {
    let mut score_events = Vec::new();
    let mut running_total: u32 = 0;

    // Find all score-up events.
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("[Your score has just gone up by ") {
            if let Some(pts_str) = rest.strip_suffix(" points.]") {
                if let Ok(pts) = pts_str.trim().parse::<u32>() {
                    running_total += pts;
                    score_events.push(ScoreEvent {
                        points: pts,
                        total_after: running_total,
                        context: find_action_context(output, line),
                    });
                }
            }
        }
    }

    // Parse final score (last occurrence).
    let mut score = None;
    let mut max_score = None;
    for line in output.lines().rev() {
        if score.is_none() {
            if let Some(caps) = parse_score_line(line) {
                score = Some(caps.0);
                max_score = Some(caps.1);
            }
        }
    }

    // Parse rank (last occurrence).
    let rank = output
        .lines()
        .rev()
        .find_map(|line| {
            let needle = "earned the rank of ";
            line.find(needle).and_then(|i| {
                let rest = &line[i + needle.len()..];
                rest.strip_suffix('.').map(|s| s.to_string())
            })
        });

    // Parse inventory (last "carrying:" block).
    let inventory = parse_last_inventory(output);

    let alive = !output.contains("*** You have died ***");

    GameResult {
        output: output.to_string(),
        score,
        max_score,
        rank,
        inventory,
        score_events,
        alive,
        commands_run,
    }
}

fn parse_score_line(line: &str) -> Option<(u32, u32)> {
    // "You scored 99 points out of a possible 100."
    let line = line.trim().trim_start_matches('>');
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 9 && parts[0] == "You" && parts[1] == "scored" && parts[4] == "out" {
        let score = parts[2].parse().ok()?;
        let max = parts[8].trim_end_matches('.').parse().ok()?;
        Some((score, max))
    } else {
        None
    }
}

fn parse_last_inventory(output: &str) -> Vec<String> {
    // Find last "You are carrying:" block.
    let mut last_inv = Vec::new();
    let mut in_inv = false;
    for line in output.lines() {
        let trimmed = line.trim().trim_start_matches('>');
        if trimmed.starts_with("You are carrying:") {
            in_inv = true;
            last_inv.clear();
        } else if in_inv {
            let t = trimmed.trim();
            if t.starts_with("a ") || t.starts_with("some ") || t.starts_with("an ") {
                last_inv.push(t.to_string());
            } else if !t.is_empty() {
                in_inv = false;
            }
        }
    }
    last_inv
}

fn find_action_context(full_output: &str, score_line: &str) -> String {
    // Find the text between the last ">" prompt and the score line.
    if let Some(pos) = full_output.find(score_line) {
        let before = &full_output[..pos];
        if let Some(prompt_pos) = before.rfind('>') {
            let ctx = before[prompt_pos + 1..].trim();
            if !ctx.is_empty() {
                return ctx.to_string();
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_score_line_works() {
        assert_eq!(
            parse_score_line("You scored 99 points out of a possible 100."),
            Some((99, 100))
        );
        assert_eq!(
            parse_score_line(">You scored 0 points out of a possible 100."),
            Some((0, 100))
        );
    }
}
