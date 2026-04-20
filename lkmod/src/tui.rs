/// Terminal UI for navigating the Lost Kingdom source map and editing strings.

use std::path::PathBuf;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::interpreter::Program;
use crate::patcher;
use crate::sourcemap::{self, Region};
use crate::strings::{self, GameString};

/// Which pane has focus.
#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Regions,
    Strings,
}

/// Active popup mode.
enum Popup {
    None,
    /// Editing a string: (string index, input buffer, status message).
    EditString(usize, String, String),
    /// Showing a message.
    Message(String),
}

/// TUI application state.
pub struct App {
    bf_path: PathBuf,
    source: Vec<u8>,
    prog: Program,
    regions: Vec<Region>,
    /// Extracted game strings (all scenarios, deduplicated).
    game_strings: Vec<GameString>,
    /// Strings filtered to current region.
    region_strings: Vec<usize>,
    /// Which tree items are expanded (region indices).
    expanded: Vec<bool>,
    /// Currently selected tree row index.
    tree_cursor: usize,
    /// Currently selected string row.
    string_cursor: usize,
    /// Active focus pane.
    focus: Focus,
    /// Popup state.
    popup: Popup,
    /// Scroll offset for region tree.
    tree_scroll: usize,
    /// Scroll offset for string list.
    string_scroll: usize,
    /// Status bar message.
    status: String,
    /// Whether source was modified.
    dirty: bool,
}

/// A row in the flattened tree view.
struct TreeRow {
    depth: u8,
    label: String,
    region_idx: usize,
    is_region: bool,
}

impl App {
    pub fn new(bf_path: PathBuf) -> Self {
        let source = std::fs::read(&bf_path).unwrap_or_else(|e| {
            eprintln!("Error reading {}: {}", bf_path.display(), e);
            std::process::exit(1);
        });
        let prog = Program::compile(&source);
        let regions = sourcemap::build_source_map();
        let expanded = vec![false; regions.len()];

        // Extract all game strings.
        let scenarios = strings::lk_default_scenarios();
        let scenario_refs: Vec<(&str, Vec<u8>)> =
            scenarios.iter().map(|(n, d)| (*n, d.clone())).collect();
        let game_strings = strings::extract_multi(&prog, &scenario_refs);

        let mut app = App {
            bf_path,
            source,
            prog,
            regions,
            game_strings,
            region_strings: Vec::new(),
            expanded,
            tree_cursor: 0,
            string_cursor: 0,
            focus: Focus::Regions,
            popup: Popup::None,
            tree_scroll: 0,
            string_scroll: 0,
            status: String::from("Arrow keys: navigate | Tab: switch pane | Enter: expand/collapse | e: edit string | s: save | q: quit"),
            dirty: false,
        };
        app.update_region_strings();
        app
    }

    /// Build flattened tree rows from regions + expanded state.
    fn tree_rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        for (i, region) in self.regions.iter().enumerate() {
            rows.push(TreeRow {
                depth: 0,
                label: format!("[{}] {} ({:.0}K, {} secs)", region.id, region.name, region.size() as f64 / 1024.0, region.sections),
                region_idx: i,
                is_region: true,
            });
            if self.expanded[i] {
                // Sub-regions.
                for sr in &region.sub_regions {
                    rows.push(TreeRow {
                        depth: 1,
                        label: format!("{} ({} secs, {} dots)", sr.name, sr.sections, sr.dots),
                        region_idx: i,
                        is_region: false,
                    });
                }
                // Notable items by category.
                let mut categories: Vec<&str> = region
                    .notable
                    .iter()
                    .map(|n| n.category)
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                categories.sort();
                for cat in categories {
                    rows.push(TreeRow {
                        depth: 1,
                        label: format!("── {} ──", cat),
                        region_idx: i,
                        is_region: false,
                    });
                    for notable in region.notable.iter().filter(|n| n.category == cat) {
                        let detail = if notable.detail.is_empty() {
                            format!("raw[{}]", notable.raw_pos)
                        } else {
                            format!("raw[{}] {}", notable.raw_pos, notable.detail)
                        };
                        rows.push(TreeRow {
                            depth: 2,
                            label: format!("{}: {}", notable.label, detail),
                            region_idx: i,
                            is_region: false,
                        });
                    }
                }
            }
        }
        rows
    }

    /// Get the region index that the current tree cursor points to.
    fn selected_region_idx(&self) -> usize {
        let rows = self.tree_rows();
        if self.tree_cursor < rows.len() {
            rows[self.tree_cursor].region_idx
        } else {
            0
        }
    }

    /// Filter game strings to the currently selected region's raw byte range.
    fn update_region_strings(&mut self) {
        let idx = self.selected_region_idx();
        let region = &self.regions[idx];

        // We don't have exact raw positions for each string, but we can run
        // patcher::trace_dot_positions to map output → source. Instead, for
        // efficiency we filter by output_offset heuristic: strings from different
        // scenarios may not map perfectly. Show all strings and let user browse.
        // In the future this can be refined.
        //
        // For now: show ALL strings (they're only ~265). The region panel gives
        // structural context.
        self.region_strings = (0..self.game_strings.len()).collect();
        let _ = region; // region context shown in tree
        self.string_cursor = 0;
        self.string_scroll = 0;
    }

    fn handle_key(&mut self, key: event::KeyEvent) -> bool {
        // Handle popup first.
        if let Popup::EditString(str_idx, ref buf, ref _status_msg) = self.popup {
            match key.code {
                KeyCode::Esc => {
                    self.popup = Popup::None;
                    return false;
                }
                KeyCode::Enter => {
                    let replacement = buf.clone();
                    let original = self.game_strings[self.region_strings[str_idx]].text.clone();
                    if replacement.len() > original.len() {
                        self.popup = Popup::EditString(str_idx, replacement.clone(),
                            format!("Too long! Max {} chars (got {})", original.len(), replacement.len()));
                    } else if replacement == original {
                        self.popup = Popup::None;
                    } else {
                        match self.apply_patch(&original, &replacement) {
                            Ok(()) => {
                                self.dirty = true;
                                self.status = format!("Patched: \"{}\" → \"{}\"", original, replacement);
                                self.popup = Popup::None;
                                self.recompile_and_extract();
                            }
                            Err(e) => {
                                self.popup = Popup::EditString(str_idx, replacement,
                                    format!("Patch failed: {}", e));
                            }
                        }
                    }
                    return false;
                }
                KeyCode::Backspace => {
                    if let Popup::EditString(_, ref mut b, _) = self.popup {
                        b.pop();
                    }
                    return false;
                }
                KeyCode::Char(c) => {
                    if let Popup::EditString(_, ref mut b, _) = self.popup {
                        b.push(c);
                    }
                    return false;
                }
                _ => { return false; }
            }
        }
        if let Popup::Message(_) = self.popup {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                    self.popup = Popup::None;
                }
                _ => {}
            }
            return false;
        }

        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Regions => Focus::Strings,
                    Focus::Strings => Focus::Regions,
                };
            }
            KeyCode::Up => match self.focus {
                Focus::Regions => {
                    if self.tree_cursor > 0 {
                        self.tree_cursor -= 1;
                        self.update_region_strings();
                    }
                }
                Focus::Strings => {
                    if self.string_cursor > 0 {
                        self.string_cursor -= 1;
                    }
                }
            },
            KeyCode::Down => match self.focus {
                Focus::Regions => {
                    let max = self.tree_rows().len().saturating_sub(1);
                    if self.tree_cursor < max {
                        self.tree_cursor += 1;
                        self.update_region_strings();
                    }
                }
                Focus::Strings => {
                    let max = self.region_strings.len().saturating_sub(1);
                    if self.string_cursor < max {
                        self.string_cursor += 1;
                    }
                }
            },
            KeyCode::Enter | KeyCode::Right => {
                if self.focus == Focus::Regions {
                    let rows = self.tree_rows();
                    if self.tree_cursor < rows.len() && rows[self.tree_cursor].is_region {
                        let ri = rows[self.tree_cursor].region_idx;
                        self.expanded[ri] = !self.expanded[ri];
                    }
                }
            }
            KeyCode::Left => {
                if self.focus == Focus::Regions {
                    let rows = self.tree_rows();
                    if self.tree_cursor < rows.len() {
                        let ri = rows[self.tree_cursor].region_idx;
                        if self.expanded[ri] {
                            self.expanded[ri] = false;
                        }
                    }
                }
            }
            KeyCode::Char('e') => {
                if self.focus == Focus::Strings && !self.region_strings.is_empty() {
                    let idx = self.string_cursor;
                    let gs = &self.game_strings[self.region_strings[idx]];
                    self.popup = Popup::EditString(idx, gs.text.clone(), String::new());
                }
            }
            KeyCode::Char('s') => {
                if self.dirty {
                    match std::fs::write(&self.bf_path, &self.source) {
                        Ok(()) => {
                            self.dirty = false;
                            self.status = format!("Saved to {}", self.bf_path.display());
                        }
                        Err(e) => {
                            self.status = format!("Save error: {}", e);
                        }
                    }
                } else {
                    self.status = "No changes to save.".to_string();
                }
            }
            KeyCode::PageUp => match self.focus {
                Focus::Regions => {
                    self.tree_cursor = self.tree_cursor.saturating_sub(20);
                    self.update_region_strings();
                }
                Focus::Strings => {
                    self.string_cursor = self.string_cursor.saturating_sub(20);
                }
            },
            KeyCode::PageDown => match self.focus {
                Focus::Regions => {
                    let max = self.tree_rows().len().saturating_sub(1);
                    self.tree_cursor = (self.tree_cursor + 20).min(max);
                    self.update_region_strings();
                }
                Focus::Strings => {
                    let max = self.region_strings.len().saturating_sub(1);
                    self.string_cursor = (self.string_cursor + 20).min(max);
                }
            },
            KeyCode::Home => match self.focus {
                Focus::Regions => {
                    self.tree_cursor = 0;
                    self.update_region_strings();
                }
                Focus::Strings => {
                    self.string_cursor = 0;
                }
            },
            KeyCode::End => match self.focus {
                Focus::Regions => {
                    self.tree_cursor = self.tree_rows().len().saturating_sub(1);
                    self.update_region_strings();
                }
                Focus::Strings => {
                    self.string_cursor = self.region_strings.len().saturating_sub(1);
                }
            },
            _ => {}
        }
        false
    }

    fn apply_patch(&mut self, original: &str, replacement: &str) -> Result<(), String> {
        // Try each scenario's input to locate the string.
        let scenarios = strings::lk_default_scenarios();
        for (_name, input) in &scenarios {
            if let Some(location) = patcher::locate_string(&self.source, input, original) {
                patcher::patch_string(&mut self.source, &location, replacement)?;
                return Ok(());
            }
        }
        Err("Could not locate string in any scenario".to_string())
    }

    fn recompile_and_extract(&mut self) {
        self.prog = Program::compile(&self.source);
        let scenarios = strings::lk_default_scenarios();
        let scenario_refs: Vec<(&str, Vec<u8>)> =
            scenarios.iter().map(|(n, d)| (*n, d.clone())).collect();
        self.game_strings = strings::extract_multi(&self.prog, &scenario_refs);
        self.update_region_strings();
    }

    fn draw(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(3),
            ])
            .split(frame.area());

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(60),
            ])
            .split(chunks[0]);

        self.draw_tree(frame, main_chunks[0]);
        self.draw_strings(frame, main_chunks[1]);
        self.draw_status(frame, chunks[1]);

        // Draw popup if active.
        match &self.popup {
            Popup::EditString(idx, buf, status_msg) => {
                let gs = &self.game_strings[self.region_strings[*idx]];
                self.draw_edit_popup(frame, &gs.text.clone(), buf, status_msg);
            }
            Popup::Message(msg) => {
                self.draw_message_popup(frame, msg);
            }
            Popup::None => {}
        }
    }

    fn draw_tree(&mut self, frame: &mut Frame, area: Rect) {
        let rows = self.tree_rows();
        let visible_height = area.height.saturating_sub(2) as usize;

        // Adjust scroll to keep cursor visible.
        if self.tree_cursor < self.tree_scroll {
            self.tree_scroll = self.tree_cursor;
        }
        if self.tree_cursor >= self.tree_scroll + visible_height {
            self.tree_scroll = self.tree_cursor - visible_height + 1;
        }

        let items: Vec<ListItem> = rows
            .iter()
            .enumerate()
            .skip(self.tree_scroll)
            .take(visible_height)
            .map(|(i, row)| {
                let indent = "  ".repeat(row.depth as usize);
                let marker = if row.is_region {
                    if self.expanded[row.region_idx] { "▼ " } else { "▶ " }
                } else {
                    ""
                };
                let text = format!("{}{}{}", indent, marker, row.label);
                let style = if i == self.tree_cursor {
                    if self.focus == Focus::Regions {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::Black).bg(Color::DarkGray)
                    }
                } else if row.is_region {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                ListItem::new(text).style(style)
            })
            .collect();

        let title = format!(" Source Map ({} regions) ", self.regions.len());
        let border_style = if self.focus == Focus::Regions {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style));
        frame.render_widget(list, area);
    }

    fn draw_strings(&mut self, frame: &mut Frame, area: Rect) {
        let visible_height = area.height.saturating_sub(2) as usize;

        // Adjust scroll.
        if self.string_cursor < self.string_scroll {
            self.string_scroll = self.string_cursor;
        }
        if self.string_cursor >= self.string_scroll + visible_height {
            self.string_scroll = self.string_cursor - visible_height + 1;
        }

        let items: Vec<ListItem> = self
            .region_strings
            .iter()
            .enumerate()
            .skip(self.string_scroll)
            .take(visible_height)
            .map(|(i, &gs_idx)| {
                let gs = &self.game_strings[gs_idx];
                let preview: String = gs.text.chars().take(80).collect();
                let text = format!("[{}] {}", gs.scenario, preview);
                let style = if i == self.string_cursor {
                    if self.focus == Focus::Strings {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::Black).bg(Color::DarkGray)
                    }
                } else {
                    Style::default()
                };
                ListItem::new(text).style(style)
            })
            .collect();

        let title = format!(
            " Strings ({}) — 'e' to edit ",
            self.region_strings.len()
        );
        let border_style = if self.focus == Focus::Strings {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style));
        frame.render_widget(list, area);
    }

    fn draw_status(&self, frame: &mut Frame, area: Rect) {
        let dirty_marker = if self.dirty { " [MODIFIED]" } else { "" };
        let text = format!("{}{}", self.status, dirty_marker);
        let para = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Status "))
            .style(Style::default().fg(Color::White));
        frame.render_widget(para, area);
    }

    fn draw_edit_popup(&self, frame: &mut Frame, original: &str, buf: &str, status_msg: &str) {
        let area = centered_rect(70, 40, frame.area());
        frame.render_widget(Clear, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(1),
            ])
            .split(area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Edit String (Enter=apply, Esc=cancel) ")
            .border_style(Style::default().fg(Color::Green));
        frame.render_widget(block, area);

        let orig_text = format!("Original ({} chars): {}", original.len(), original);
        let orig_para = Paragraph::new(orig_text)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(orig_para, chunks[0]);

        let remaining = original.len() as i64 - buf.len() as i64;
        let counter_style = if remaining < 0 {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Green)
        };
        let input_text = Line::from(vec![
            Span::raw("New: "),
            Span::styled(buf, Style::default().fg(Color::White)),
            Span::raw("█"),
            Span::styled(format!("  ({} remaining)", remaining), counter_style),
        ]);
        let input_para = Paragraph::new(input_text);
        frame.render_widget(input_para, chunks[1]);

        if !status_msg.is_empty() {
            let msg_para = Paragraph::new(status_msg.to_string())
                .style(Style::default().fg(Color::Red));
            frame.render_widget(msg_para, chunks[2]);
        }
    }

    fn draw_message_popup(&self, frame: &mut Frame, msg: &str) {
        let area = centered_rect(50, 20, frame.area());
        frame.render_widget(Clear, area);

        let para = Paragraph::new(msg.to_string())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Info (press Enter/Esc) ")
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(para, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Run the TUI application.
pub fn run(bf_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal.
    terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Show loading message.
    terminal.draw(|f| {
        let area = f.area();
        let para = Paragraph::new("Loading Lost Kingdom source map and extracting strings...")
            .block(Block::default().borders(Borders::ALL).title(" lkmod TUI "))
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(para, area);
    })?;

    let mut app = App::new(bf_path);

    loop {
        terminal.draw(|f| app.draw(f))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                if app.handle_key(key) {
                    break;
                }
            }
        }
    }

    // Restore terminal.
    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if app.dirty {
        eprintln!("Warning: unsaved changes were discarded.");
    }

    Ok(())
}
