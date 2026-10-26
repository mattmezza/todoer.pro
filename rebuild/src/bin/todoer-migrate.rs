//! Administrative, non-destructive migration. No source row data is logged.
use clap::{Parser, Subcommand};
use std::{collections::BTreeMap, path::PathBuf};
use todoer::{
    core::Store,
    transfer::{self, ExportBundle, ExportScope, IdentityMap},
};
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Legacy {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        destination: PathBuf,
        #[arg(long)]
        identity_map: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        ack_unsupported: bool,
    },
    Restore {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        destination: PathBuf,
        #[arg(long)]
        identity_map: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
    },
    Export {
        #[arg(long)]
        database: PathBuf,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        list: Option<String>,
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "json")]
        format: String,
    },
}
fn mapping(path: Option<PathBuf>) -> Result<IdentityMap, Box<dyn std::error::Error>> {
    let Some(path) = path else {
        return Ok(IdentityMap::default());
    };
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    if value.get("users").is_some() {
        Ok(serde_json::from_value(value)?)
    } else {
        let users: BTreeMap<String, String> = serde_json::from_value(value)?;
        Ok(IdentityMap { users })
    }
}
fn independent(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if destination.exists() && std::fs::canonicalize(source)? == std::fs::canonicalize(destination)?
    {
        return Err("source and destination must be different files".into());
    }
    #[cfg(unix)]
    if destination.exists() {
        use std::os::unix::fs::MetadataExt;
        let a = std::fs::metadata(source)?;
        let b = std::fs::metadata(destination)?;
        if a.dev() == b.dev() && a.ino() == b.ino() {
            return Err("source and destination must be different files".into());
        }
    }
    Ok(())
}
fn destination(path: &std::path::Path, apply: bool) -> Result<Store, Box<dyn std::error::Error>> {
    if apply {
        if !path.exists() {
            let mut options = std::fs::OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(path)?;
        }
        return Ok(Store::open(
            path.to_str().ok_or("non-UTF8 destination path")?,
        )?);
    }
    let mut db = Store::memory()?;
    if path.exists() {
        let source = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        db = Store::memory()?; // backup through a temporary private copy is handled by transfer helper below
        transfer::copy_destination_for_dry_run(&source, &mut db)?;
    }
    Ok(db)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Command::Legacy {
            source,
            destination: target,
            identity_map,
            apply,
            ack_unsupported,
        } => {
            independent(&source, &target)?;
            let snapshot = transfer::read_legacy(&source)?;
            let mut db = destination(&target, apply)?;
            let report = transfer::import_legacy(
                &mut db,
                &snapshot,
                &mapping(identity_map)?,
                apply,
                ack_unsupported,
            )?;
            println!("{}", serde_json::to_string(&report)?);
        }
        Command::Restore {
            source,
            destination: target,
            identity_map,
            apply,
        } => {
            independent(&source, &target)?;
            let bundle: ExportBundle = serde_json::from_slice(&std::fs::read(&source)?)?;
            let mut db = destination(&target, apply)?;
            let report = transfer::import_bundle(&mut db, &bundle, &mapping(identity_map)?, apply)?;
            println!("{}", serde_json::to_string(&report)?);
        }
        Command::Export {
            database,
            actor,
            list,
            workspace,
            output,
            format,
        } => {
            if list.is_some() && workspace.is_some() {
                return Err("select list or workspace, not both".into());
            }
            let scope = if let Some(id) = list {
                ExportScope::List(id)
            } else if let Some(id) = workspace {
                ExportScope::Workspace(id)
            } else {
                ExportScope::Account
            };
            let db = transfer::open_readonly_snapshot(&database)?;
            if format == "zip" {
                let objects = todoer::object_storage::S3Config::from_env()?
                    .map(todoer::object_storage::ObjectStorage::new)
                    .transpose()?;
                let legacy = todoer::object_storage::LegacyStorageConfig::from_env()?
                    .map(todoer::object_storage::LegacyBlobSource::new)
                    .transpose()?;
                let manifest = transfer::export_zip(
                    std::sync::Arc::new(std::sync::Mutex::new(db)),
                    actor,
                    scope,
                    objects.as_ref(),
                    legacy.as_ref(),
                    &output,
                )
                .await?;
                println!(
                    "{}",
                    serde_json::json!({"files":manifest.entries.len(),"bytes":manifest.entries.iter().map(|e|e.size).sum::<u64>()})
                );
            } else {
                let bundle = db.export_bundle(&actor, scope)?;
                let bytes = match format.as_str() {
                    "json" => serde_json::to_vec_pretty(&bundle)?,
                    "text" => transfer::export_text(&bundle)?.into_bytes(),
                    "csv" => transfer::export_csv(&bundle)?,
                    _ => return Err("format must be json, text, csv or zip".into()),
                };
                transfer::write_private_new(&output, &bytes)?;
                println!("{{\"exported\":true}}");
            }
        }
    }
    Ok(())
}
