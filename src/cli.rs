use std::{collections::HashMap, sync::Arc};

use clap::{command, Parser};
use russh::MethodSet;
use tokio::sync::Mutex;

use crate::ssh;

#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"), author, about, version, long_about = None)]
pub struct Command {
    #[arg(short, long)]
    pub verbose: bool,
}

pub async fn run(cmd: Command) -> anyhow::Result<()> {
    // init logger
    let mut tmp = env_logger::builder();
    let mut log_builder = &mut tmp;
    if cmd.verbose {
        log::debug!("Debug logging enabled");
        log_builder = log_builder.filter_level(log::LevelFilter::Debug);
    }
    log_builder.init();

    let config = russh::server::Config {
        methods: MethodSet::PASSWORD | MethodSet::PUBLICKEY,
        inactivity_timeout: Some(std::time::Duration::from_secs(60 * 60)),
        auth_rejection_time: std::time::Duration::from_secs(5),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        keys: vec![
            // TODO: only do this in dev
            russh_keys::key::KeyPair::generate_ed25519().unwrap(),
        ],
        ..Default::default()
    };
    let server = ssh::Server {
        clients: Arc::new(Mutex::new(HashMap::new())),
        channel_pty_writers: Arc::new(Mutex::new(HashMap::new())),
        id: 0,
    };

    log::info!("Listening on 0.0.0.0:2222");
    russh::server::run(Arc::new(config), ("0.0.0.0", 2222), server).await?;
    Ok(())
}
