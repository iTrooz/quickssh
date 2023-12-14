use std::collections::HashMap;
use std::sync::Arc;

use russh::*;
use tokio::sync::Mutex;

pub mod ssh;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();
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
