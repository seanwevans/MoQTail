use std::collections::HashSet;
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

    for package in metadata.packages {
        if workspace.contains(&package.id) {
            for dep in package.dependencies {
                println!("{} -> {}", package.name, dep.name);
            }
        }
    }
    Ok(())
}
