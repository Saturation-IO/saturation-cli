use anyhow::Result;
use serde::Serialize;

use crate::cli::{WorkspaceArgs, WorkspaceCommand};
use crate::config::Config;
use crate::output::Output;

#[derive(Serialize)]
struct WorkspaceListEntry {
    id: String,
    name: String,
    role: String,
    active: bool,
}

pub async fn execute(args: WorkspaceArgs, output: &Output) -> Result<()> {
    match args.command {
        WorkspaceCommand::List => list(output),
        WorkspaceCommand::Use { id } => use_workspace(&id, output),
        WorkspaceCommand::Current => current(output),
    }
}

fn list(output: &Output) -> Result<()> {
    let config = Config::load()?;

    if config.workspaces.is_empty() {
        output.status(
            "Workspaces",
            "None saved. Run `saturation auth token TOKEN --workspace ID`.",
        );
        return Ok(());
    }

    let entries: Vec<WorkspaceListEntry> = config
        .workspaces
        .iter()
        .map(|(id, ws)| WorkspaceListEntry {
            id: id.clone(),
            name: ws.name.clone(),
            role: ws.role.clone(),
            active: config.active_workspace.as_deref() == Some(id.as_str()),
        })
        .collect();

    output.print(&entries)
}

fn use_workspace(id: &str, output: &Output) -> Result<()> {
    let mut config = Config::load()?;

    // Validate the workspace exists in our known list
    if !config.workspaces.contains_key(id) {
        // Allow setting it anyway — it may be valid, just not cached
        output.status(
            "Warning",
            &format!(
                "Workspace '{id}' not found in cached list. \
                 The CLI will still use it for agent commands."
            ),
        );
    }

    config.active_workspace = Some(id.to_string());
    config.save()?;

    if let Some(ws) = config.workspaces.get(id) {
        output.success(&format!("Active agent workspace: {} ({})", ws.name, id));
    } else {
        output.success(&format!("Active agent workspace: {id}"));
    }

    Ok(())
}

fn current(output: &Output) -> Result<()> {
    let config = Config::load()?;

    match &config.active_workspace {
        Some(id) => {
            if let Some(ws) = config.workspaces.get(id) {
                output.print(&serde_json::json!({
                    "id": id,
                    "name": ws.name,
                    "role": ws.role,
                }))?;
            } else {
                output.print(&serde_json::json!({
                    "id": id,
                }))?;
            }
        }
        None => {
            output.error("No active agent workspace. Run `saturation workspace use <id>` first.");
        }
    }

    Ok(())
}
