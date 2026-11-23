//! aurelius-bridge: the MCP server Claude Code starts for one run.
//! `aurelius-bridge --data-dir <dir> --run <run_id> [--project]`; see `mcp`.
//! The daemon spawns it; it is not meant to be run by hand.

mod design;
mod mcp;
mod screen;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    mcp::serve(&args).await
}
