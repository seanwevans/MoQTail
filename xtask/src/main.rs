use std::collections::{HashMap, HashSet};
use anyhow::Result;
use clap::{Parser, Subcommand};
use cargo_metadata::{MetadataCommand, PackageId};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print crate dependency graph
    RepoGraph,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::RepoGraph => repo_graph()?,
    }
    Ok(())
}

fn repo_graph() -> Result<()> {
    let metadata = MetadataCommand::new().exec()?;
    let workspace: HashSet<PackageId> = metadata.workspace_members.into_iter().collect();

    let resolve = metadata.resolve.expect("resolve graph missing");
    let id_to_name: HashMap<PackageId, String> = metadata
        .packages
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();

    for node in resolve.nodes {
        if workspace.contains(&node.id) {
            for dep_id in node.dependencies {
                if workspace.contains(&dep_id) {
                    println!(
                        "{} -> {}",
                        id_to_name.get(&node.id).unwrap(),
                        id_to_name.get(&dep_id).unwrap()
                    );
                }
            }
        }
    }
    Ok(())
}
