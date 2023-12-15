use clap::Parser;

pub mod cli;
pub mod logic;
pub mod ssh;
pub mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // env_logger::builder()
    //     .filter_level(log::LevelFilter::Debug)
    //     .init();
    logic::run(cli::Command::parse()).await?;
    Ok(())
}
