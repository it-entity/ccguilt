use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Clone)]
pub struct ClaudeDataDir {
    pub base: PathBuf,
}

impl ClaudeDataDir {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    pub fn default_path() -> Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        Ok(home.join(".claude"))
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.base.join("projects")
    }

    pub fn stats_cache_path(&self) -> PathBuf {
        self.base.join("stats-cache.json")
    }

    /// Returns all JSONL session files, optionally filtered by project substring
    pub fn jsonl_files(&self, project_filter: Option<&str>) -> Vec<PathBuf> {
        let projects_dir = self.projects_dir();
        if !projects_dir.exists() {
            return Vec::new();
        }

        let mut files = Vec::new();

        for entry in WalkDir::new(&projects_dir)
            .min_depth(1)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                if let Some(filter) = project_filter {
                    let project_name = Self::project_name_from_jsonl(path, &projects_dir);
                    let decoded = decode_project_name(&project_name);
                    if !project_name.contains(filter) && !decoded.contains(filter) {
                        continue;
                    }
                }
                files.push(path.to_path_buf());
            }
        }

        files
    }

    /// Extract project directory name from a JSONL file path
    fn project_name_from_jsonl(jsonl_path: &Path, projects_dir: &Path) -> String {
        if let Ok(relative) = jsonl_path.strip_prefix(projects_dir) {
            if let Some(first_component) = relative.components().next() {
                return first_component.as_os_str().to_string_lossy().to_string();
            }
        }
        String::new()
    }

    /// Returns JSONL files filtered by project regex pattern
    pub fn jsonl_files_regex(&self, pattern: &str) -> anyhow::Result<Vec<PathBuf>> {
        let re = regex::Regex::new(pattern)?;
        let projects_dir = self.projects_dir();
        if !projects_dir.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        for entry in WalkDir::new(&projects_dir)
            .min_depth(1)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                let project_name = Self::project_name_from_jsonl(path, &projects_dir);
                let decoded = decode_project_name(&project_name);
                if re.is_match(&project_name) || re.is_match(&decoded) {
                    files.push(path.to_path_buf());
                }
            }
        }
        Ok(files)
    }

    /// Count the number of project directories
    pub fn project_count(&self) -> usize {
        let projects_dir = self.projects_dir();
        if !projects_dir.exists() {
            return 0;
        }
        std::fs::read_dir(projects_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .count()
            })
            .unwrap_or(0)
    }
}

pub struct OpenCodeDataDir {
    pub base: PathBuf,
}

impl OpenCodeDataDir {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    pub fn default_path() -> Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        Ok(home.join(".local").join("share").join("opencode"))
    }

    pub fn db_path(&self) -> PathBuf {
        self.base.join("opencode.db")
    }

    pub fn exists(&self) -> bool {
        self.db_path().exists()
    }
}

pub struct GeminiDataDir {
    pub base: PathBuf,
}

impl GeminiDataDir {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    pub fn default_path() -> Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        Ok(home.join(".gemini"))
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.base.join("tmp")
    }

    pub fn exists(&self) -> bool {
        self.tmp_dir().exists()
    }

    pub fn session_files(&self, project_filter: Option<&str>) -> Vec<PathBuf> {
        let tmp = self.tmp_dir();
        if !tmp.exists() {
            return Vec::new();
        }

        let mut files = Vec::new();

        for entry in WalkDir::new(&tmp)
            .min_depth(1)
            .max_depth(4)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if filename.starts_with("session-") && filename.ends_with(".json") {
                if let Some(filter) = project_filter {
                    let project_name = Self::project_name_from_path(path);
                    if !project_name.contains(filter) {
                        continue;
                    }
                }
                files.push(path.to_path_buf());
            }
        }

        files
    }

    fn project_name_from_path(session_path: &Path) -> String {
        session_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

pub fn decode_project_name(encoded: &str) -> String {
    if encoded.is_empty() {
        return String::new();
    }
    encoded
        .chars()
        .map(|c| if c == '-' { '/' } else { c })
        .collect()
}
