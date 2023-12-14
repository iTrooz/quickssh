use clap::Parser;

pub mod cli;
pub mod ssh;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // env_logger::builder()
    //     .filter_level(log::LevelFilter::Debug)
    //     .init();
    cli::run(cli::Command::parse()).await?;
    Ok(())
}
