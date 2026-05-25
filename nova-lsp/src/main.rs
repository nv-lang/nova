//! nova-lsp — Nova language LSP server.
//!
//! Communicates with editors over JSON-RPC on stdin/stdout (stdio transport).
//! Log output (tracing) goes to **stderr** so it doesn't pollute the JSON-RPC
//! channel. Editors display it in the "Output" panel (VSCode: "Nova LSP").
//!
//! # Build
//! ```sh
//! cd nova-lsp && cargo build --release
//! # Binary: nova-lsp/target/release/nova-lsp[.exe]
//! ```
//!
//! # Usage
//! Editors spawn `nova-lsp` as a child process and communicate via stdio.
//! See `nova-lsp/README.md` for per-editor configuration snippets.
//!
//! # Plan
//! Plan 104.0 — foundation skeleton.
//! Plan 104.1+ — diagnostics, hover, completion, quick-fixes, rename.

mod server;
mod state;

use clap::Parser;
use server::Backend;
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
    // Parse CLI args first.
    // `--version` and `--help` call std::process::exit(0) here,
    // before any async runtime or LSP server initialization.
    let _args = Args::parse();

    // Initialize structured logging to stderr.
    // Editors pipe stderr to their output panel (e.g., VSCode "Output > Nova LSP").
    // NOVA_LSP_LOG env var controls verbosity; default is INFO + warnings from deps.
    let filter = EnvFilter::try_from_env("NOVA_LSP_LOG")
        .unwrap_or_else(|_| EnvFilter::new("nova_lsp=info,warn"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        // Disable ANSI color codes — editor output panels don't render them.
        .with_ansi(false)
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "nova-lsp starting"
    );

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    // Build the LSP service.
    // LspService::new takes a factory fn (called once, with the Client handle).
    // The returned `socket` is used by Server to send client notifications.
    let (service, socket) = LspService::new(Backend::new);

    // Serve JSON-RPC over stdin/stdout until the connection closes.
    // When stdin reaches EOF (editor process died or clean shutdown), serve()
    // returns and main() exits naturally with code 0.
    Server::new(stdin, stdout, socket).serve(service).await;

    tracing::info!("nova-lsp exiting");
}
