//! fnox-add: fuzzy-select known secret references from a personal catalog and
//! write them into the current project's `fnox.toml` via `fnox set`.

mod config;

use anyhow::{Context, Result, bail};
use clap::Parser;
use config::{Item, build_items, build_set_args, parse_config, resolve_config_path, value_for_ref};
use inquire::{InquireError, MultiSelect};
use std::process::Command;

#[derive(Parser)]
#[command(
    name = "fnox-add",
    version,
    about = "Fuzzy-select fnox secret references into a project's fnox.toml"
)]
struct Cli {
    /// Restrict to a single catalog group
    #[arg(short, long)]
    group: Option<String>,

    /// Print the fnox commands without running them
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,

    /// Catalog config path (default: $FNOX_ADD_CONFIG or ~/.config/fnox-add/config.toml)
    #[arg(long)]
    config: Option<String>,

    /// Target fnox.toml path (default: ./fnox.toml in cwd)
    #[arg(short, long)]
    target: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let path = resolve_config_path(
        cli.config.as_deref(),
        std::env::var("FNOX_ADD_CONFIG").ok().as_deref(),
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        dirs::home_dir().as_deref().and_then(|p| p.to_str()),
    )?;

    let data = std::fs::read_to_string(&path)
        .with_context(|| format!("reading config {}", path.display()))?;
    let cfg = parse_config(&data).with_context(|| format!("parsing config {}", path.display()))?;

    let items = build_items(&cfg, cli.group.as_deref())?;
    if items.is_empty() {
        bail!("no secrets in catalog for the selected group");
    }

    let selected = match MultiSelect::new("Select secrets to add:", items).prompt() {
        Ok(sel) => sel,
        // Esc / Ctrl-C: nothing to do.
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    if selected.is_empty() {
        return Ok(());
    }

    apply(&selected, cli.dry_run, cli.target.as_deref())
}

/// Run (or, in dry-run, print) `fnox set` for each selected item, continuing past
/// per-item failures and reporting a summary.
fn apply(selected: &[Item], dry_run: bool, target: Option<&str>) -> Result<()> {
    let mut failures = 0;
    for item in selected {
        let args = build_set_args(item, target);
        if dry_run {
            println!("fnox {}", args.join(" "));
            continue;
        }
        let status = Command::new("fnox")
            .args(&args)
            .status()
            .with_context(|| format!("running fnox set for {}", item.env))?;
        if status.success() {
            println!(
                "✓ {} → {} (provider {})",
                item.env,
                value_for_ref(&item.reference),
                item.provider
            );
        } else {
            failures += 1;
            eprintln!("✗ {} (fnox set exited with {status})", item.env);
        }
    }
    if failures > 0 {
        bail!("{failures} of {} secrets failed", selected.len());
    }
    Ok(())
}
