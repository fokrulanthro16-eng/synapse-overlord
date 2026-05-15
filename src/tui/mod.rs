use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{
        self, Event, KeyCode, KeyEventKind, KeyModifiers,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame, Terminal,
};

use crate::system::SystemMonitor;

pub struct AppState {
    pub input: String,
    pub logs: Vec<String>,
    pub status: String,
    pub thought_stream: Vec<String>,
    pub should_quit: bool,
    monitor: SystemMonitor,
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            logs: vec![
                "Synapse-Overlord initialized.".to_string(),
                "System monitor active.".to_string(),
                "Commands: map project | test sandbox | ask models | run agent".to_string(),
            ],
            status: "Idle".to_string(),
            thought_stream: vec!["Awaiting goal...".to_string()],
            should_quit: false,
            monitor: SystemMonitor::new(),
            cpu_percent: 0.0,
            ram_percent: 0.0,
            ram_used_mb: 0,
            ram_total_mb: 0,
        }
    }

    pub fn refresh_system(&mut self) {
        let snap = self.monitor.snapshot();
        self.cpu_percent = snap.cpu_percent;
        self.ram_percent = snap.ram_percent;
        self.ram_used_mb = snap.ram_used_mb;
        self.ram_total_mb = snap.ram_total_mb;

        if snap.ram_critical() {
            self.push_log("[CRITICAL] RAM above 90% — agent pausing risky operations.".to_string());
        }
    }

    pub fn push_log(&mut self, msg: String) {
        self.logs.push(msg);
        if self.logs.len() > 500 {
            self.logs.drain(0..100);
        }
    }

    fn push_thought(&mut self, msg: String) {
        self.thought_stream.push(msg);
        if self.thought_stream.len() > 100 {
            self.thought_stream.drain(0..20);
        }
    }

    pub fn handle_input(&mut self) {
        let cmd = self.input.trim().to_string();
        self.input.clear();
        if cmd.is_empty() {
            return;
        }
        self.push_log(format!("> {}", cmd));

        match cmd.to_lowercase().trim() {
            "map project" => self.run_map_project(),
            "test sandbox" => self.run_test_sandbox(),
            "ask models" => self.run_ask_models(),
            "run agent" => self.run_agent_pipeline(),
            other => {
                self.push_thought(format!("Unknown command: {}", other));
                self.push_log(format!(
                    "[TUI] Unknown command '{}'. Try: map project | test sandbox | ask models | run agent",
                    other
                ));
            }
        }
    }

    // ── map project ───────────────────────────────────────────────────────────

    fn run_map_project(&mut self) {
        self.status = "Mapping project...".to_string();
        self.push_log("[RAG] Scanning project structure...".to_string());
        self.push_thought("Mapping project files...".to_string());

        match crate::rag::map_project(std::path::Path::new(".")) {
            Ok(map) => {
                self.push_log(format!(
                    "[RAG] {} files mapped  |  {} skipped  |  root: {}",
                    map.files.len(),
                    map.skipped_count,
                    map.root.display()
                ));
                for node in map.files.iter().take(50) {
                    let size_label = if node.size_bytes >= 1024 {
                        format!("{}KB", node.size_bytes / 1024)
                    } else {
                        format!("{}B", node.size_bytes)
                    };
                    let import_hint = if node.imports.is_empty() {
                        String::new()
                    } else {
                        format!("  [uses: {}]", node.imports.join(", "))
                    };
                    self.push_log(format!(
                        "  {}  [{}]  {}{}",
                        node.relative_path,
                        node.role.label(),
                        size_label,
                        import_hint
                    ));
                }
                if map.files.len() > 50 {
                    self.push_log(format!("  ... and {} more files", map.files.len() - 50));
                }
                self.push_thought(format!("Project mapped: {} files.", map.files.len()));
            }
            Err(e) => self.push_log(format!("[RAG ERROR] {}", e)),
        }

        self.status = "Idle".to_string();
    }

    // ── test sandbox ──────────────────────────────────────────────────────────

    fn run_test_sandbox(&mut self) {
        self.status = "Testing sandbox...".to_string();
        self.push_log("[Sandbox] Compiling and running Rust test artifact...".to_string());
        self.push_thought("Sandbox: compiling Rust...".to_string());

        let code = "fn main() {\n    \
                    println!(\"Synapse sandbox: OK\");\n    \
                    println!(\"Rust execution: confirmed\");\n\
                    }\n";

        let req = crate::sandbox::SandboxRequest::new(crate::sandbox::SandboxLanguage::Rust, code);

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(crate::sandbox::run_sandbox(req))
        });

        match result {
            Ok(r) => {
                self.push_log(format!("[Sandbox] {}", r.summary()));
                for line in r.stdout.lines().take(10) {
                    self.push_log(format!("[Sandbox] {}", line));
                }
                if !r.success && !r.stderr.is_empty() {
                    let preview = &r.stderr[..r.stderr.len().min(200)];
                    self.push_log(format!("[Sandbox] stderr: {}", preview.trim()));
                }
                self.push_thought(if r.success {
                    "Sandbox: PASS".to_string()
                } else {
                    "Sandbox: FAIL".to_string()
                });
            }
            Err(e) => self.push_log(format!("[Sandbox ERROR] {}", e)),
        }

        self.status = "Idle".to_string();
    }

    // ── ask models ────────────────────────────────────────────────────────────

    fn run_ask_models(&mut self) {
        self.status = "Consulting models...".to_string();
        self.push_log("[Models] Running triple consensus...".to_string());
        self.push_thought("Sending to logic · audit · optimization models...".to_string());

        let prompt =
            "Analyze the Synapse-Overlord Rust project and suggest one concrete improvement.";

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(crate::models::run_consensus(prompt))
        });

        match result {
            Ok(consensus) => {
                self.push_log(format!("[Models] {}", consensus.summary()));
                for (label, resp) in [
                    ("Logic", &consensus.logic),
                    ("Audit", &consensus.audit),
                    ("Optimize", &consensus.optimize),
                ] {
                    let preview: String = resp.content.chars().take(200).collect();
                    self.push_log(format!("[{}] {}", label, preview.trim()));
                }
                self.push_thought(if consensus.logic.offline {
                    "Models: OFFLINE fallback".to_string()
                } else {
                    "Models: consensus complete".to_string()
                });
            }
            Err(e) => self.push_log(format!("[Models ERROR] {}", e)),
        }

        self.status = "Idle".to_string();
    }

    // ── run agent ─────────────────────────────────────────────────────────────

    fn run_agent_pipeline(&mut self) {
        self.status = "Agent running...".to_string();
        self.push_log("[Agent] Starting full pipeline...".to_string());
        self.push_thought("Agent: initializing...".to_string());

        let config = crate::agent::AgentConfig::default();
        let goal = "Analyze this Rust project structure and generate a minimal summary artifact.";

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(crate::agent::run_goal(goal, &config))
        });

        match result {
            Ok(agent_state) => {
                for event in agent_state.events {
                    match event {
                        crate::agent::AgentEvent::Log(msg) => self.push_log(msg),
                        crate::agent::AgentEvent::ThoughtStream(msg) => self.push_thought(msg),
                        crate::agent::AgentEvent::Done { success } => {
                            self.push_log(format!(
                                "[Agent] Pipeline {}.",
                                if success { "SUCCEEDED" } else { "FAILED" }
                            ));
                        }
                    }
                }
            }
            Err(e) => self.push_log(format!("[Agent ERROR] {}", e)),
        }

        self.status = "Idle".to_string();
    }
}

// ── TUI event loop ────────────────────────────────────────────────────────────

pub fn run_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let enhanced = supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        execute!(
            terminal.backend_mut(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
            )
        )?;
    }

    let mut state = AppState::new();
    let tick = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| render(f, &state))?;

        let timeout = tick.checked_sub(last_tick.elapsed()).unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Release {
                    match key.code {
                        KeyCode::Esc => state.should_quit = true,
                        KeyCode::Char('c')
                            if key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            state.should_quit = true;
                        }
                        KeyCode::Enter => state.handle_input(),
                        KeyCode::Backspace => {
                            state.input.pop();
                        }
                        KeyCode::Char(c) => state.input.push(c),
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick {
            state.refresh_system();
            last_tick = Instant::now();
        }

        if state.should_quit {
            break;
        }
    }

    if enhanced {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render(f: &mut Frame, state: &AppState) {
    let outer = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(26), Constraint::Min(40)])
        .split(f.area());

    render_sidebar(f, outer[0], state);
    render_main(f, outer[1], state);
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

fn render_sidebar(f: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" ◈ SYNAPSE ")
        .border_style(Style::default().fg(Color::Cyan))
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status indicator
            Constraint::Length(1), // separator
            Constraint::Length(1), // COMMANDS label
            Constraint::Length(1), // map project
            Constraint::Length(1), // test sandbox
            Constraint::Length(1), // ask models
            Constraint::Length(1), // run agent
            Constraint::Length(1), // separator
            Constraint::Length(1), // DATA & SETTINGS label
            Constraint::Length(1), // database status
            Constraint::Length(1), // connect placeholder
            Constraint::Min(0),    // remainder
        ])
        .split(inner);

    // Status indicator
    let (status_icon, status_color) = if state.status == "Idle" {
        ("● Idle", Color::Green)
    } else {
        ("◌ Running", Color::Yellow)
    };
    f.render_widget(
        Paragraph::new(status_icon).style(Style::default().fg(status_color)),
        rows[0],
    );

    // Separator
    let w = inner.width as usize;
    f.render_widget(
        Paragraph::new("─".repeat(w)).style(Style::default().fg(Color::DarkGray)),
        rows[1],
    );

    // COMMANDS header
    f.render_widget(
        Paragraph::new("COMMANDS")
            .style(Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
        rows[2],
    );

    // Command buttons — background fills the row rect
    let buttons: [(&str, Color); 4] = [
        (" ◆ map project ", Color::Cyan),
        (" ◆ test sandbox", Color::Green),
        (" ◆ ask models  ", Color::Magenta),
        (" ◆ run agent   ", Color::Yellow),
    ];
    for (i, (label, color)) in buttons.iter().enumerate() {
        f.render_widget(
            Paragraph::new(*label)
                .style(Style::default().fg(Color::Black).bg(*color).add_modifier(Modifier::BOLD)),
            rows[3 + i],
        );
    }

    // Separator
    f.render_widget(
        Paragraph::new("─".repeat(w)).style(Style::default().fg(Color::DarkGray)),
        rows[7],
    );

    // DATA & SETTINGS header
    f.render_widget(
        Paragraph::new("DATA & SETTINGS")
            .style(Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
        rows[8],
    );

    // Database status
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("DB: ", Style::default().fg(Color::Gray)),
            Span::styled("Not Connected", Style::default().fg(Color::Yellow)),
        ])),
        rows[9],
    );

    // Connect placeholder
    f.render_widget(
        Paragraph::new("[ Connect ]").style(Style::default().fg(Color::DarkGray)),
        rows[10],
    );
}

// ── Main area ─────────────────────────────────────────────────────────────────

fn render_main(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // metric cards
            Constraint::Min(5),    // stream panels
            Constraint::Length(3), // command input
            Constraint::Length(1), // hints footer
        ])
        .split(area);

    render_cards(f, chunks[0], state);
    render_streams(f, chunks[1], state);
    render_command_input(f, chunks[2], state);
    render_hints(f, chunks[3]);
}

fn render_cards(f: &mut Frame, area: Rect, state: &AppState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(area);

    // CPU card
    let cpu_color = if state.cpu_percent >= 90.0 {
        Color::Red
    } else if state.cpu_percent >= 60.0 {
        Color::Yellow
    } else {
        Color::Cyan
    };
    f.render_widget(
        Gauge::default()
            .block(
                Block::default()
                    .title(" CPU ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .gauge_style(Style::default().fg(cpu_color).bg(Color::Black))
            .ratio((state.cpu_percent as f64 / 100.0).clamp(0.0, 1.0))
            .label(format!("{:.1}%", state.cpu_percent)),
        cols[0],
    );

    // RAM card
    let ram_color = if state.ram_percent >= 90.0 {
        Color::Red
    } else if state.ram_percent >= 75.0 {
        Color::Yellow
    } else {
        Color::Green
    };
    f.render_widget(
        Gauge::default()
            .block(
                Block::default()
                    .title(" RAM ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green)),
            )
            .gauge_style(Style::default().fg(ram_color).bg(Color::Black))
            .ratio((state.ram_percent as f64 / 100.0).clamp(0.0, 1.0))
            .label(format!(
                "{:.1}%  {}/{}MB",
                state.ram_percent, state.ram_used_mb, state.ram_total_mb
            )),
        cols[1],
    );

    // Memory Guard card
    let (guard_icon, guard_color) = if state.ram_percent >= 90.0 {
        ("■ CRITICAL", Color::Red)
    } else if state.ram_percent >= 75.0 {
        ("▲ WARNING", Color::Yellow)
    } else {
        ("● NOMINAL", Color::Green)
    };
    f.render_widget(
        Paragraph::new(guard_icon)
            .block(
                Block::default()
                    .title(" Memory Guard ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .style(
                Style::default()
                    .fg(guard_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center),
        cols[2],
    );
}

fn render_streams(f: &mut Frame, area: Rect, state: &AppState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    // AI Consensus Stream
    let thought_cap = cols[0].height.saturating_sub(2) as usize;
    let thought_items: Vec<ListItem> = state
        .thought_stream
        .iter()
        .rev()
        .take(thought_cap)
        .rev()
        .map(|s| ListItem::new(s.as_str()).style(Style::default().fg(Color::Magenta)))
        .collect();
    f.render_widget(
        List::new(thought_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" AI Consensus Stream ")
                .border_style(Style::default().fg(Color::Magenta)),
        ),
        cols[0],
    );

    // Execution Log
    let log_cap = cols[1].height.saturating_sub(2) as usize;
    let log_items: Vec<ListItem> = state
        .logs
        .iter()
        .rev()
        .take(log_cap)
        .rev()
        .map(|s| {
            let color = if s.contains("[CRITICAL]") || s.contains("ERROR") {
                Color::Red
            } else if s.contains("[!]") || s.contains("WARN") || s.contains("FAIL") {
                Color::Yellow
            } else if s.starts_with("> ") {
                Color::Cyan
            } else if s.contains("SUCCEED") || s.contains(": OK") || s.contains("PASS") {
                Color::Green
            } else {
                Color::Gray
            };
            ListItem::new(s.as_str()).style(Style::default().fg(color))
        })
        .collect();
    f.render_widget(
        List::new(log_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Execution Log ")
                .border_style(Style::default().fg(Color::Blue)),
        ),
        cols[1],
    );
}

fn render_command_input(f: &mut Frame, area: Rect, state: &AppState) {
    let display = format!(" > {}\u{2588}", state.input);
    f.render_widget(
        Paragraph::new(display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Command Agency ")
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .style(Style::default().fg(Color::White)),
        area,
    );
}

fn render_hints(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new(
            " Enter: run  |  Backspace: delete  |  Esc / Ctrl+C: quit",
        )
        .style(Style::default().fg(Color::DarkGray)),
        area,
    );
}
