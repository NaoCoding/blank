use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::PathBuf,
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
            resolve_open_file,
            list_directory,
            read_file,
            get_file_version,
            allow_image_asset,
            save_pasted_image,
            write_file,
            create_file,
            start_terminal,
            terminal_write,
            terminal_resize,
            close_terminal
        ])
        .run(tauri::generate_context!())
        .expect("error while running blank");
}
