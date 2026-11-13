//! virsh-tui library root.

// The release binaries are static musl builds, and musl's allocator makes the
// app 1.4-2x slower than with glibc; mimalloc closes that gap. In the library so
// the benchmarks measure the same allocator as the binary.
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod app;
pub mod backend;
pub mod cli;
pub mod command;
pub mod config;
pub mod input;
pub mod metrics;
pub mod model;
pub mod theme;
pub mod ui;
pub mod xml;
