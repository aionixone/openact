use anyhow::{anyhow, Result};
use std::process::Command;

#[derive(Debug, serde::Deserialize)]
struct Config {
    enabled: Vec<String>,
}

fn read_connectors() -> Result<Vec<String>> {
    let content = std::fs::read_to_string("connectors.toml")?;
    let config: Config = toml::from_str(&content)?;
    Ok(config.enabled)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: cargo xtask <command>");
        eprintln!("Commands:");
        eprintln!(
            "  build -p <package> [--release]  Build package with connectors from connectors.toml"
        );
        return Err(anyhow!("missing command"));
    }

    match args[1].as_str() {
        "build" => {
            if args.len() < 4 || !args.contains(&"-p".to_string()) {
                eprintln!("Usage: cargo xtask build -p <package> [--release]");
                return Err(anyhow!("missing -p argument"));
            }

            let pkg_idx = args.iter().position(|a| a == "-p").unwrap();
            let package = args.get(pkg_idx + 1).ok_or_else(|| anyhow!("missing package name"))?;

            // Read enabled connectors from connectors.toml
            let enabled = read_connectors()?;
            let features: String = enabled
                .into_iter()
                .map(|k| format!("openact-plugins/{}", k))
                .collect::<Vec<_>>()
                .join(",");

            println!("Building {} with connectors: {}", package, features);

            let mut cmd = Command::new("cargo");
            cmd.arg("build").arg("-p").arg(package);

            if !features.is_empty() {
                cmd.arg("--features").arg(&features);
            }

            if args.contains(&"--release".to_string()) {
                cmd.arg("--release");
            }

            let status = cmd.status()?;
            if !status.success() {
                return Err(anyhow!("cargo build failed"));
            }

            println!("✓ Build completed successfully");
            Ok(())
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            eprintln!("Available commands: build");
            Err(anyhow!("unknown command"))
        }
    }
}
