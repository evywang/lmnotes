use clap::{Parser, Subcommand};
use lmnotes_core::vault::Vault;

#[derive(Parser)]
#[command(name = "lmnotes", version, about = "LMNotes CLI")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 校验 vault 是否符合 OKF v0.1 §9
    Validate { path: String },
    /// 创建新 vault 骨架
    Init { path: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Validate { path } => {
            let vault = Vault::open(&path).map_err(|e| anyhow::anyhow!(e))?;
            let report = vault.validate().await.map_err(|e| anyhow::anyhow!(e))?;
            println!("Checked {} concept(s)", report.checked);
            if report.is_ok() {
                println!("✓ OKF v0.1 conformant");
            } else {
                println!("✗ {} error(s):", report.errors.len());
                for (p, msg) in &report.errors {
                    println!("  {p}: {msg}");
                }
                std::process::exit(1);
            }
        }
        Cmd::Init { path } => {
            Vault::create(&path).await.map_err(|e| anyhow::anyhow!(e))?;
            println!("✓ Created vault at {path}");
        }
    }
    Ok(())
}
