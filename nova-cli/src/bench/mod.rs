// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57 — `nova bench` infrastructure.
//!
//! Layers:
//!   - stats.rs   — L3 statistical functions
//!   - schema.rs  — L4 JSON v1 schema
//!   - repro.rs   — L10 reproducibility metadata + env checks
//!   - config.rs  — L6 bench.toml parser
//!   - run.rs     — L1+L4 sample collection (orchestrates compile+execute+parse)
//!   - diff.rs    — L5 compare tool (Welch's t-test + geomean)
//!   - gate.rs    — L6 CI gate (auto noise-floor + thresholds)
//!   - report.rs  — L4 output formatters (terminal table, markdown, CSV)

pub mod stats;
pub mod schema;
pub mod repro;
pub mod config;
pub mod run;
pub mod diff;
pub mod gate;
pub mod report;
pub mod history;
pub mod dashboard;
pub mod noise;
pub mod profile;
pub mod criterion_compat;
pub mod cpu_instr;
pub mod corpus;
pub mod anomaly;
pub mod remote;
pub mod ai;
pub mod membw;
pub mod errno;

pub use stats::SampleStats;
pub use schema::{RawBenchResult, AnalyzedBench, RunResultParsed, run_result_to_json, SCHEMA_VERSION};
pub use repro::{ReproMeta, SamplingMeta};
pub use config::BenchToml;
