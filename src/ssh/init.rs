use std::{collections::HashMap, sync::Arc};

use pty_process::OwnedWritePty;
use russh::{server::Msg, Channel, ChannelId, MethodSet};
use russh_keys::key::{KeyPair, self};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Server {
    #[allow(clippy::type_complexity)]
    pub clients: Arc<Mutex<HashMap<(usize, ChannelId), Channel<Msg>>>>,
    pub channel_pty_writers: Arc<Mutex<HashMap<ChannelId, OwnedWritePty>>>,
    pub id: usize,
    pub options: ServerOptions,
}

#[derive(Clone)]
pub struct ServerOptions {
    pub user: String,
    pub password: Option<String>,
    pub pubkeys: Vec<key::PublicKey>,
}

pub async fn start_ssh_server(options: ServerOptions, keypair: KeyPair) -> anyhow::Result<()> {
    let config = russh::server::Config {
        methods: MethodSet::PASSWORD | MethodSet::PUBLICKEY,
        inactivity_timeout: Some(std::time::Duration::from_secs(60 * 60)),
        auth_rejection_time: std::time::Duration::from_secs(5),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        keys: vec![keypair],
        ..Default::default()
    };

    let server = Server {
        clients: Arc::new(Mutex::new(HashMap::new())),
        channel_pty_writers: Arc::new(Mutex::new(HashMap::new())),
        id: 0,
        options,
    };

    russh::server::run(Arc::new(config), ("0.0.0.0", 2222), server).await?;
    Ok(())
}
