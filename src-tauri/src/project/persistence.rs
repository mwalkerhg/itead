use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

use super::{AppSettings, ProjectManifest};

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| anyhow!("Failed to resolve app data directory: {}", e))
}

fn projects_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    Ok(app_data_dir(app)?.join("projects"))
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn load_settings(app: &tauri::AppHandle) -> Result<AppSettings> {
    let path = app_data_dir(app)?.join("settings.json");
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let data = fs::read_to_string(&path)?;
    let settings: AppSettings = serde_json::from_str(&data)?;
    Ok(settings)
}

pub fn save_settings(app: &tauri::AppHandle, settings: &AppSettings) -> Result<()> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("settings.json");
    let data = serde_json::to_string_pretty(settings)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn list_projects(app: &tauri::AppHandle) -> Result<Vec<String>> {
    let dir = projects_dir(app)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let project_file = entry.path().join("project.json");
            if project_file.exists() {
                if let Some(name) = entry.file_name().to_str() {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

pub fn create_project(app: &tauri::AppHandle, name: &str) -> Result<(PathBuf, ProjectManifest)> {
    let safe_name = sanitize_name(name);
    if safe_name.is_empty() {
        return Err(anyhow!("Project name cannot be empty"));
    }

    let dir = projects_dir(app)?.join(&safe_name);
    if dir.exists() {
        return Err(anyhow!("Project '{}' already exists", safe_name));
    }

    fs::create_dir_all(dir.join("recordings"))?;

    let manifest = ProjectManifest::new(name.to_string());
    save_project(&dir, &manifest)?;

    Ok((dir, manifest))
}

pub fn load_project(project_dir: &Path) -> Result<ProjectManifest> {
    let path = project_dir.join("project.json");
    if !path.exists() {
        return Err(anyhow!("Project file not found: {}", path.display()));
    }
    let data = fs::read_to_string(&path)?;
    let manifest: ProjectManifest = serde_json::from_str(&data)?;
    Ok(manifest)
}

pub fn save_project(project_dir: &Path, manifest: &ProjectManifest) -> Result<()> {
    fs::create_dir_all(project_dir)?;
    let path = project_dir.join("project.json");
    let data = serde_json::to_string_pretty(manifest)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn open_project(app: &tauri::AppHandle, name: &str) -> Result<(PathBuf, ProjectManifest)> {
    let safe_name = sanitize_name(name);
    let dir = projects_dir(app)?.join(&safe_name);
    let manifest = load_project(&dir)?;
    Ok((dir, manifest))
}

pub fn recordings_dir(project_dir: &Path) -> PathBuf {
    project_dir.join("recordings")
}
