use serde_json::Value;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_PROXY: &str = "http://127.0.0.1:8787";
const DEFAULT_MODEL: &str = "claude-opus-4-6";
const LATEST_MODEL: &str = "claude-opus-4-7";

fn set_proxy_env() {
    if std::env::var("ANTHROPIC_BASE_URL").is_err() {
        std::env::set_var("ANTHROPIC_BASE_URL", DEFAULT_PROXY);
    }
}

fn has_model_flag(args: &[String]) -> bool {
    args.iter()
        .any(|a| a == "--model" || a.starts_with("--model="))
}

fn read_model_from_settings(path: &Path) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let settings: Value = serde_json::from_str(&contents).ok()?;
    settings
        .get("model")
        .and_then(Value::as_str)
        .map(String::from)
}

fn effective_model_from(project_settings: &Path, global_settings: &Path) -> String {
    if let Some(model) = read_model_from_settings(project_settings) {
        return model;
    }
    if let Some(model) = read_model_from_settings(global_settings) {
        return model;
    }
    DEFAULT_MODEL.to_string()
}

fn effective_model() -> String {
    let project = Path::new(".claude/settings.local.json");
    let global = dirs::home_dir()
        .map(|h| h.join(".claude/settings.json"))
        .unwrap_or_default();
    effective_model_from(project, &global)
}

fn write_model_to_settings(path: &Path, model: &str) {
    let mut settings: Value = path
        .exists()
        .then(|| fs::read_to_string(path).ok())
        .flatten()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

    settings["model"] = Value::String(model.to_string());

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(&settings) {
        let _ = fs::write(path, json);
    }
}

fn has_model_value(args: &[String], model: &str) -> bool {
    if args.iter().any(|a| a == &format!("--model={model}")) {
        return true;
    }
    args.windows(2).any(|w| w[0] == "--model" && w[1] == model)
}

fn maybe_prompt_model_upgrade(args: &[String]) -> Vec<String> {
    if !crate::ui::is_interactive() {
        return vec![];
    }

    if has_model_value(args, LATEST_MODEL) {
        return vec![];
    }

    let current = effective_model();
    if current == LATEST_MODEL {
        return vec![];
    }

    crate::ui::warn(&format!(
        "Current model: {current} (latest is {LATEST_MODEL})"
    ));

    let choices = [
        format!("Keep {current} (continue)"),
        format!("Use {LATEST_MODEL} this session only"),
        format!("Set {LATEST_MODEL} project-wide (.claude/settings.local.json)"),
        format!("Set {LATEST_MODEL} globally (~/.claude/settings.json)"),
    ];

    let selected = crate::ui::select("Model selection:", &choices, 0);

    match selected {
        1 => {
            vec!["--model".into(), LATEST_MODEL.into()]
        }
        2 => {
            let path = Path::new(".claude/settings.local.json");
            write_model_to_settings(path, LATEST_MODEL);
            crate::ui::ok(&format!("Set model={LATEST_MODEL} in {}", path.display()));
            vec!["--model".into(), LATEST_MODEL.into()]
        }
        3 => {
            if let Some(home) = dirs::home_dir() {
                let path = home.join(".claude/settings.json");
                write_model_to_settings(&path, LATEST_MODEL);
                crate::ui::ok(&format!("Set model={LATEST_MODEL} in {}", path.display()));
            }
            vec!["--model".into(), LATEST_MODEL.into()]
        }
        _ => vec![],
    }
}

pub fn wrap_claude(args: &[String]) -> ! {
    set_proxy_env();

    let extra = maybe_prompt_model_upgrade(args);
    let mut all_args: Vec<String> = args.to_vec();
    all_args.extend(extra);

    let skip_rtk_setup = should_skip_headroom_rtk_setup();
    let cmd_args = build_claude_args(&all_args, skip_rtk_setup);

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
    global_headroom_rtk_hook_exists()
}

fn rtk_exists_on_path(path_var: Option<&OsStr>) -> bool {
    let Some(path_var) = path_var else {
        return false;
    };

    env::split_paths(path_var).any(path_has_rtk_binary)
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
    settings_has_headroom_rtk_hook_for(settings, env::var_os("PATH").as_deref())
}

fn settings_has_headroom_rtk_hook_for(settings: &Value, path_var: Option<&OsStr>) -> bool {
    settings
        .get("hooks")
        .and_then(|hooks| hooks.get("PreToolUse"))
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry_has_headroom_rtk_hook(entry, path_var))
        })
}

fn entry_has_headroom_rtk_hook(entry: &Value, path_var: Option<&OsStr>) -> bool {
    matcher_allows_bash(entry)
        && entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| {
                hooks
                    .iter()
                    .any(|hook| command_is_headroom_rtk_hook(hook, path_var))
            })
}

fn matcher_allows_bash(entry: &Value) -> bool {
    entry
        .get("matcher")
        .and_then(Value::as_str)
        .is_some_and(|matcher| matcher.contains("Bash"))
}

fn command_is_headroom_rtk_hook(hook: &Value, path_var: Option<&OsStr>) -> bool {
    hook.get("type").and_then(Value::as_str) == Some("command")
        && hook
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| rtk_hook_command_is_usable(command, path_var))
}

fn rtk_hook_command_is_usable(command: &str, path_var: Option<&OsStr>) -> bool {
    let Some(program) = rtk_hook_program(command) else {
        return false;
    };

    if !program_ends_with_rtk(program) {
        return false;
    }

    if is_bare_rtk_program(program) {
        return rtk_exists_on_path(path_var);
    }

    let path = Path::new(program);
    path.is_file() && path_is_executable(path)
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

fn is_bare_rtk_program(program: &str) -> bool {
    program == rtk_binary_name()
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
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn create_fake_rtk_binary() -> (PathBuf, PathBuf) {
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

        (dir, binary)
    }

    fn cleanup_fake_rtk(dir: &Path, binary: &Path) {
        let _ = fs::remove_file(binary);
        let _ = fs::remove_dir(dir);
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
        let (dir, binary) = create_fake_rtk_binary();
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

        assert!(settings_has_headroom_rtk_hook_for(
            &settings,
            Some(dir.as_os_str()),
        ));
        cleanup_fake_rtk(&dir, &binary);
    }

    #[test]
    fn settings_hook_detection_matches_absolute_rtk_hook_path() {
        let (dir, binary) = create_fake_rtk_binary();
        let settings = json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": format!("{} hook claude", binary.display())
                    }]
                }]
            }
        });

        assert!(settings_has_headroom_rtk_hook(&settings));
        cleanup_fake_rtk(&dir, &binary);
    }

    #[test]
    fn settings_hook_detection_rejects_missing_absolute_rtk_hook() {
        let settings = json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "/tmp/missing/rtk hook claude"
                    }]
                }]
            }
        });

        assert!(!settings_has_headroom_rtk_hook(&settings));
    }

    #[test]
    fn settings_hook_detection_rejects_non_rtk_hook() {
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
        let (dir, binary) = create_fake_rtk_binary();

        assert!(path_has_rtk_binary(dir.clone()));
        cleanup_fake_rtk(&dir, &binary);
    }

    fn temp_dir_with_stamp(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("whetstone-{label}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn effective_model_prefers_project_over_global() {
        let dir = temp_dir_with_stamp("model-prio");
        let project = dir.join("project.json");
        let global = dir.join("global.json");

        fs::write(&project, r#"{"model":"claude-opus-4-7"}"#).unwrap();
        fs::write(&global, r#"{"model":"claude-opus-4-6"}"#).unwrap();

        assert_eq!(effective_model_from(&project, &global), "claude-opus-4-7");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn effective_model_falls_back_to_global() {
        let dir = temp_dir_with_stamp("model-global");
        let project = dir.join("missing.json");
        let global = dir.join("global.json");

        fs::write(&global, r#"{"model":"claude-sonnet-4-6"}"#).unwrap();

        assert_eq!(effective_model_from(&project, &global), "claude-sonnet-4-6");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn effective_model_falls_back_to_default() {
        let dir = temp_dir_with_stamp("model-default");
        let project = dir.join("missing1.json");
        let global = dir.join("missing2.json");

        assert_eq!(effective_model_from(&project, &global), DEFAULT_MODEL);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn has_model_value_detects_equals_form() {
        assert!(has_model_value(
            &strings(&["--model=claude-opus-4-7"]),
            "claude-opus-4-7"
        ));
    }

    #[test]
    fn has_model_value_detects_space_form() {
        assert!(has_model_value(
            &strings(&["--model", "claude-opus-4-7"]),
            "claude-opus-4-7"
        ));
    }

    #[test]
    fn has_model_value_rejects_different_model() {
        assert!(!has_model_value(
            &strings(&["--model", "claude-sonnet-4-6"]),
            "claude-opus-4-7"
        ));
    }

    #[test]
    fn write_model_creates_new_file() {
        let dir = temp_dir_with_stamp("model-write");
        let path = dir.join("new-settings.json");

        write_model_to_settings(&path, "claude-opus-4-7");

        let content: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(content["model"], "claude-opus-4-7");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_model_preserves_existing_keys() {
        let dir = temp_dir_with_stamp("model-preserve");
        let path = dir.join("existing.json");

        fs::write(
            &path,
            r#"{"apiKey":"sk-test","theme":"dark","model":"claude-opus-4-6"}"#,
        )
        .unwrap();

        write_model_to_settings(&path, "claude-opus-4-7");

        let content: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(content["model"], "claude-opus-4-7");
        assert_eq!(content["apiKey"], "sk-test");
        assert_eq!(content["theme"], "dark");

        let _ = fs::remove_dir_all(&dir);
    }
}
