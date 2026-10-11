use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::Command,
    sync::Mutex,
    thread,
    time::UNIX_EPOCH,
};
use tauri::{Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    workspace: Mutex<Option<PathBuf>>,
    terminals: Mutex<HashMap<String, Terminal>>,
}
struct Terminal {
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[derive(Serialize)]
struct Entry {
    name: String,
    path: String,
    is_dir: bool,
}
#[derive(Serialize)]
struct FileSnapshot {
    content: String,
    version: String,
}
#[derive(Serialize)]
struct OpenTarget {
    path: String,
    workspace: String,
    needs_workspace: bool,
}
#[derive(Serialize)]
struct GitWorktree {
    path: String,
    head: String,
    branch: Option<String>,
    detached: bool,
    current: bool,
}
#[derive(Serialize)]
struct GitWorktrees {
    repository: String,
    worktrees: Vec<GitWorktree>,
}
#[derive(Serialize)]
struct GitCommit {
    hash: String,
    parents: Vec<String>,
    author: String,
    date: String,
    subject: String,
    refs: Vec<String>,
}
#[derive(Serialize)]
struct GitChange {
    path: String,
    index_status: String,
    worktree_status: String,
    untracked: bool,
}

fn repository_for_workspace(state: &State<'_, AppState>) -> Result<PathBuf, String> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Open a project folder first")?;
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err("This folder is not inside a Git repository".into());
    }
    PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
        .canonicalize()
        .map_err(|e| e.to_string())
}

fn git_changes_for(repository: &std::path::Path) -> Result<Vec<GitChange>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["status", "--porcelain=v1", "--untracked-files=all", "-z"])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let mut entries = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .peekable();
    let mut changes = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let index_status = entry[0] as char;
        let worktree_status = entry[1] as char;
        let path = String::from_utf8_lossy(&entry[3..]).to_string();
        let untracked = index_status == '?' && worktree_status == '?';
        changes.push(GitChange {
            path,
            index_status: index_status.to_string(),
            worktree_status: worktree_status.to_string(),
            untracked,
        });
        if matches!(index_status, 'R' | 'C') || matches!(worktree_status, 'R' | 'C') {
            let _ = entries.next();
        }
    }
    Ok(changes)
}

fn literal_pathspec(path: &str) -> Result<String, String> {
    let path = PathBuf::from(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("Invalid repository-relative path".into());
    }
    Ok(format!(":(literal){}", path.to_string_lossy()))
}

fn file_version(path: &std::path::Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    let modified = metadata
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "{}:{}:{}",
        metadata.len(),
        modified.as_secs(),
        modified.subsec_nanos()
    ))
}

fn authorized(root: &Option<PathBuf>, raw: &str) -> Result<PathBuf, String> {
    let root = root.as_ref().ok_or("Open a project folder first")?;
    let candidate = PathBuf::from(raw);
    let canonical = candidate.canonicalize().map_err(|e| e.to_string())?;
    let base = root.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&base) {
        return Err("Path is outside the open project".into());
    }
    Ok(canonical)
}

#[tauri::command]
fn set_workspace(path: String, state: State<'_, AppState>) -> Result<String, String> {
    let canonical = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !canonical.is_dir() {
        return Err("Selected workspace is not a directory".into());
    }
    *state.workspace.lock().map_err(|e| e.to_string())? = Some(canonical.clone());
    Ok(dunce::simplified(&canonical).to_string_lossy().to_string())
}

#[tauri::command]
fn git_worktrees(state: State<'_, AppState>) -> Result<GitWorktrees, String> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Open a project folder first")?;
    let workspace = workspace.canonicalize().map_err(|e| e.to_string())?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err("This folder is not inside a Git repository".into());
    }
    let repository = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let repository_path = PathBuf::from(&repository)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository_path)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let output = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut path: Option<String> = None;
    let mut head = String::new();
    let mut branch = None;
    let mut detached = false;
    let push_worktree = |worktrees: &mut Vec<GitWorktree>, path: &mut Option<String>, head: &mut String, branch: &mut Option<String>, detached: &mut bool| {
        if let Some(path) = path.take() {
            let path_buf = PathBuf::from(&path);
            let current = path_buf.canonicalize().map(|p| workspace.starts_with(p)).unwrap_or(false);
            worktrees.push(GitWorktree {
                path,
                head: std::mem::take(head),
                branch: branch.take(),
                detached: std::mem::replace(detached, false),
                current,
            });
        }
    };
    for line in output.lines() {
        if line.is_empty() {
            push_worktree(&mut worktrees, &mut path, &mut head, &mut branch, &mut detached);
        } else if let Some(value) = line.strip_prefix("worktree ") {
            path = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("HEAD ") {
            head = value.to_string();
        } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
            branch = Some(value.to_string());
        } else if line == "detached" {
            detached = true;
        }
    }
    push_worktree(&mut worktrees, &mut path, &mut head, &mut branch, &mut detached);
    Ok(GitWorktrees {
        repository: dunce::simplified(&repository_path).to_string_lossy().to_string(),
        worktrees,
    })
}

#[tauri::command]
fn git_history(state: State<'_, AppState>) -> Result<Vec<GitCommit>, String> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Open a project folder first")?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err("This folder is not inside a Git repository".into());
    }
    let repository = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args([
            "for-each-ref",
            "--format=%(refname:short)%00%(objectname)",
            "refs/heads",
            "refs/remotes",
        ])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let mut refs_by_hash: HashMap<String, Vec<String>> = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((name, hash)) = line.split_once('\0') {
            refs_by_hash
                .entry(hash.to_string())
                .or_default()
                .push(name.to_string());
        }
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args([
            "log",
            "--all",
            "HEAD",
            "--topo-order",
            "-n",
            "300",
            "--format=%H%x1f%P%x1f%an%x1f%ar%x1f%s",
        ])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let mut commits = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<_> = line.splitn(5, '\u{1f}').collect();
        if fields.len() != 5 {
            continue;
        }
        commits.push(GitCommit {
            hash: fields[0].to_string(),
            parents: fields[1].split_whitespace().map(str::to_string).collect(),
            author: fields[2].to_string(),
            date: fields[3].to_string(),
            subject: fields[4].to_string(),
            refs: refs_by_hash.remove(fields[0]).unwrap_or_default(),
        });
    }
    Ok(commits)
}

#[tauri::command]
fn git_changes(state: State<'_, AppState>) -> Result<Vec<GitChange>, String> {
    let repository = repository_for_workspace(&state)?;
    git_changes_for(&repository)
}

#[tauri::command]
fn git_file_diff(path: String, staged: bool, state: State<'_, AppState>) -> Result<String, String> {
    let repository = repository_for_workspace(&state)?;
    let pathspec = literal_pathspec(&path)?;
    let changes = git_changes_for(&repository)?;
    if !staged && changes.iter().any(|change| change.path == path && change.untracked) {
        let file_path = repository.join(&path);
        let metadata = fs::symlink_metadata(&file_path).map_err(|e| e.to_string())?;
        let contents = if metadata.file_type().is_symlink() {
            fs::read_link(&file_path)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned()
        } else {
            let canonical = file_path.canonicalize().map_err(|e| e.to_string())?;
            if !canonical.starts_with(&repository) {
                return Err("File is outside the Git repository".into());
            }
            fs::read_to_string(canonical)
                .map_err(|_| "This file is binary or is not valid UTF-8".to_string())?
        };
        let lines: Vec<_> = contents.lines().collect();
        let added_range = if lines.is_empty() {
            "0,0".to_string()
        } else {
            format!("1,{}", lines.len())
        };
        let mut diff = format!(
            "diff --git a/{path} b/{path}\nnew file\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +{added_range} @@\n"
        );
        for line in &lines {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
        if !contents.is_empty() && !contents.ends_with('\n') {
            diff.push_str("\\ No newline at end of file\n");
        }
        return Ok(diff);
    }
    let mut command = Command::new("git");
    command.arg("-C").arg(&repository);
    if staged {
        command.arg("diff").arg("--cached");
    } else {
        command.arg("diff");
    }
    let output = command
        .args(["--no-ext-diff", "--unified=3", "--", &pathspec])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
fn git_stage_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let repository = repository_for_workspace(&state)?;
    let pathspec = literal_pathspec(&path)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["add", "--", &pathspec])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[tauri::command]
fn git_unstage_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let repository = repository_for_workspace(&state)?;
    let pathspec = literal_pathspec(&path)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["restore", "--staged", "--", &pathspec])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[tauri::command]
fn git_commit(message: String, state: State<'_, AppState>) -> Result<String, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("Enter a commit message".into());
    }
    let repository = repository_for_workspace(&state)?;
    let staged = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if staged.success() {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(["add", "--all", "--", "."])
            .output()
            .map_err(|e| format!("Could not run Git: {e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
    } else if staged.code() != Some(1) {
        return Err("Could not inspect staged changes".into());
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .arg("commit")
        .arg("-m")
        .arg(message)
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map_err(|e| format!("Could not run Git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
fn resolve_open_file(path: String, state: State<'_, AppState>) -> Result<OpenTarget, String> {
    let file = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !file.is_file() {
        return Err("Selected path is not a file".into());
    }
    let parent = file
        .parent()
        .ok_or("Selected file has no parent folder")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let needs_workspace = match root {
        Some(root) => !file.starts_with(root.canonicalize().map_err(|e| e.to_string())?),
        None => true,
    };
    Ok(OpenTarget {
        path: dunce::simplified(&file).to_string_lossy().to_string(),
        workspace: dunce::simplified(&parent).to_string_lossy().to_string(),
        needs_workspace,
    })
}

#[tauri::command]
fn list_directory(path: String, state: State<'_, AppState>) -> Result<Vec<Entry>, String> {
    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let dir = authorized(&root, &path)?;
    if !dir.is_dir() {
        return Err("Not a directory".into());
    }
    let mut out = Vec::new();
    for item in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let name = item.file_name().to_string_lossy().to_string();
        if name.starts_with('.')
            || matches!(name.as_str(), "node_modules" | "target" | "dist" | ".git")
        {
            continue;
        }
        let path = item.path();
        let metadata = item.file_type().map_err(|e| e.to_string())?;
        out.push(Entry {
            name,
            path: path.to_string_lossy().to_string(),
            is_dir: metadata.is_dir(),
        });
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

#[tauri::command]
fn read_file(path: String, state: State<'_, AppState>) -> Result<FileSnapshot, String> {
    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let file = authorized(&root, &path)?;
    let before = file_version(&file)?;
    let content = fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let version = file_version(&file)?;
    if before != version {
        return Err("File changed while it was being opened; try again".into());
    }
    Ok(FileSnapshot { content, version })
}

#[tauri::command]
fn get_file_version(path: String, state: State<'_, AppState>) -> Result<String, String> {
    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let file = authorized(&root, &path)?;
    file_version(&file)
}

#[tauri::command]
fn allow_image_asset(
    app: tauri::AppHandle,
    path: String,
    relative_to: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let root = root.as_ref().ok_or("Open a project folder first")?;
    let requested = PathBuf::from(path);
    let target = if requested.is_absolute() {
        requested
    } else if let Some(base_file) = relative_to {
        let base_file = PathBuf::from(base_file);
        base_file
            .parent()
            .ok_or("Invalid Markdown file path")?
            .join(requested)
    } else {
        root.join(requested)
    };
    let file = authorized(&Some(root.clone()), &target.to_string_lossy())?;
    if !file.is_file() {
        return Err("Image file does not exist".into());
    }
    app.asset_protocol_scope()
        .allow_file(&file)
        .map_err(|e| e.to_string())?;
    Ok(dunce::simplified(&file).to_string_lossy().to_string())
}

#[tauri::command]
fn save_pasted_image(
    markdown_path: String,
    base64_image: String,
    extension: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use base64::Engine;

    if base64_image.len() > 70 * 1024 * 1024 {
        return Err("Pasted images must be smaller than 50 MB".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_image)
        .map_err(|_| "Could not read the pasted image data")?;
    if bytes.is_empty() {
        return Err("The pasted image is empty".into());
    }
    if bytes.len() > 50 * 1024 * 1024 {
        return Err("Pasted images must be smaller than 50 MB".into());
    }
    let extension = extension.to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "png" | "jpg" | "gif" | "webp" | "bmp" | "svg"
    ) {
        return Err("This pasted image format is not supported".into());
    }

    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let markdown = authorized(&root, &markdown_path)?;
    if !markdown.is_file() {
        return Err("Save the Markdown file before pasting an image".into());
    }
    let parent = markdown
        .parent()
        .ok_or("Markdown file has no parent folder")?;

    for index in 1..=10_000 {
        let name = if index == 1 {
            format!("image.{extension}")
        } else {
            format!("image{index}.{extension}")
        };
        let target = parent.join(&name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(&bytes) {
                    let _ = fs::remove_file(&target);
                    return Err(error.to_string());
                }
                return Ok(name);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("Could not find an available image filename".into())
}

#[tauri::command]
fn write_file(
    path: String,
    contents: String,
    expected_version: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let target = authorized(&root, &path)?;
    if !target.is_file() {
        return Err("File does not exist".into());
    }
    let current = file_version(&target)?;
    if expected_version.as_deref() != Some(current.as_str()) {
        return Err(
            "File changed outside blank. Reopen it before saving to avoid overwriting changes."
                .into(),
        );
    }
    fs::write(&target, contents).map_err(|e| e.to_string())?;
    file_version(&target)
}

#[tauri::command]
fn create_file(
    path: String,
    contents: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let root = root.as_ref().ok_or("Open a project folder first")?;
    let target = PathBuf::from(path);
    let parent = target
        .parent()
        .ok_or("Invalid path")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let base = root.canonicalize().map_err(|e| e.to_string())?;
    if !parent.starts_with(base) {
        return Err("Path is outside the open project".into());
    }
    let target = parent.join(target.file_name().ok_or("Invalid filename")?);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| e.to_string())?;
    f.write_all(contents.as_bytes())
        .map_err(|e| e.to_string())?;
    drop(f);
    file_version(&target)
}

#[tauri::command]
fn delete_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let target = PathBuf::from(path);
    let name = target
        .file_name()
        .ok_or("Invalid file path")?
        .to_os_string();
    let parent = target.parent().ok_or("Invalid file path")?;
    let directory = authorized(&root, &parent.to_string_lossy())?;
    if !directory.is_dir() {
        return Err("Parent path is not a directory".into());
    }
    let target = directory.join(name);
    let metadata = fs::symlink_metadata(&target).map_err(|e| e.to_string())?;
    if metadata.file_type().is_dir() {
        return Err("This action only deletes files".into());
    }
    fs::remove_file(target).map_err(|e| e.to_string())
}

#[tauri::command]
fn start_terminal(
    app: tauri::AppHandle,
    cwd: String,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let root = state.workspace.lock().map_err(|e| e.to_string())?.clone();
    let workdir = authorized(&root, &cwd)?;
    if !workdir.is_dir() {
        return Err("Terminal working directory is not a folder".into());
    }
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: rows.max(2),
            cols: cols.max(2),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let mut cmd = {
        let mut c = CommandBuilder::new("powershell.exe");
        c.arg("-NoLogo");
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = CommandBuilder::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into()));
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd =
        CommandBuilder::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into()));
    cmd.cwd(dunce::simplified(&workdir));
    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);
    let id = uuid::Uuid::new_v4().to_string();
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let event_id = id.clone();
    thread::spawn(move || {
        let mut bytes = [0u8; 4096];
        loop {
            match reader.read(&mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = app.emit(&format!("terminal-output-{event_id}"), bytes[..n].to_vec());
                }
            }
        }
        let _ = app.emit(&format!("terminal-exit-{event_id}"), ());
    });
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    state.terminals.lock().map_err(|e| e.to_string())?.insert(
        id.clone(),
        Terminal {
            writer,
            child,
            master: pair.master,
        },
    );
    Ok(id)
}

#[tauri::command]
fn terminal_write(id: String, data: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut map = state.terminals.lock().map_err(|e| e.to_string())?;
    let terminal = map.get_mut(&id).ok_or("Terminal not found")?;
    terminal
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn terminal_resize(
    id: String,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let map = state.terminals.lock().map_err(|e| e.to_string())?;
    let terminal = map.get(&id).ok_or("Terminal not found")?;
    terminal
        .master
        .resize(PtySize {
            rows: rows.max(2),
            cols: cols.max(2),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn close_terminal(id: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(mut terminal) = state
        .terminals
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&id)
    {
        let _ = terminal.child.kill();
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            set_workspace,
            git_worktrees,
            git_history,
            git_changes,
            git_file_diff,
            git_stage_file,
            git_unstage_file,
            git_commit,
            resolve_open_file,
            list_directory,
            read_file,
            get_file_version,
            allow_image_asset,
            save_pasted_image,
            write_file,
            create_file,
            delete_file,
            start_terminal,
            terminal_write,
            terminal_resize,
            close_terminal
        ])
        .run(tauri::generate_context!())
        .expect("error while running blank");
}
