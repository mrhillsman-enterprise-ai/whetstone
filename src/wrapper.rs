use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_PROXY: &str = "http://127.0.0.1:8787";
const DEFAULT_MODEL: &str = "claude-opus-4-6";

fn set_proxy_env() {
    if std::env::var("ANTHROPIC_BASE_URL").is_err() {
        std::env::set_var("ANTHROPIC_BASE_URL", DEFAULT_PROXY);
    }
}

fn has_model_flag(args: &[String]) -> bool {
    args.iter()
        .any(|a| a == "--model" || a.starts_with("--model="))
}

pub fn wrap_claude(args: &[String]) -> ! {
    set_proxy_env();

    let skip_rtk_setup = should_skip_headroom_rtk_setup();
    let cmd_args = build_claude_args(args, skip_rtk_setup);

    exec("headroom", &cmd_args);
}

fn build_claude_args(args: &[String], skip_rtk_setup: bool) -> Vec<String> {
    let mut cmd_args = vec!["wrap".to_string(), "claude".to_string()];

    if skip_rtk_setup {
        cmd_args.push("--no-rtk".into());
    }

    if !has_model_flag(args) {
        cmd_args.push("--model".into());
        cmd_args.push(DEFAULT_MODEL.into());
    }

    cmd_args.extend_from_slice(args);
    cmd_args
}

fn should_skip_headroom_rtk_setup() -> bool {
    rtk_exists_on_path() && global_headroom_rtk_hook_exists()
}

fn rtk_exists_on_path() -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&path_var).any(path_has_rtk_binary)
}

fn path_has_rtk_binary(dir: PathBuf) -> bool {
    let binary = dir.join(rtk_binary_name());
    binary.is_file() && path_is_executable(&binary)
}

fn rtk_binary_name() -> &'static str {
    if cfg!(windows) {
        "rtk.exe"
    } else {
        "rtk"
    }
}

fn global_headroom_rtk_hook_exists() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let settings_path = home.join(".claude/settings.json");
    let Ok(contents) = fs::read_to_string(settings_path) else {
        return false;
    };
    let Ok(settings) = serde_json::from_str::<Value>(&contents) else {
        return false;
    };

    settings_has_headroom_rtk_hook(&settings)
}

fn settings_has_headroom_rtk_hook(settings: &Value) -> bool {
    settings
        .get("hooks")
        .and_then(|hooks| hooks.get("PreToolUse"))
        .and_then(Value::as_array)
        .is_some_and(|entries| entries.iter().any(entry_has_headroom_rtk_hook))
}

fn entry_has_headroom_rtk_hook(entry: &Value) -> bool {
    matcher_allows_bash(entry)
        && entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| hooks.iter().any(command_is_headroom_rtk_hook))
}

fn matcher_allows_bash(entry: &Value) -> bool {
    entry
        .get("matcher")
        .and_then(Value::as_str)
        .is_some_and(|matcher| matcher.contains("Bash"))
}

fn command_is_headroom_rtk_hook(hook: &Value) -> bool {
    hook.get("type").and_then(Value::as_str) == Some("command")
        && hook.get("command").and_then(Value::as_str) == Some("rtk hook claude")
}

#[cfg(unix)]
fn path_is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn path_is_executable(path: &Path) -> bool {
    path.is_file()
}

pub fn wrap_proxy(args: &[String]) -> ! {
    set_proxy_env();
    exec("headroom", &[&["proxy".to_string()], args].concat());
}

pub fn wrap_rtk(args: &[String]) -> ! {
    set_proxy_env();
    exec("rtk", args);
}

#[cfg(unix)]
fn exec(program: &str, args: &[String]) -> ! {
    use std::os::unix::process::CommandExt;
    let err = Command::new(program).args(args).exec();
    eprintln!("[FAIL] failed to exec {program}: {err}");
    std::process::exit(127);
}

#[cfg(not(unix))]
fn exec(program: &str, args: &[String]) -> ! {
    let status = Command::new(program)
        .args(args)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("[FAIL] failed to run {program}: {e}");
            std::process::exit(127);
        });
    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn build_claude_args_adds_model_and_skips_rtk_when_ready() {
        let args = build_claude_args(&[], true);

        assert_eq!(
            args,
            strings(&["wrap", "claude", "--no-rtk", "--model", DEFAULT_MODEL,])
        );
    }

    #[test]
    fn build_claude_args_preserves_explicit_model() {
        let args = build_claude_args(&["--model".into(), "claude-sonnet".into()], false);

        assert_eq!(
            args,
            strings(&["wrap", "claude", "--model", "claude-sonnet"])
        );
    }

    #[test]
    fn settings_hook_detection_matches_headroom_hook() {
        let settings = json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "rtk hook claude"
                    }]
                }]
            }
        });

        assert!(settings_has_headroom_rtk_hook(&settings));
    }

    #[test]
    fn settings_hook_detection_rejects_non_headroom_hook() {
        let settings = json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "/tmp/rtk-rewrite.sh"
                    }]
                }]
            }
        });

        assert!(!settings_has_headroom_rtk_hook(&settings));
    }

    #[test]
    fn path_detection_finds_executable_rtk() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("whetstone-rtk-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        let binary = dir.join(rtk_binary_name());
        fs::write(
            &binary,
            "#!/bin/sh
exit 0
",
        )
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut perms = fs::metadata(&binary).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).unwrap();
        }

        assert!(path_has_rtk_binary(dir.clone()));
        let _ = fs::remove_file(&binary);
        let _ = fs::remove_dir(&dir);
    }
}
