use std::{collections::HashMap, fs::File, sync::Arc};

use clap::{command, Parser};
use russh::MethodSet;
use russh_keys::key::KeyPair;
use tokio::sync::Mutex;

use crate::ssh::{self};

#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"), author, about, version, long_about = None)]
pub struct Command {
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(short, long)]
    pub user: Option<String>,
}

fn init_server_key() -> anyhow::Result<KeyPair> {
    let xdg = xdg::BaseDirectories::with_prefix("quickssh")?;
    let existing_prv_key_path = xdg.find_config_file("private.key");
    if let Some(existing_prv_key_path) = existing_prv_key_path {
        let keypair = russh_keys::load_secret_key(&existing_prv_key_path, None)?;
        log::debug!(
            "Loaded private key from {}",
            existing_prv_key_path.display()
        );
        Ok(keypair)
    } else {
        let keypair = russh_keys::key::KeyPair::generate_ed25519().unwrap();

        let path = xdg.place_config_file("private.key")?;
        let f = File::create(&path)?;
        russh_keys::encode_pkcs8_pem(&keypair, f)?;

        log::info!("Created new ed25519 private key at {}", path.display());

        Ok(keypair)
    }
}

fn init_logger(verbose: bool) {
    let mut tmp = env_logger::builder();
    let mut log_builder = &mut tmp;
    if verbose {
        log_builder = log_builder.filter_level(log::LevelFilter::Debug);
        log::debug!("Debug logging enabled");
    } else {
        log_builder = log_builder.filter_level(log::LevelFilter::Info);
    }
    log_builder.init();
}

pub async fn run(cmd: Command) -> anyhow::Result<()> {
    init_logger(cmd.verbose);

    let keypair = init_server_key()?;

    let config = russh::server::Config {
        methods: MethodSet::PASSWORD | MethodSet::PUBLICKEY,
        inactivity_timeout: Some(std::time::Duration::from_secs(60 * 60)),
        auth_rejection_time: std::time::Duration::from_secs(5),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        keys: vec![keypair],
        ..Default::default()
    };

    let options = ssh::ServerOptions {
        user: cmd.user.unwrap_or(crate::utils::get_username()?),
    };

    let server = ssh::Server {
        clients: Arc::new(Mutex::new(HashMap::new())),
        channel_pty_writers: Arc::new(Mutex::new(HashMap::new())),
        id: 0,
        options,
    };

    log::info!("Listening on 0.0.0.0:2222");
    russh::server::run(Arc::new(config), ("0.0.0.0", 2222), server).await?;
    Ok(())
}
