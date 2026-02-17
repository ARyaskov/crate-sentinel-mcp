mod api_diff;
mod cargo;
mod ci;
mod crates_io;
mod error;
mod guards;
mod mcp;
mod persistence;
mod policy;
mod refactor;
mod session;
mod upgrade;

use anyhow::Error as AnyError;
use ci::{ci_cli_mode, error_exit_code, set_ci_cli_mode};
use error::{AppError, AppResult};
use tracing::info;
use tracing_subscriber::EnvFilter;

const API_SCHEMA_VERSION: &str = "mcp_tools_v1";

fn main() {
    let args = parse_args();
    if args.help {
        print_usage();
        return;
    }
    if args.version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    set_ci_cli_mode(args.ci);

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(error) => {
            eprintln!(
                "{{\"error\":\"runtime_init_failed\",\"details\":\"{}\"}}",
                error
            );
            std::process::exit(3);
        }
    };

    let result = runtime.block_on(async {
        init_tracing()?;
        let search_root = args
            .workspace
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        session::manager::recover_startup_sessions(&search_root).await?;
        info!(
            schema_version = API_SCHEMA_VERSION,
            "starting crate-sentinel-mcp"
        );
        mcp::run().await
    });

    if let Err(error) = result {
        let message = error.to_string();
        if ci_cli_mode() {
            eprintln!(
                "{{\"error\":\"ci_failure\",\"details\":\"{}\"}}",
                message.replace('"', "\\\"")
            );
            std::process::exit(error_exit_code(&message));
        }
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[derive(Debug)]
struct CliArgs {
    ci: bool,
    help: bool,
    version: bool,
    workspace: Option<std::path::PathBuf>,
}

fn parse_args() -> CliArgs {
    let mut ci = false;
    let mut help = false;
    let mut version = false;
    let mut workspace = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ci" => ci = true,
            "--help" | "-h" => help = true,
            "--version" | "-V" => version = true,
            "--workspace" => {
                if let Some(path) = args.next() {
                    workspace = Some(std::path::PathBuf::from(path));
                }
            }
            _ => {}
        }
    }
    CliArgs {
        ci,
        help,
        version,
        workspace,
    }
}

fn print_usage() {
    println!("crate-sentinel-mcp {}", env!("CARGO_PKG_VERSION"));
    println!("Usage: crate-sentinel-mcp [--ci] [--workspace <path>] [--help] [--version]");
    println!("  --ci                 Enable CI mode with deterministic machine-readable failures");
    println!("  --workspace <path>   Recovery scan root for persisted sessions");
    println!("  --help, -h           Print this help message");
    println!("  --version, -V        Print semantic version");
}

fn init_tracing() -> AppResult<()> {
    let env_filter = if ci_cli_mode() {
        EnvFilter::new("info")
    } else {
        EnvFilter::from_default_env()
    };
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .try_init()
        .map_err(|error| AnyError::msg(error.to_string()))
        .map_err(AppError::from)
}
