mod api;
mod assets;
mod auth;
mod cli;
mod config;
mod execution;
mod mcp;
mod policy;
mod registry;
mod search;
mod service;
mod state;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--sandbox-worker") {
        return cli::worker();
    }
    cli::run(cli::Cli::parse())
}
