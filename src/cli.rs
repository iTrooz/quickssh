use clap::{command, Parser};

#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"), author, about, version, long_about = None)]
pub struct Command {
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(short, long)]
    pub user: Option<String>,
    #[arg(short, long)]
    pub password: Option<String>,
    // public keys that can be used to connect
    #[arg(long)]
    pub pubkey: Vec<String>,
}
