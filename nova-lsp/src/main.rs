//! nova-lsp — Nova language LSP server binary entry point.
//!
//! All logic lives in `lib.rs` (modules: server, state, compiler, …).
//! This file just parses CLI args, initialises logging, and starts the server.
//!
//! # Build
//! ```sh
//! cd nova-lsp && cargo build --release
//! # Binary: nova-lsp/target/release/nova-lsp[.exe]
//! ```

use clap::Parser;
use nova_lsp::server::Backend;
use tower_lsp::{LspService, Server};
use tracing_subscriber::EnvFilter;

/// Nova language LSP server.
///
/// Communicates with editors via JSON-RPC over stdin/stdout.
/// Logging output goes to stderr (editors show it in their output panels).
///
/// Control log verbosity with: NOVA_LSP_LOG=trace|debug|info|warn|error
#[derive(Parser, Debug)]
#[command(
    name = "nova-lsp",
    version,
    about = "Nova language LSP server",
    long_about = None,
)]
struct Args {
    // V1: no positional arguments — LSP communicates via stdio.
    // Planned: --workspace-root <path> for multi-root workspaces (Plan 104.1).
}

#[tokio::main]
async fn main() {
    // Parse CLI args first — `--version` / `--help` exit here before async code.
    let _args = Args::parse();

    // Structured logging to stderr (stdout = JSON-RPC transport).
    // NOVA_LSP_LOG env var controls verbosity; default is INFO + dep warnings.
    let filter = EnvFilter::try_from_env("NOVA_LSP_LOG")
        .unwrap_or_else(|_| EnvFilter::new("nova_lsp=info,warn"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "nova-lsp starting"
    );

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;

    tracing::info!("nova-lsp exiting");
}
