use crate::cancel::CancelToken;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use walkdir::{DirEntry, WalkDir};

const IGNORED_DIRS: &[&str] = &[
    ".git", "node_modules", "target", "dist", "build", ".next", ".nuxt", ".venv", "venv",
    "__pycache__", ".idea", ".vscode", ".cache", "coverage", ".turbo", ".svelte-kit",
];
const MAX_READ_BYTES: u64 = 512 * 1024;
const MAX_READ_LINES: usize = 2000;
const MAX_OUTPUT_CHARS: usize = 30_000;
const MAX_LIST_ENTRIES: usize = 600;
const MAX_SEARCH_MATCHES: usize = 200;
const MAX_SEARCH_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct ToolOutput {
    pub ok: bool,
    pub output: String,
    /// Workspace-relative path of a file that was created/modified/deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_path: Option<String>,
}

impl ToolOutput {
    fn ok(output: impl Into<String>) -> Self {
        Self { ok: true, output: output.into(), changed_path: None }
    }

    fn changed(output: impl Into<String>, path: String) -> Self {
        Self { ok: true, output: output.into(), changed_path: Some(path) }
    }

    fn err(output: impl Into<String>) -> Self {
        Self { ok: false, output: output.into(), changed_path: None }
    }
}

type ToolResult = Result<ToolOutput, String>;

/// A workspace root. Every path a tool touches must resolve inside it.
pub struct Workspace {
    root: PathBuf,
}

/// `std::fs::canonicalize` on Windows yields `\\?\C:\...`; strip it for display.
pub fn display_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

pub fn canonical_dir(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path.trim());
    if !p.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }
    p.canonicalize().map_err(|e| format!("Cannot open {}: {}", path, e))
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn is_ignored_dir(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry.file_type().is_dir()
        && IGNORED_DIRS.contains(&entry.file_name().to_string_lossy().as_ref())
}

fn truncate_middle(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max / 2).collect();
    let tail: String = {
        let rev: Vec<char> = text.chars().rev().take(max / 2).collect();
        rev.into_iter().rev().collect()
    };
    let omitted = text.chars().count() - max;
    format!("{head}\n\n... [{omitted} characters truncated] ...\n\n{tail}")
}

/// Convert a simple glob (`*.ts`, `src/**/*.{js,vue}`) into a regex.
fn glob_to_regex(glob: &str) -> Option<Regex> {
    let mut re = String::from("^");
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    let mut in_group = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '*' if chars.get(i + 1) == Some(&'*') => {
                re.push_str(".*");
                i += 1;
                if chars.get(i + 1) == Some(&'/') {
                    i += 1;
                }
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '{' => {
                in_group = true;
                re.push('(');
            }
            '}' if in_group => {
                in_group = false;
                re.push(')');
            }
            ',' if in_group => re.push('|'),
            other => re.push_str(&regex::escape(&other.to_string())),
        }
        i += 1;
    }
    re.push('$');
    RegexBuilder::new(&re).case_insensitive(cfg!(windows)).build().ok()
}

fn str_arg<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required string argument `{key}`"))
}

fn opt_str<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty())
}

fn opt_u64(args: &serde_json::Value, key: &str) -> Option<u64> {
    args.get(key).and_then(|v| match v {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}

fn opt_bool(args: &serde_json::Value, key: &str) -> bool {
    args.get(key).is_some_and(|v| match v {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => s == "true",
        _ => false,
    })
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Resolve a model-supplied path inside the workspace, following symlinks
    /// on the longest existing ancestor so links cannot escape the root.
    pub fn resolve(&self, input: &str) -> Result<PathBuf, String> {
        let trimmed = input.trim().trim_matches(|c| c == '"' || c == '\'');
        let candidate = if trimmed.is_empty() || trimmed == "." || trimmed == "/" {
            self.root.clone()
        } else {
            let p = Path::new(trimmed);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.root.join(trimmed.trim_start_matches(['/', '\\']))
            }
        };
        let normalized = normalize(&candidate);

        let mut existing = normalized.clone();
        let mut rest: Vec<std::ffi::OsString> = Vec::new();
        // `Path::exists()` follows links and reports `false` for a *dangling*
        // symlink; `symlink_metadata` reports the link itself, so a broken link
        // is canonicalized (and rejected) instead of being written through.
        while existing.symlink_metadata().is_err() {
            match existing.file_name() {
                Some(name) => rest.push(name.to_os_string()),
                None => break,
            }
            if !existing.pop() {
                break;
            }
        }
        let mut resolved = existing
            .canonicalize()
            .map_err(|e| format!("Cannot resolve path `{input}`: {e}"))?;
        for part in rest.iter().rev() {
            resolved.push(part);
        }

        if !resolved.starts_with(&self.root) {
            return Err(format!(
                "Path `{input}` is outside the workspace. Use paths relative to the workspace root."
            ));
        }
        Ok(resolved)
    }

    pub fn relative(&self, path: &Path) -> String {
        let rel = path.strip_prefix(&self.root).unwrap_or(path);
        let s = rel.to_string_lossy().replace('\\', "/");
        if s.is_empty() { ".".to_string() } else { s }
    }

    pub async fn execute(
        &self,
        name: &str,
        args: &serde_json::Value,
        cancel: Arc<CancelToken>,
    ) -> ToolOutput {
        let result = match name {
            "list_directory" => self.list_directory(args),
            "read_file" => self.read_file(args),
            "write_file" => self.write_file(args),
            "edit_file" => self.edit_file(args),
            "search_files" => self.search_files(args),
            "delete_path" => self.delete_path(args),
            "run_command" => self.run_command(args, cancel).await,
            other => Err(format!("Unknown tool `{other}`")),
        };
        result.unwrap_or_else(ToolOutput::err)
    }

    fn list_directory(&self, args: &serde_json::Value) -> ToolResult {
        let dir = self.resolve(opt_str(args, "path").unwrap_or("."))?;
        if !dir.is_dir() {
            return Err(format!("`{}` is not a directory", self.relative(&dir)));
        }
        let recursive = opt_bool(args, "recursive");
        let max_depth = if recursive { 6 } else { 1 };

        let mut entries: Vec<String> = Vec::new();
        let mut truncated = false;
        let walker = WalkDir::new(&dir)
            .min_depth(1)
            .max_depth(max_depth)
            .sort_by(|a, b| {
                b.file_type()
                    .is_dir()
                    .cmp(&a.file_type().is_dir())
                    .then_with(|| a.file_name().cmp(b.file_name()))
            })
            .into_iter()
            .filter_entry(|e| !is_ignored_dir(e));

        for entry in walker.flatten() {
            if entries.len() >= MAX_LIST_ENTRIES {
                truncated = true;
                break;
            }
            let rel = self.relative(entry.path());
            if entry.file_type().is_dir() {
                entries.push(format!("{rel}/"));
            } else {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                entries.push(format!("{rel} ({})", human_size(size)));
            }
        }

        if entries.is_empty() {
            return Ok(ToolOutput::ok(format!("`{}` is empty.", self.relative(&dir))));
        }
        let mut out = entries.join("\n");
        if truncated {
            out.push_str(&format!("\n... (listing truncated at {MAX_LIST_ENTRIES} entries)"));
        }
        Ok(ToolOutput::ok(out))
    }

    fn read_file(&self, args: &serde_json::Value) -> ToolResult {
        let path = self.resolve(str_arg(args, "path")?)?;
        let rel = self.relative(&path);
        if !path.is_file() {
            return Err(format!("File not found: `{rel}`"));
        }
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        if size > MAX_READ_BYTES * 8 {
            return Err(format!(
                "`{rel}` is {} — too large to read. Use search_files to find the relevant part.",
                human_size(size)
            ));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("Cannot read `{rel}`: {e}"))?;
        if bytes.iter().take(8000).any(|b| *b == 0) {
            return Err(format!("`{rel}` looks like a binary file."));
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let total = lines.len();

        let start = opt_u64(args, "start_line").unwrap_or(1).max(1) as usize;
        let requested_end = opt_u64(args, "end_line").map(|n| n as usize).unwrap_or(total);

        if total == 0 {
            return Ok(ToolOutput::ok(format!("`{rel}` is empty.")));
        }
        if start > total {
            return Err(format!("start_line {start} is past the end of `{rel}` ({total} lines)"));
        }
        // A reversed or zero range would make the slice below panic; reject it
        // instead so the model gets a recoverable error.
        if requested_end < start {
            return Err(format!(
                "end_line {requested_end} is before start_line {start}. Use a 1-based inclusive range such as start_line={start}&end_line={}.",
                start.saturating_add(MAX_READ_LINES - 1).min(total)
            ));
        }
        let end = requested_end
            .min(total)
            .min(start.saturating_add(MAX_READ_LINES - 1));

        let body = lines[start - 1..end].join("\n");
        let header = if start == 1 && end == total {
            format!("`{rel}` ({total} lines)")
        } else {
            format!("`{rel}` lines {start}-{end} of {total}")
        };
        let mut out = format!("{header}\n{body}");
        if end < total && requested_end > end {
            out.push_str(&format!(
                "\n... [{} more lines — call read_file with start_line={}]",
                total - end,
                end + 1
            ));
        }
        Ok(ToolOutput::ok(out))
    }

    fn write_file(&self, args: &serde_json::Value) -> ToolResult {
        let path = self.resolve(str_arg(args, "path")?)?;
        let content = str_arg(args, "content")?;
        let rel = self.relative(&path);
        if path.is_dir() {
            return Err(format!("`{rel}` is a directory"));
        }
        let existed = path.exists();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create directory: {e}"))?;
        }
        std::fs::write(&path, content).map_err(|e| format!("Cannot write `{rel}`: {e}"))?;
        let lines = content.lines().count();
        let verb = if existed { "Overwrote" } else { "Created" };
        Ok(ToolOutput::changed(format!("{verb} `{rel}` ({lines} lines)"), rel))
    }

    fn edit_file(&self, args: &serde_json::Value) -> ToolResult {
        let path = self.resolve(str_arg(args, "path")?)?;
        let old = str_arg(args, "old_string")?;
        let new = str_arg(args, "new_string")?;
        let replace_all = opt_bool(args, "replace_all");
        let rel = self.relative(&path);

        if !path.is_file() {
            return Err(format!("File not found: `{rel}`. Use write_file to create it."));
        }
        if old.is_empty() {
            return Err("old_string must not be empty".to_string());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read `{rel}`: {e}"))?;

        // Models usually emit `\n`; match CRLF files transparently. Guard both
        // sides so an already-CRLF replacement cannot gain a second `\r`.
        let (old, new) = if !text.contains(old) && text.contains("\r\n") && !old.contains('\r') {
            (
                old.replace('\n', "\r\n"),
                if new.contains('\r') { new.to_string() } else { new.replace('\n', "\r\n") },
            )
        } else {
            (old.to_string(), new.to_string())
        };

        let count = text.matches(old.as_str()).count();
        if count == 0 {
            return Err(format!(
                "old_string was not found in `{rel}`. Read the file again and copy the exact text, including whitespace."
            ));
        }
        if count > 1 && !replace_all {
            return Err(format!(
                "old_string matches {count} places in `{rel}`. Include more surrounding context to make it unique, or set replace_all=true."
            ));
        }
        let updated = if replace_all {
            text.replace(old.as_str(), &new)
        } else {
            text.replacen(old.as_str(), &new, 1)
        };
        std::fs::write(&path, &updated).map_err(|e| format!("Cannot write `{rel}`: {e}"))?;
        let plural = if count == 1 { "" } else { "s" };
        Ok(ToolOutput::changed(format!("Edited `{rel}` ({count} replacement{plural})"), rel))
    }

    fn search_files(&self, args: &serde_json::Value) -> ToolResult {
        let pattern = str_arg(args, "pattern")?;
        let base = self.resolve(opt_str(args, "path").unwrap_or("."))?;
        let include = opt_str(args, "include").and_then(glob_to_regex);

        let regex = RegexBuilder::new(pattern)
            .case_insensitive(opt_bool(args, "case_insensitive"))
            .build()
            .or_else(|_| Regex::new(&regex::escape(pattern)))
            .map_err(|e| format!("Invalid pattern: {e}"))?;

        let mut matches: Vec<String> = Vec::new();
        let mut files_matched = 0usize;
        'files: for entry in WalkDir::new(&base)
            .into_iter()
            .filter_entry(|e| !is_ignored_dir(e))
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = self.relative(entry.path());
            if let Some(glob) = &include {
                let name = entry.file_name().to_string_lossy();
                if !glob.is_match(&rel) && !glob.is_match(&name) {
                    continue;
                }
            }
            if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_SEARCH_FILE_BYTES {
                continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else { continue };
            if bytes.iter().take(8000).any(|b| *b == 0) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let mut file_hit = false;
            for (i, line) in text.lines().enumerate() {
                if regex.is_match(line) {
                    if !file_hit {
                        files_matched += 1;
                        file_hit = true;
                    }
                    let snippet: String = line.trim().chars().take(240).collect();
                    matches.push(format!("{rel}:{}: {snippet}", i + 1));
                    if matches.len() >= MAX_SEARCH_MATCHES {
                        break 'files;
                    }
                }
            }
        }

        if matches.is_empty() {
            return Ok(ToolOutput::ok(format!("No matches for `{pattern}`.")));
        }
        let mut out = format!("{} matches in {files_matched} files:\n", matches.len());
        out.push_str(&matches.join("\n"));
        if matches.len() >= MAX_SEARCH_MATCHES {
            out.push_str("\n... (results truncated; narrow the pattern or path)");
        }
        Ok(ToolOutput::ok(out))
    }

    fn delete_path(&self, args: &serde_json::Value) -> ToolResult {
        let path = self.resolve(str_arg(args, "path")?)?;
        let rel = self.relative(&path);
        if path == self.root {
            return Err("Refusing to delete the workspace root".to_string());
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| format!("Cannot delete `{rel}`: {e}"))?;
        } else if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Cannot delete `{rel}`: {e}"))?;
        } else {
            return Err(format!("`{rel}` does not exist"));
        }
        Ok(ToolOutput::changed(format!("Deleted `{rel}`"), rel))
    }

    async fn run_command(&self, args: &serde_json::Value, cancel: Arc<CancelToken>) -> ToolResult {
        let command = str_arg(args, "command")?;
        let cwd = self.resolve(opt_str(args, "cwd").unwrap_or("."))?;
        if !cwd.is_dir() {
            return Err(format!("cwd `{}` is not a directory", self.relative(&cwd)));
        }
        let timeout = Duration::from_secs(opt_u64(args, "timeout_secs").unwrap_or(120).clamp(1, 900));

        let mut cmd = shell_command(command);
        cmd.current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = cmd.spawn().map_err(|e| format!("Failed to start shell: {e}"))?;
        let output = tokio::select! {
            out = tokio::time::timeout(timeout, child.wait_with_output()) => match out {
                Ok(result) => result.map_err(|e| format!("Command failed: {e}"))?,
                Err(_) => return Err(format!("Command timed out after {}s and was killed", timeout.as_secs())),
            },
            _ = cancel.cancelled() => return Err("Command was cancelled by the user".to_string()),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().map(|c| c.to_string()).unwrap_or_else(|| "killed".into());

        let mut out = format!("Exit code: {code}\n");
        if !stdout.trim().is_empty() {
            out.push_str(&format!("--- stdout ---\n{}\n", stdout.trim_end()));
        }
        if !stderr.trim().is_empty() {
            out.push_str(&format!("--- stderr ---\n{}\n", stderr.trim_end()));
        }
        if stdout.trim().is_empty() && stderr.trim().is_empty() {
            out.push_str("(no output)");
        }
        let out = truncate_middle(&out, MAX_OUTPUT_CHARS);
        Ok(if output.status.success() { ToolOutput::ok(out) } else { ToolOutput::err(out) })
    }
}

#[cfg(windows)]
fn shell_command(command: &str) -> tokio::process::Command {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = tokio::process::Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &format!(
            "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; $ProgressPreference='SilentlyContinue'; {command}"
        ),
    ]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
fn shell_command(command: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.args(["-c", command]);
    cmd
}

fn human_size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

#[derive(Debug, Deserialize)]
pub struct ToolInvocation {
    pub name: String,
    /// Raw JSON arguments string as produced by the model.
    pub arguments: String,
}

/// Parse model-produced arguments; an empty string means "no arguments".
pub fn parse_arguments(raw: &str) -> Result<serde_json::Value, String> {
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(raw).map_err(|e| format!("Tool arguments are not valid JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        (dir, Workspace::new(root))
    }

    async fn run(ws: &Workspace, name: &str, args: serde_json::Value) -> ToolOutput {
        ws.execute(name, &args, Arc::new(CancelToken::default())).await
    }

    #[tokio::test]
    async fn write_read_edit_roundtrip() {
        let (_dir, ws) = workspace();
        let out = run(&ws, "write_file", json!({"path": "src/main.rs", "content": "fn main() {\n    println!(\"hi\");\n}\n"})).await;
        assert!(out.ok, "{}", out.output);
        assert_eq!(out.changed_path.as_deref(), Some("src/main.rs"));

        let out = run(&ws, "read_file", json!({"path": "src/main.rs"})).await;
        assert!(out.output.contains("println!(\"hi\")"));

        let out = run(&ws, "edit_file", json!({"path": "src/main.rs", "old_string": "\"hi\"", "new_string": "\"bye\""})).await;
        assert!(out.ok, "{}", out.output);
        let text = std::fs::read_to_string(ws.resolve("src/main.rs").unwrap()).unwrap();
        assert!(text.contains("\"bye\""));
    }

    #[tokio::test]
    async fn edit_requires_unique_match() {
        let (_dir, ws) = workspace();
        run(&ws, "write_file", json!({"path": "a.txt", "content": "x\nx\n"})).await;
        let out = run(&ws, "edit_file", json!({"path": "a.txt", "old_string": "x", "new_string": "y"})).await;
        assert!(!out.ok);
        let out = run(&ws, "edit_file", json!({"path": "a.txt", "old_string": "x", "new_string": "y", "replace_all": true})).await;
        assert!(out.ok);
    }

    #[tokio::test]
    async fn edit_matches_crlf_files() {
        let (_dir, ws) = workspace();
        std::fs::write(ws.root.join("w.txt"), "one\r\ntwo\r\n").unwrap();
        let out = run(&ws, "edit_file", json!({"path": "w.txt", "old_string": "one\ntwo", "new_string": "1\n2"})).await;
        assert!(out.ok, "{}", out.output);
        assert_eq!(std::fs::read_to_string(ws.root.join("w.txt")).unwrap(), "1\r\n2\r\n");
    }

    #[test]
    fn paths_cannot_escape_workspace() {
        let (_dir, ws) = workspace();
        assert!(ws.resolve("../outside.txt").is_err());
        assert!(ws.resolve("a/../../outside.txt").is_err());
        let abs = if cfg!(windows) { "C:\\Windows\\win.ini" } else { "/etc/passwd" };
        assert!(ws.resolve(abs).is_err());
        assert!(ws.resolve("a/../b.txt").is_ok());
        assert!(ws.resolve("/nested/file.txt").is_ok(), "leading slash is workspace-relative");
    }

    #[tokio::test]
    async fn list_and_search_skip_ignored_dirs() {
        let (_dir, ws) = workspace();
        run(&ws, "write_file", json!({"path": "src/app.ts", "content": "const needle = 1;"})).await;
        run(&ws, "write_file", json!({"path": "node_modules/x/index.js", "content": "needle"})).await;

        let out = run(&ws, "list_directory", json!({"recursive": true})).await;
        assert!(out.output.contains("src/app.ts"));
        assert!(!out.output.contains("index.js"));

        let out = run(&ws, "search_files", json!({"pattern": "needle", "include": "*.ts"})).await;
        assert!(out.output.contains("src/app.ts:1"));
        assert!(!out.output.contains("node_modules"));
    }

    #[tokio::test]
    async fn delete_refuses_root() {
        let (_dir, ws) = workspace();
        assert!(!run(&ws, "delete_path", json!({"path": "."})).await.ok);
    }

    #[tokio::test]
    async fn run_command_captures_output() {
        let (_dir, ws) = workspace();
        let out = run(&ws, "run_command", json!({"command": "echo kilo"})).await;
        assert!(out.ok, "{}", out.output);
        assert!(out.output.contains("kilo"));
    }

    #[tokio::test]
    async fn reversed_read_range_is_rejected_not_panicking() {
        let (_dir, ws) = workspace();
        run(&ws, "write_file", json!({"path": "ten.txt", "content": "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n"})).await;
        let out = run(&ws, "read_file", json!({"path": "ten.txt", "start_line": 5, "end_line": 3})).await;
        assert!(!out.ok, "a reversed range must be an error, got: {}", out.output);
        let out = run(&ws, "read_file", json!({"path": "ten.txt", "start_line": 2, "end_line": 0})).await;
        assert!(!out.ok, "end_line 0 must be an error, got: {}", out.output);
    }

    #[tokio::test]
    async fn huge_start_line_does_not_overflow() {
        let (_dir, ws) = workspace();
        run(&ws, "write_file", json!({"path": "a.txt", "content": "one\ntwo\n"})).await;
        let out = run(&ws, "read_file", json!({"path": "a.txt", "start_line": u64::MAX})).await;
        assert!(!out.ok, "start past the end must be an error, got: {}", out.output);
    }

    #[tokio::test]
    async fn edit_does_not_double_carriage_return() {
        let (_dir, ws) = workspace();
        std::fs::write(ws.root.join("c.txt"), "one\r\ntwo\r\n").unwrap();
        // old_string uses LF (the common model output) while new_string already has CRLF.
        let out = run(&ws, "edit_file", json!({"path": "c.txt", "old_string": "one\ntwo", "new_string": "1\r\n2"})).await;
        assert!(out.ok, "{}", out.output);
        let text = std::fs::read_to_string(ws.root.join("c.txt")).unwrap();
        assert_eq!(text, "1\r\n2\r\n", "must not produce \\r\\r\\n");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn dangling_symlink_cannot_escape_workspace() {
        let (_dir, ws) = workspace();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("created-outside.txt");
        let link = ws.root.join("escape.txt");
        // A link whose target does not exist yet: `Path::exists()` reports false.
        if std::os::windows::fs::symlink_file(&target, &link).is_err() {
            return; // creating symlinks needs Developer Mode or elevation
        }
        let out = run(&ws, "write_file", json!({"path": "escape.txt", "content": "x"})).await;
        assert!(!out.ok, "writing through a dangling link must be refused, got: {}", out.output);
        assert!(!target.exists(), "the file must not be created outside the workspace");
    }

    #[test]
    fn glob_matching() {
        let g = glob_to_regex("*.{ts,vue}").unwrap();
        assert!(g.is_match("App.vue"));
        assert!(g.is_match("main.ts"));
        assert!(!g.is_match("main.rs"));
        let g = glob_to_regex("src/**/*.rs").unwrap();
        assert!(g.is_match("src/a/b/c.rs"));
        assert!(g.is_match("src/c.rs"));
    }
}
