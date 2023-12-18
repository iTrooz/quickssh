use clap::{command, Parser, ArgAction};

#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"), author, about, version, long_about = None)]
pub struct Command {
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,
    #[arg(short, long)]
    pub user: Option<String>,
    #[arg(short, long)]
    pub password: Option<String>,
    // public keys that can be used to connect
    #[arg(long)]
    pub pubkey: Vec<String>,
    // Default shell that connected users will have. Default to the shell used to start the quickssh server process
    #[arg(long)]
    pub shell: Option<String>,
    // Disable shell
    #[arg(long)]
    pub no_shell: bool,
    // Disable SFTP submodule
    #[arg(long)]
    pub no_sftp: bool,
}
