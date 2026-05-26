use crossterm::style::Stylize;
use crossterm::{cursor, event, terminal};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal, TerminalOptions, Viewport,
};
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn render_inline_stderr(lines: Vec<Line<'_>>) {
    let height = lines.len() as u16;
    let backend = ratatui::backend::CrosstermBackend::new(io::stderr());
    if let Ok(mut terminal) = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    ) {
        let _ = terminal.draw(|frame| {
            let paragraph = Paragraph::new(lines);
            frame.render_widget(paragraph, frame.area());
        });
    }
}

pub fn info(msg: &str) {
    if is_interactive() {
        render_inline_stderr(vec![Line::from(vec![
            Span::styled(
                "[INFO]",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {msg}")),
        ])]);
    } else {
        eprintln!("[INFO] {msg}");
    }
}

pub fn ok(msg: &str) {
    if is_interactive() {
        render_inline_stderr(vec![Line::from(vec![
            Span::styled(
                "  [OK]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {msg}")),
        ])]);
    } else {
        eprintln!("  [OK] {msg}");
    }
}

pub fn warn(msg: &str) {
    if is_interactive() {
        render_inline_stderr(vec![Line::from(vec![
            Span::styled(
                "[WARN]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {msg}")),
        ])]);
    } else {
        eprintln!("[WARN] {msg}");
    }
}

pub fn fail(msg: &str) -> ! {
    if is_interactive() {
        render_inline_stderr(vec![Line::from(vec![
            Span::styled(
                "[FAIL]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {msg}")),
        ])]);
    } else {
        eprintln!("[FAIL] {msg}");
    }
    std::process::exit(1);
}

pub fn is_interactive() -> bool {
    io::stdin().is_terminal()
}

pub fn confirm(prompt: &str, default: bool) -> bool {
    if !is_interactive() {
        return default;
    }

    let hint = if default { "[Y/n]" } else { "[y/N]" };
    eprint!("{} {} ", prompt, hint.dim());
    let _ = io::stderr().flush();

    terminal::enable_raw_mode().ok();
    let result = loop {
        if let Ok(event::Event::Key(key)) = event::read() {
            match key.code {
                event::KeyCode::Char('y' | 'Y') => break true,
                event::KeyCode::Char('n' | 'N') => break false,
                event::KeyCode::Enter => break default,
                event::KeyCode::Esc => break false,
                _ => {}
            }
        }
    };
    terminal::disable_raw_mode().ok();
    eprintln!("{}", if result { "yes" } else { "no" });
    result
}

pub fn select<T: std::fmt::Display>(prompt: &str, items: &[T], default: usize) -> usize {
    if !is_interactive() || items.is_empty() {
        return default;
    }

    let mut selected = default;

    eprintln!("{prompt}");

    terminal::enable_raw_mode().ok();
    loop {
        let mut stderr = io::stderr();
        for (i, item) in items.iter().enumerate() {
            if i == selected {
                let _ = write!(
                    stderr,
                    "\r  {} {}\r\n",
                    "›".cyan().bold(),
                    item.to_string().bold()
                );
            } else {
                let _ = write!(stderr, "\r    {}\r\n", item);
            }
        }
        let _ = stderr.flush();

        if let Ok(event::Event::Key(key)) = event::read() {
            match key.code {
                event::KeyCode::Up | event::KeyCode::Char('k') => {
                    selected = selected.saturating_sub(1);
                }
                event::KeyCode::Down | event::KeyCode::Char('j') if selected + 1 < items.len() => {
                    selected += 1;
                }
                event::KeyCode::Enter => break,
                event::KeyCode::Esc => {
                    selected = default;
                    break;
                }
                _ => {}
            }
        }

        // Move cursor back up to redraw
        let _ = crossterm::execute!(stderr, cursor::MoveUp(items.len() as u16));
    }
    terminal::disable_raw_mode().ok();

    // Clear the selection display
    let mut stderr = io::stderr();
    for _ in items {
        let _ = write!(stderr, "\r{}\r\n", " ".repeat(60));
    }
    let _ = crossterm::execute!(stderr, cursor::MoveUp(items.len() as u16));
    eprintln!("{}: {}", prompt, items[selected].to_string().cyan().bold());

    selected
}

pub fn upgrade_banner(components: &[crate::update::OutdatedComponent]) {
    if components.is_empty() {
        return;
    }

    let title = "⬆  UPDATES AVAILABLE";
    let action_line = "Run: whetstone update";

    let mut content_lines: Vec<Line<'_>> = Vec::new();
    for c in components {
        content_lines.push(Line::from(vec![Span::styled(
            format!("{}: {} → {}", c.name, c.current, c.latest),
            Style::default().add_modifier(Modifier::BOLD),
        )]));
    }
    content_lines.push(Line::from(""));
    content_lines.push(Line::from(Span::styled(
        action_line,
        Style::default().add_modifier(Modifier::DIM),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(content_lines).block(block);

    let height = (components.len() + 5) as u16;
    eprintln!();
    let backend = ratatui::backend::CrosstermBackend::new(io::stderr());
    if let Ok(mut terminal) = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    ) {
        let _ = terminal.draw(|frame| {
            frame.render_widget(paragraph, frame.area());
        });
    }
    eprintln!();
}

pub fn section(title: &str) {
    if is_interactive() {
        let line_chars = "─".repeat(40);
        render_inline_stderr(vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::styled(line_chars, Style::default().add_modifier(Modifier::DIM)),
            ]),
            Line::from(""),
        ]);
    } else {
        eprintln!();
        eprintln!("  {title} {}", "─".repeat(40));
        eprintln!();
    }
}

#[derive(Debug)]
pub enum ComponentStatus {
    UpToDate(String),
    Updated(String, String),
    NotInstalled,
    Failed(String),
}

pub fn component_line(name: &str, status: &ComponentStatus) {
    let label = format!("{:.<16}", format!("{name} "));
    let line = match status {
        ComponentStatus::UpToDate(ver) => Line::from(vec![
            Span::raw("  "),
            Span::styled("●", Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(&*label, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(
                format!("{ver} (up to date)"),
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]),
        ComponentStatus::Updated(from, to) => Line::from(vec![
            Span::raw("  "),
            Span::styled("●", Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(&*label, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(from.as_str(), Style::default().add_modifier(Modifier::DIM)),
            Span::raw(" → "),
            Span::styled(
                to.as_str(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        ComponentStatus::NotInstalled => Line::from(vec![
            Span::raw("  "),
            Span::styled("○", Style::default().add_modifier(Modifier::DIM)),
            Span::raw(" "),
            Span::styled(&*label, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(
                "not installed",
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]),
        ComponentStatus::Failed(reason) => Line::from(vec![
            Span::raw("  "),
            Span::styled("✗", Style::default().fg(Color::Red)),
            Span::raw(" "),
            Span::styled(&*label, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(reason.as_str(), Style::default().fg(Color::Red)),
        ]),
    };

    if is_interactive() {
        render_inline_stderr(vec![line]);
    } else {
        let plain = match status {
            ComponentStatus::UpToDate(ver) => format!("  ● {label} {ver} (up to date)"),
            ComponentStatus::Updated(from, to) => format!("  ● {label} {from} → {to}"),
            ComponentStatus::NotInstalled => format!("  ○ {label} not installed"),
            ComponentStatus::Failed(reason) => format!("  ✗ {label} {reason}"),
        };
        eprintln!("{plain}");
    }
}

pub struct VersionEntry {
    pub name: &'static str,
    pub version: Option<String>,
    pub outdated: bool,
}

pub fn version_report(entries: &[VersionEntry]) {
    let parts: Vec<String> = entries
        .iter()
        .map(|e| {
            let indicator = if e.outdated { " ⬆" } else { "" };
            match &e.version {
                Some(v) => format!(
                    "{} {}{}",
                    e.name.bold(),
                    v.clone().cyan(),
                    indicator.yellow().bold()
                ),
                None => format!("{} {}", e.name.bold(), "—".dim()),
            }
        })
        .collect();
    println!("{}", parts.join("  "));
}

pub fn summary_ok(msg: &str) {
    if is_interactive() {
        render_inline_stderr(vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "✓",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(msg, Style::default().fg(Color::Green)),
            ]),
            Line::from(""),
        ]);
    } else {
        eprintln!();
        eprintln!("  ✓ {msg}");
        eprintln!();
    }
}

pub fn summary_info(msg: &str) {
    if is_interactive() {
        render_inline_stderr(vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "ℹ",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(msg, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
        ]);
    } else {
        eprintln!();
        eprintln!("  ℹ {msg}");
        eprintln!();
    }
}

const BRAILLE: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    pub fn finish_and_clear(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        let mut stderr = io::stderr();
        let _ = write!(stderr, "\r{}\r", " ".repeat(80));
        let _ = stderr.flush();
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if !self.stop.load(Ordering::Relaxed) {
            self.finish_and_clear();
        }
    }
}

pub fn spinner(msg: &str) -> Spinner {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let msg = msg.to_string();

    if !is_interactive() {
        eprintln!("  {msg}");
        return Spinner { stop, handle: None };
    }

    let handle = std::thread::spawn(move || {
        let mut i = 0;
        let mut stderr = io::stderr();
        while !stop_clone.load(Ordering::Relaxed) {
            let frame = BRAILLE[i % BRAILLE.len()];
            let _ = write!(stderr, "\r  {} {}", frame.cyan(), msg);
            let _ = stderr.flush();
            i += 1;
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    });

    Spinner {
        stop,
        handle: Some(handle),
    }
}
