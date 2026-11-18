use crate::model::{AppConfig, Job, Project, WorkflowSummary};
use anyhow::Context;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub fn load_or_create_config(data_dir: &Path) -> anyhow::Result<AppConfig> {
    let path = data_dir.join("settings.json");
    if path.exists() {
        return read_json(&path).context("cannot read data/settings.json");
    }
    let config = AppConfig::default();
    write_json(&path, &config)?;
    Ok(config)
}

pub fn load_projects(data_dir: &Path) -> anyhow::Result<BTreeMap<Uuid, Project>> {
    let projects_dir = data_dir.join("projects");
    fs::create_dir_all(&projects_dir)?;
    let mut projects = BTreeMap::new();
    for entry in fs::read_dir(projects_dir)? {
        let entry = entry?;
        crate::edit_versions::recover_pending(&entry.path())?;
        let path = entry.path().join("project.json");
        if path.is_file() {
            let mut project: Project = read_json(&path)
                .with_context(|| format!("cannot read project record {}", path.display()))?;
            crate::album::restore_checkpoints(path.parent().unwrap(), &mut project)?;
            projects.insert(project.id, project);
        }
    }
    Ok(projects)
}

pub fn save_project(data_dir: &Path, project: &Project) -> anyhow::Result<()> {
    let project_dir = project_dir(data_dir, project.id);
    fs::create_dir_all(project_dir.join("assets"))?;
    fs::create_dir_all(project_dir.join("exports"))?;
    write_json(&project_dir.join("project.json"), project)
}

pub fn project_dir(data_dir: &Path, project_id: Uuid) -> PathBuf {
    data_dir.join("projects").join(project_id.to_string())
}

pub fn load_workflows(data_dir: &Path) -> anyhow::Result<Vec<WorkflowSummary>> {
    let path = data_dir.join("workflows.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_json(&path)
}

pub fn save_workflows(data_dir: &Path, workflows: &[WorkflowSummary]) -> anyhow::Result<()> {
    write_json(&data_dir.join("workflows.json"), workflows)
}

pub fn save_jobs(data_dir: &Path, jobs: &[Job]) -> anyhow::Result<()> {
    write_json(&data_dir.join("jobs.json"), jobs)
}

pub fn load_jobs(data_dir: &Path) -> anyhow::Result<Vec<Job>> {
    let path = data_dir.join("jobs.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_json(&path)
}

pub fn write_json<T: serde::Serialize + ?Sized>(path: &Path, value: &T) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path)?;
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}
