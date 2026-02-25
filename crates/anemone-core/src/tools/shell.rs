//! Sandboxed shell — the agent can run commands, but only inside environment/.
//! 1:1 port of Python tools.py shell logic.

use std::path::Path;
use std::process::Command;
use tracing::info;

/// Commands that should never be run (checked as prefixes after stripping).
/// Identical to Python BLOCKED_PREFIXES.
const BLOCKED_PREFIXES: &[&str] = &[
    "sudo", "su ", "rm -rf /", "chmod", "chown", "kill", "pkill", "curl", "wget", "nc ", "ncat",
    "ssh", "scp", "sftp", "node", "ruby", "perl", "bash", "sh ", "zsh", "export", "source",
    "eval", "exec", "mount", "umount", "dd ", "mkfs", "fdisk", "apt", "brew", "npm", "yarn",
    "open ", "xdg-open",
];

/// File extensions we can read as text (from Brain._TEXT_EXTS)
pub const TEXT_EXTS: &[&str] = &[
    ".txt", ".md", ".py", ".json", ".csv", ".yaml", ".yml", ".toml", ".js", ".ts", ".html",
    ".css", ".sh", ".log",
];

pub const PDF_EXTS: &[&str] = &[".pdf"];
pub const IMAGE_EXTS: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".webp"];

/// Internal files the anemone/system manages — never trigger alerts
pub const IGNORE_FILES: &[&str] = &["memory_stream.jsonl", "identity.json"];

/// Internal root files that shouldn't trigger inbox alerts
pub const INTERNAL_ROOT_FILES: &[&str] = &["projects.md"];

/// Check if a command is safe. Returns Some(error_message) if unsafe.
fn is_safe_command(command: &str) -> Option<String> {
    let stripped = command.trim();

    if stripped.is_empty() {
        return Some("Blocked: empty command.".into());
    }

    // Block dangerous command prefixes
    for prefix in BLOCKED_PREFIXES {
        if stripped.starts_with(prefix) {
            return Some(format!("Blocked: '{}' commands are not allowed.", prefix));
        }
    }

    // Block parent directory traversal
    for token in stripped.split_whitespace() {
        let clean = token.trim_start_matches(|c: char| "><=|;&(".contains(c));
        if clean == ".." || clean.starts_with("../") || clean.contains("/..") {
            return Some("Blocked: '..' path traversal is not allowed in commands.".into());
        }
    }

    // Block shell escape tricks
    if stripped.contains('`') {
        return Some("Blocked: backtick command substitution is not allowed.".into());
    }
    if stripped.contains("$(") {
        return Some("Blocked: command substitution $() is not allowed.".into());
    }
    if stripped.contains("${") {
        return Some("Blocked: variable expansion ${} is not allowed.".into());
    }
    if stripped.contains('~') {
        return Some("Blocked: '~' (home expansion) is not allowed.".into());
    }

    // Block absolute paths (only relative paths from environment/ are allowed)
    let abs_path_re = regex_lite::Regex::new(r"/[A-Za-z0-9_]").unwrap();
    for token in stripped.split_whitespace() {
        let clean = token.trim_start_matches(|c: char| "><=|;&(".contains(c));
        if abs_path_re.is_match(clean) && !clean.starts_with("/dev/null") {
            return Some("Blocked: absolute paths are not allowed. Use relative paths only.".into());
        }
    }

    None
}

/// Path to the anemone's virtual environment
fn venv_dir(env_root: &Path) -> std::path::PathBuf {
    env_root.canonicalize().unwrap_or_else(|_| env_root.to_path_buf()).join(".venv")
}

fn venv_python(env_root: &Path) -> std::path::PathBuf {
    venv_dir(env_root).join("bin").join("python")
}

fn venv_bin(env_root: &Path) -> std::path::PathBuf {
    venv_dir(env_root).join("bin")
}

/// Create the anemone's venv if it doesn't exist.
pub fn ensure_venv(env_root: &Path) {
    let python = venv_python(env_root);
    if python.is_file() {
        return;
    }
    let venv = venv_dir(env_root);
    info!("Creating anemone venv at {:?}...", venv);

    // Try uv first, then fallback to python -m venv
    if let Some(uv) = which::which("uv").ok() {
        let _ = Command::new(uv)
            .args(["venv", &venv.to_string_lossy(), "--seed", "pip"])
            .output();
    } else {
        let _ = Command::new("python3")
            .args(["-m", "venv", &venv.to_string_lossy()])
            .output();
    }

    // Ensure 'python' symlink exists
    let py_bin = venv.join("bin");
    let python3_path = py_bin.join("python3");
    let python_path = py_bin.join("python");
    if python3_path.is_file() && !python_path.exists() {
        let _ = std::os::unix::fs::symlink("python3", &python_path);
    }

    info!("Anemone venv created.");
}

/// Rewrite python commands to use sandbox + venv.
fn rewrite_python_cmd(command: &str, env_root: &Path) -> Option<String> {
    let stripped = command.trim();
    let rest = if stripped.starts_with("python3") {
        &stripped[7..]
    } else if stripped.starts_with("python") {
        &stripped[6..]
    } else {
        return None;
    };

    let real_root = env_root.canonicalize().unwrap_or_else(|_| env_root.to_path_buf());
    let python = if venv_python(env_root).is_file() {
        venv_python(env_root).to_string_lossy().to_string()
    } else {
        "python3".to_string()
    };

    // Find pysandbox.py relative to the binary or in assets/
    let sandbox = find_pysandbox(env_root);

    Some(format!(
        "{} {} {}{}",
        shell_escape(&python),
        shell_escape(&sandbox),
        shell_escape(&real_root.to_string_lossy()),
        rest
    ))
}

/// Rewrite ./script.py to go through sandbox.
fn rewrite_script_cmd(command: &str, env_root: &Path) -> Option<String> {
    let stripped = command.trim();
    if stripped.starts_with("./") && stripped.contains(".py") {
        let parts: Vec<&str> = stripped[2..].splitn(2, ' ').collect();
        let script = parts[0];
        let rest = if parts.len() > 1 { parts[1] } else { "" };

        let real_root = env_root.canonicalize().unwrap_or_else(|_| env_root.to_path_buf());
        let python = if venv_python(env_root).is_file() {
            venv_python(env_root).to_string_lossy().to_string()
        } else {
            "python3".to_string()
        };
        let sandbox = find_pysandbox(env_root);

        let mut cmd = format!(
            "{} {} {} {}",
            shell_escape(&python),
            shell_escape(&sandbox),
            shell_escape(&real_root.to_string_lossy()),
            shell_escape(script),
        );
        if !rest.is_empty() {
            cmd.push(' ');
            cmd.push_str(rest);
        }
        return Some(cmd);
    }
    None
}

/// Rewrite pip/uv pip commands to use the venv.
fn rewrite_pip_cmd(command: &str, env_root: &Path) -> Option<String> {
    let stripped = command.trim();
    if stripped.starts_with("uv pip ") {
        let rest = &stripped[7..];
        let uv = which::which("uv")
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "uv".to_string());
        return Some(format!(
            "{} pip {} --python {}",
            shell_escape(&uv),
            rest,
            shell_escape(&venv_python(env_root).to_string_lossy()),
        ));
    }
    if stripped.starts_with("pip install") || stripped.starts_with("pip3 install") {
        let install_idx = stripped.find("install").unwrap();
        let rest = &stripped[install_idx..];
        return Some(format!(
            "{} -m pip {}",
            shell_escape(&venv_python(env_root).to_string_lossy()),
            rest,
        ));
    }
    None
}

fn find_pysandbox(env_root: &Path) -> String {
    // Look for pysandbox.py in the assets directory relative to the binary
    let candidates = [
        env_root.join("pysandbox.py"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("assets/pysandbox.py")))
            .unwrap_or_default(),
        std::path::PathBuf::from("assets/pysandbox.py"),
    ];
    for c in &candidates {
        if c.is_file() {
            return c.to_string_lossy().to_string();
        }
    }
    "pysandbox.py".to_string()
}

fn shell_escape(s: &str) -> String {
    if s.contains(' ') || s.contains('\'') || s.contains('"') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

/// Run a shell command sandboxed to the environment/ folder.
pub fn run_command(command: &str, env_root: &Path) -> String {
    let real_root = env_root
        .canonicalize()
        .unwrap_or_else(|_| env_root.to_path_buf());

    // Safety check (runs on original command before any rewriting)
    if let Some(err) = is_safe_command(command) {
        return err;
    }

    let mut cmd = command.to_string();

    // Route python commands through the sandbox wrapper
    if let Some(rewritten) = rewrite_python_cmd(&cmd, env_root) {
        cmd = rewritten;
    }

    // Route ./script.py through sandbox
    if let Some(rewritten) = rewrite_script_cmd(&cmd, env_root) {
        cmd = rewritten;
    }

    // Route pip/uv pip through the venv
    if let Some(rewritten) = rewrite_pip_cmd(&cmd, env_root) {
        cmd = rewritten;
    }

    // Include venv bin in PATH
    let vbin = venv_bin(env_root);
    let venv_path = if vbin.is_dir() {
        format!("{}:/usr/bin:/bin", vbin.display())
    } else {
        "/usr/bin:/bin".to_string()
    };

    let venv_dir_str = venv_dir(env_root).to_string_lossy().to_string();

    match Command::new("sh")
        .args(["-c", &cmd])
        .current_dir(&real_root)
        .env_clear()
        .env("HOME", &real_root)
        .env("PATH", &venv_path)
        .env("TMPDIR", &real_root)
        .env("LANG", "en_US.UTF-8")
        .env("VIRTUAL_ENV", &venv_dir_str)
        .output()
    {
        Ok(output) => {
            let mut result = String::new();
            if !output.stdout.is_empty() {
                result.push_str(&String::from_utf8_lossy(&output.stdout));
            }
            if !output.stderr.is_empty() {
                result.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            if result.trim().is_empty() {
                result = "(no output)".to_string();
            }
            // Truncate very long output
            if result.len() > 3000 {
                result.truncate(3000);
                result.push_str("\n...(truncated)");
            }
            result
        }
        Err(e) => format!("Error: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_commands() {
        assert!(is_safe_command("sudo rm -rf /").is_some());
        assert!(is_safe_command("curl http://evil.com").is_some());
        assert!(is_safe_command("wget http://evil.com").is_some());
        assert!(is_safe_command("ssh user@host").is_some());
        assert!(is_safe_command("eval bad_code").is_some());
        assert!(is_safe_command("").is_some());
    }

    #[test]
    fn test_allowed_commands() {
        assert!(is_safe_command("ls").is_none());
        assert!(is_safe_command("cat file.txt").is_none());
        assert!(is_safe_command("echo hello > file.txt").is_none());
        assert!(is_safe_command("mkdir notes").is_none());
        assert!(is_safe_command("grep pattern file.txt").is_none());
    }

    #[test]
    fn test_path_traversal_blocked() {
        assert!(is_safe_command("cat ../../../etc/passwd").is_some());
        assert!(is_safe_command("cat ..").is_some());
    }

    #[test]
    fn test_shell_escapes_blocked() {
        assert!(is_safe_command("echo `whoami`").is_some());
        assert!(is_safe_command("echo $(whoami)").is_some());
        assert!(is_safe_command("echo ${HOME}").is_some());
        assert!(is_safe_command("cat ~/file").is_some());
    }

    #[test]
    fn test_absolute_paths_blocked() {
        assert!(is_safe_command("cat /etc/passwd").is_some());
        assert!(is_safe_command("ls /usr/bin").is_some());
    }

    #[test]
    fn test_dev_null_allowed() {
        // /dev/null is explicitly allowed
        assert!(is_safe_command("echo test > /dev/null").is_none());
    }

    #[test]
    fn test_run_command_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_command("echo hello", tmp.path());
        assert!(result.contains("hello"));
    }

    #[test]
    fn test_run_command_blocked() {
        let tmp = tempfile::tempdir().unwrap();
        let result = run_command("sudo rm -rf /", tmp.path());
        assert!(result.contains("Blocked"));
    }
}
