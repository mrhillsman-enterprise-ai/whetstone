use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::memory::MemoryProvider;
use crate::ui;

const RTK_HOOK_ARGS: [&str; 2] = ["hook", "claude"];
const LEGACY_RTK_REWRITE_SCRIPT: &str = "rtk-rewrite.sh";
const MANAGED_HOOK_SCRIPTS: [&str; 6] = [
    LEGACY_RTK_REWRITE_SCRIPT,
    "pre-tool-notify.sh",
    "pre-push.sh",
    "post-commit.sh",
    "session-start.sh",
    "session-end.sh",
];

pub fn copy_hook_scripts(assets_hooks: &Path, dest_hooks: &Path) -> Result<()> {
    fs::create_dir_all(dest_hooks).with_context(|| format!("creating {}", dest_hooks.display()))?;

    let scripts = [
        "pre-tool-notify.sh",
        "pre-push.sh",
        "post-commit.sh",
        "session-start.sh",
        "session-end.sh",
    ];

    for script in &scripts {
        let src = assets_hooks.join(script);
        if !src.exists() {
            continue;
        }
        let dst = dest_hooks.join(script);
        fs::copy(&src, &dst)
            .with_context(|| format!("copying {script} to {}", dest_hooks.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dst, fs::Permissions::from_mode(0o755))?;
        }
    }

    ui::ok(&format!("copied hook scripts to {}", dest_hooks.display()));
    Ok(())
}

pub fn merge_settings_json(
    settings_path: &Path,
    hooks_dir: &Path,
    provider: MemoryProvider,
) -> Result<()> {
    if settings_path.exists() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let backup = settings_path.with_file_name(format!("settings.json.bak.{ts}"));
        fs::copy(settings_path, &backup)
            .with_context(|| format!("backing up {}", settings_path.display()))?;
        ui::ok("backed up existing settings.json");
    }

    let existing: Value = if settings_path.exists() {
        let content = fs::read_to_string(settings_path)
            .with_context(|| format!("reading {}", settings_path.display()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    let hd = hooks_dir.display().to_string();
    let rtk_command = rtk_claude_hook_command();
    let merged = build_hooks_value(&existing, &hd, provider, &rtk_command);

    let json_str = serde_json::to_string_pretty(&merged).context("serializing settings.json")?;

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(settings_path, json_str)
        .with_context(|| format!("writing {}", settings_path.display()))?;

    ui::ok("all hooks registered in settings.json");
    Ok(())
}

fn build_hooks_value(
    existing: &Value,
    hd: &str,
    provider: MemoryProvider,
    rtk_command: &str,
) -> Value {
    let mut result = existing.clone();

    let whetstone_hooks: Vec<(&str, Vec<Value>)> = vec![
        (
            "PreToolUse",
            vec![
                json!({
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": rtk_command,
                    }]
                }),
                json!({
                    "matcher": "Write|Edit|MultiEdit|Bash",
                    "hooks": [{
                        "type": "command",
                        "command": format!("{hd}/pre-tool-notify.sh"),
                        "timeout": 10000,
                    }]
                }),
                json!({
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": format!(
                            "bash -c 'echo \"$CLAUDE_TOOL_INPUT\" | grep -q \"git push\" && {hd}/pre-push.sh || exit 0'"
                        ),
                        "timeout": 60000,
                    }]
                }),
            ],
        ),
        (
            "PostToolUse",
            vec![json!({
                "matcher": "Bash",
                "hooks": [{
                    "type": "command",
                    "command": format!(
                        "bash -c 'echo \"$CLAUDE_TOOL_INPUT\" | grep -q \"git commit\" && {hd}/post-commit.sh || exit 0'"
                    ),
                    "timeout": 10000,
                }]
            })],
        ),
        (
            "SessionStart",
            vec![json!({
                "hooks": [{
                    "type": "command",
                    "command": format!("{hd}/session-start.sh"),
                    "timeout": 10000,
                }]
            })],
        ),
        (
            "Stop",
            vec![json!({
                "hooks": [{
                    "type": "command",
                    "command": format!("{hd}/session-end.sh"),
                    "timeout": 10000,
                }]
            })],
        ),
    ];

    let old_hooks = result.get("hooks").cloned().unwrap_or_else(|| json!({}));
    let mut new_hooks = serde_json::Map::new();

    for (event, whetstone_entries) in &whetstone_hooks {
        let mut merged: Vec<Value> = old_hooks
            .get(*event)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|entry| !entry_is_whetstone_managed(entry, hd, rtk_command))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        merged.extend(whetstone_entries.iter().cloned());
        new_hooks.insert((*event).to_string(), Value::Array(merged));
    }

    if let Some(obj) = old_hooks.as_object() {
        for (key, val) in obj {
            if !new_hooks.contains_key(key) {
                new_hooks.insert(key.clone(), val.clone());
            }
        }
    }

    result["hooks"] = Value::Object(new_hooks);

    if provider == MemoryProvider::AutoMem {
        if result.get("mcpServers").is_none() {
            result["mcpServers"] = json!({});
        }
        result["mcpServers"]["memory"] = json!({
            "command": "npx",
            "args": ["-y", "@verygoodplugins/mcp-automem"],
        });
    }

    result
}

fn rtk_claude_hook_command() -> String {
    let binary = resolve_rtk_binary();
    let program = shell_escape_program(&binary.display().to_string());
    format!("{program} {} {}", RTK_HOOK_ARGS[0], RTK_HOOK_ARGS[1])
}

fn resolve_rtk_binary() -> PathBuf {
    which::which(rtk_binary_name()).unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|home| home.join(".local/bin").join(rtk_binary_name()))
            .unwrap_or_else(|| PathBuf::from(rtk_binary_name()))
    })
}

fn rtk_binary_name() -> &'static str {
    if cfg!(windows) {
        "rtk.exe"
    } else {
        "rtk"
    }
}

fn entry_is_whetstone_managed(entry: &Value, dir: &str, rtk_command: &str) -> bool {
    entry_is_whetstone_rtk_hook(entry, rtk_command) || entry_references_managed_script(entry, dir)
}

fn entry_is_whetstone_rtk_hook(entry: &Value, _rtk_command: &str) -> bool {
    let hooks = entry.get("hooks").and_then(Value::as_array);

    entry.get("matcher").and_then(Value::as_str) == Some("Bash")
        && hooks.is_some_and(|hooks| {
            hooks.len() == 1 && hook_command(&hooks[0]).is_some_and(is_rtk_claude_hook_command)
        })
}

fn entry_references_managed_script(entry: &Value, dir: &str) -> bool {
    let text = entry.to_string();
    MANAGED_HOOK_SCRIPTS
        .iter()
        .any(|name| text.contains(&format!("{dir}/{name}")))
}

fn hook_command(hook: &Value) -> Option<&str> {
    if hook.get("type").and_then(Value::as_str) != Some("command") {
        return None;
    }

    hook.get("command").and_then(Value::as_str)
}

fn is_rtk_claude_hook_command(command: &str) -> bool {
    rtk_hook_program(command).is_some_and(program_ends_with_rtk)
}

fn rtk_hook_program(command: &str) -> Option<&str> {
    let program = command.strip_suffix(" hook claude")?.trim();
    Some(program.trim_matches('"'))
}

fn program_ends_with_rtk(program: &str) -> bool {
    let path = Path::new(program);
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == rtk_binary_name())
}

fn shell_escape_program(program: &str) -> String {
    if program.contains([' ', '\t']) {
        format!("\"{}\"", program.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        program.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_RTK_COMMAND: &str = "/home/user/.local/bin/rtk hook claude";

    #[test]
    fn merge_into_empty_settings() {
        let existing = json!({});
        let result = build_hooks_value(
            &existing,
            "/home/user/.claude/hooks",
            MemoryProvider::Skip,
            TEST_RTK_COMMAND,
        );

        let hooks = &result["hooks"];
        assert!(hooks["PreToolUse"].is_array());
        assert_eq!(hooks["PreToolUse"].as_array().unwrap().len(), 3);
        assert_eq!(hooks["PostToolUse"].as_array().unwrap().len(), 1);
        assert_eq!(hooks["SessionStart"].as_array().unwrap().len(), 1);
        assert_eq!(hooks["Stop"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn merge_preserves_existing_keys() {
        let existing = json!({
            "apiKey": "sk-test",
            "model": "claude-opus-4-6"
        });
        let result = build_hooks_value(
            &existing,
            "/tmp/hooks",
            MemoryProvider::Icm,
            TEST_RTK_COMMAND,
        );

        assert_eq!(result["apiKey"], "sk-test");
        assert_eq!(result["model"], "claude-opus-4-6");
        assert!(result["hooks"].is_object());
    }

    #[test]
    fn hooks_use_absolute_rtk_claude_command() {
        let result = build_hooks_value(
            &json!({}),
            "/home/user/.claude/hooks",
            MemoryProvider::Skip,
            TEST_RTK_COMMAND,
        );

        let rtk_cmd = result["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(rtk_cmd, TEST_RTK_COMMAND);
    }

    #[test]
    fn whetstone_script_hooks_use_absolute_paths() {
        let result = build_hooks_value(
            &json!({}),
            "/home/user/.claude/hooks",
            MemoryProvider::Skip,
            TEST_RTK_COMMAND,
        );

        let notify_cmd = result["hooks"]["PreToolUse"][1]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(notify_cmd.starts_with("/home/user/.claude/hooks/"));
        assert!(notify_cmd.ends_with("pre-tool-notify.sh"));
    }

    #[test]
    fn merge_replaces_existing_rtk_hook_command() {
        let existing = json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "/tmp/old-rtk/bin/rtk hook claude",
                    }]
                }]
            }
        });
        let result = build_hooks_value(
            &existing,
            "/home/user/.claude/hooks",
            MemoryProvider::Skip,
            TEST_RTK_COMMAND,
        );

        let pre = result["hooks"]["PreToolUse"].as_array().unwrap();
        let rtk_count = pre
            .iter()
            .filter(|entry| entry.to_string().contains(TEST_RTK_COMMAND))
            .count();
        assert_eq!(rtk_count, 1);
    }

    #[test]
    fn merge_removes_legacy_rtk_rewrite_entry() {
        let existing = json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "/home/user/.claude/hooks/rtk-rewrite.sh",
                    }]
                }]
            }
        });
        let result = build_hooks_value(
            &existing,
            "/home/user/.claude/hooks",
            MemoryProvider::Skip,
            TEST_RTK_COMMAND,
        );

        let pre = result["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(pre
            .iter()
            .all(|entry| { !entry.to_string().contains(LEGACY_RTK_REWRITE_SCRIPT) }));
        assert!(pre
            .iter()
            .any(|entry| { entry.to_string().contains(TEST_RTK_COMMAND) }));
    }

    #[test]
    fn merge_preserves_custom_hook_in_claude_hooks_dir() {
        let existing = json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "/home/user/.claude/hooks/custom.sh",
                    }]
                }]
            }
        });
        let result = build_hooks_value(
            &existing,
            "/home/user/.claude/hooks",
            MemoryProvider::Skip,
            TEST_RTK_COMMAND,
        );

        let pre = result["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(pre.iter().any(|entry| {
            entry
                .to_string()
                .contains("/home/user/.claude/hooks/custom.sh")
        }));
    }

    #[test]
    fn merge_preserves_non_whetstone_hooks() {
        let existing = json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "/usr/local/bin/icm hook pre"
                    }]
                }],
                "PreCompact": [{
                    "hooks": [{
                        "type": "command",
                        "command": "/usr/local/bin/icm hook compact"
                    }]
                }]
            }
        });
        let result = build_hooks_value(
            &existing,
            "/home/user/.claude/hooks",
            MemoryProvider::Icm,
            TEST_RTK_COMMAND,
        );

        let pre = result["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(pre.iter().any(|e| e.to_string().contains("icm hook pre")));
        assert!(pre.iter().any(|e| e.to_string().contains(TEST_RTK_COMMAND)));

        assert!(result["hooks"]["PreCompact"].is_array());
    }

    #[test]
    fn automem_adds_mcp_server() {
        let result = build_hooks_value(
            &json!({}),
            "/home/user/.claude/hooks",
            MemoryProvider::AutoMem,
            TEST_RTK_COMMAND,
        );

        assert!(result["mcpServers"]["memory"].is_object());
        assert_eq!(result["mcpServers"]["memory"]["command"], "npx");
    }

    #[test]
    fn skip_provider_no_mcp_servers() {
        let result = build_hooks_value(
            &json!({}),
            "/home/user/.claude/hooks",
            MemoryProvider::Skip,
            TEST_RTK_COMMAND,
        );

        assert!(result.get("mcpServers").is_none());
    }
}
