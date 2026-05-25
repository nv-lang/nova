//! Build smoke tests for the `nova-lsp` binary — Plan 104.0.1.
//!
//! Three tests:
//! - pos1: binary was compiled and is executable (--help works)
//! - pos2: `nova-lsp --version` prints a version string and exits 0
//! - neg1: server exits gracefully (not panic) when stdin is immediately closed

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path to the compiled `nova-lsp` binary.
/// Cargo sets `CARGO_BIN_EXE_nova-lsp` when compiling integration tests for
/// a package that declares a `[[bin]]` with that name.
fn lsp_binary() -> std::path::PathBuf {
    // env! is evaluated at compile time; Cargo guarantees the var is set.
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_nova-lsp"))
}

// ---------------------------------------------------------------------------
// pos1 — binary exists and responds to --help
// ---------------------------------------------------------------------------

/// pos1: The nova-lsp binary was compiled successfully and can be launched.
///
/// `cargo test` already built the binary before running this test.
/// We verify the artifact is present, is a regular file, and responds to
/// `--help` with exit code 0 (Clap handles --help automatically).
#[test]
fn pos1_binary_exists_and_is_executable() {
    let binary = lsp_binary();

    assert!(
        binary.exists(),
        "nova-lsp binary not found at {binary:?}.\n\
         Run `cd nova-lsp && cargo build` to build it first.",
    );
    assert!(
        binary.is_file(),
        "nova-lsp path {binary:?} exists but is not a regular file",
    );

    // Invoke --help; Clap exits 0 on success.
    let status = Command::new(&binary)
        .arg("--help")
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn `nova-lsp --help`: {e}"));

    assert!(
        status.success(),
        "`nova-lsp --help` exited with non-zero status: {status}",
    );
}

// ---------------------------------------------------------------------------
// pos2 — --version flag
// ---------------------------------------------------------------------------

/// pos2: `nova-lsp --version` prints the package version and exits 0.
///
/// Clap's `#[command(version)]` generates a `--version` flag that prints
/// "<binary> <version>" to stdout.  We assert the output contains both
/// "nova-lsp" and at least one digit (semver digit).
#[test]
fn pos2_version_flag_prints_version() {
    let output = Command::new(lsp_binary())
        .arg("--version")
        .output()
        .unwrap_or_else(|e| panic!("failed to run `nova-lsp --version`: {e}"));

    assert!(
        output.status.success(),
        "`nova-lsp --version` exited non-zero: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("nova-lsp"),
        "expected 'nova-lsp' in --version output, got: {stdout:?}",
    );
    assert!(
        stdout.chars().any(|c| c.is_ascii_digit()),
        "expected semver digits in --version output, got: {stdout:?}",
    );
}

// ---------------------------------------------------------------------------
// neg1 — closed stdin → graceful exit, no panic
// ---------------------------------------------------------------------------

/// neg1: When stdin is immediately closed (EOF), nova-lsp exits gracefully.
///
/// A well-behaved LSP server notices stdin EOF and exits without panicking.
/// We verify:
///   1. The process exits within 30 seconds (not hung indefinitely).
///   2. stderr does not contain "panicked at" (Rust panic marker).
///   3. The exit code is not 101 (Windows Rust panic exit code).
///
/// The exit code itself may be non-zero (EOF is an error condition for an LSP
/// server), but it must be a *clean* exit, not a crash.
#[test]
fn neg1_closed_stdin_graceful_exit() {
    let mut child = Command::new(lsp_binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn nova-lsp: {e}"));

    // Drain stderr in a background thread so the process never blocks on a
    // full pipe buffer (which would prevent it from exiting).
    let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
    let (stderr_tx, stderr_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        use std::io::Read;
        let mut buf = String::new();
        let _ = stderr_pipe.read_to_string(&mut buf);
        let _ = stderr_tx.send(buf);
    });

    // Also drain stdout so the server doesn't block on a full stdout pipe.
    let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
    thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
    });

    // Close stdin immediately — this sends EOF to the LSP server's transport.
    drop(child.stdin.take());

    // Poll for exit with a 30-second deadline.
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        match child.try_wait().expect("failed to poll child exit status") {
            Some(s) => break s,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    panic!(
                        "nova-lsp did not exit within 30 seconds after stdin was closed. \
                         Possible hang in the event loop."
                    );
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    };

    // Collect stderr (the drain thread should be done since the process exited).
    let stderr = stderr_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_default();

    // Must not be a Rust panic.
    assert!(
        !stderr.contains("panicked at"),
        "nova-lsp panicked on closed stdin:\n{stderr}",
    );

    // On Windows: Rust panic exit code is 101 (STATUS_ILLEGAL_INSTRUCTION
    // path) or 0xC0000005 (access violation).  Either way, 101 is the
    // canonical "Rust unwinding panic reached main" code.
    #[cfg(windows)]
    assert_ne!(
        status.code(),
        Some(101),
        "nova-lsp exited with code 101 (Rust panic) after stdin close",
    );

    // If we reach here the test passes: the process exited cleanly.
    let _ = status;
}
