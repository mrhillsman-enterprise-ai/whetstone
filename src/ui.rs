use console::style;
use dialoguer::{Confirm, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, IsTerminal};

const BOX_WIDTH: usize = 48;

pub fn info(msg: &str) {
    eprintln!("{} {msg}", style("[INFO]").blue().bold());
}

pub fn ok(msg: &str) {
    eprintln!("{} {msg}", style("  [OK]").green().bold());
}

pub fn warn(msg: &str) {
    eprintln!("{} {msg}", style("[WARN]").yellow().bold());
}

pub fn fail(msg: &str) -> ! {
    eprintln!("{} {msg}", style("[FAIL]").red().bold());
    std::process::exit(1);
}

pub fn is_interactive() -> bool {
    io::stdin().is_terminal()
}

pub fn confirm(prompt: &str, default: bool) -> bool {
    if !is_interactive() {
        return default;
    }
    Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact()
        .unwrap_or(default)
}

pub fn select<T: std::fmt::Display>(prompt: &str, items: &[T], default: usize) -> usize {
    if !is_interactive() {
        return default;
    }
    Select::new()
        .with_prompt(prompt)
        .items(items)
        .default(default)
        .interact()
        .unwrap_or(default)
}

pub fn upgrade_banner(components: &[crate::update::OutdatedComponent]) {
    if components.is_empty() {
        return;
    }

    let title = "UPDATES AVAILABLE";
    let action_line = "Run: whetstone update";

    let mut content_lines: Vec<String> = Vec::new();
    for c in components {
        content_lines.push(format!("{}: {} → {}", c.name, c.current, c.latest));
    }

    let max_content = content_lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let min_width = [
        title.chars().count() + 4,
        action_line.chars().count(),
        max_content,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    let inner = min_width.max(BOX_WIDTH - 4);
    let border = "─".repeat(inner + 2);

    eprintln!();
    eprintln!(
        "  {}{}{}",
        style("┌").cyan(),
        style(&border).cyan(),
        style("┐").cyan()
    );
    eprintln!(
        "  {}{}{}",
        style("│").cyan(),
        " ".repeat(inner + 2),
        style("│").cyan()
    );
    eprintln!(
        "  {} {} {}{}",
        style("│").cyan(),
        style(format!("⬆  {title}")).yellow().bold(),
        " ".repeat(inner.saturating_sub(title.chars().count() + 4)),
        style("│").cyan()
    );
    eprintln!(
        "  {}{}{}",
        style("│").cyan(),
        " ".repeat(inner + 2),
        style("│").cyan()
    );
    for line in &content_lines {
        eprintln!(
            "  {} {} {}{}",
            style("│").cyan(),
            style(line).white().bold(),
            " ".repeat(inner.saturating_sub(line.chars().count() + 1)),
            style("│").cyan()
        );
    }
    eprintln!(
        "  {}{}{}",
        style("│").cyan(),
        " ".repeat(inner + 2),
        style("│").cyan()
    );
    eprintln!(
        "  {} {} {}{}",
        style("│").cyan(),
        style(action_line).dim(),
        " ".repeat(inner.saturating_sub(action_line.chars().count() + 1)),
        style("│").cyan()
    );
    eprintln!(
        "  {}{}{}",
        style("│").cyan(),
        " ".repeat(inner + 2),
        style("│").cyan()
    );
    eprintln!(
        "  {}{}{}",
        style("└").cyan(),
        style(&border).cyan(),
        style("┘").cyan()
    );
    eprintln!();
}

pub fn section(title: &str) {
    let line = "─".repeat(40);
    eprintln!();
    eprintln!("  {} {}", style(title).bold(), style(&line).dim());
    eprintln!();
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
    match status {
        ComponentStatus::UpToDate(ver) => {
            eprintln!(
                "  {} {} {}",
                style("●").green(),
                style(&label).bold(),
                style(format!("{ver} (up to date)")).dim()
            );
        }
        ComponentStatus::Updated(from, to) => {
            eprintln!(
                "  {} {} {} → {}",
                style("●").green(),
                style(&label).bold(),
                style(from).dim(),
                style(to).green().bold()
            );
        }
        ComponentStatus::NotInstalled => {
            eprintln!(
                "  {} {} {}",
                style("○").dim(),
                style(&label).bold(),
                style("not installed").dim()
            );
        }
        ComponentStatus::Failed(reason) => {
            eprintln!(
                "  {} {} {}",
                style("✗").red(),
                style(&label).bold(),
                style(reason).red()
            );
        }
    }
}

pub fn summary_ok(msg: &str) {
    eprintln!();
    eprintln!("  {} {}", style("✓").green().bold(), style(msg).green());
    eprintln!();
}

pub fn summary_info(msg: &str) {
    eprintln!();
    eprintln!("  {} {}", style("ℹ").blue().bold(), style(msg).bold());
    eprintln!();
}

pub fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    let style = ProgressStyle::with_template("  {spinner:.cyan} {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_spinner());
    pb.set_style(style.tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✓"]));
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}
