/// inspired from https://github.com/brandonros/rustbear/blob/master/src/main.rs
use async_trait::async_trait;
use log::info;
use russh::server::{Auth, Msg, Session};
use russh::*;
use russh_keys::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::init::Password;
use super::su_login::su_login;
use super::Server;

impl server::Server for Server {
    type Handler = Self;
    fn new_client(&mut self, addr: Option<std::net::SocketAddr>) -> Self {
        if let Some(addr) = addr {
            log::info!("new client from {}", addr);
        }
        let s = self.clone();
        self.id += 1;
        s
    }
}

#[async_trait]
impl server::Handler for Server {
    type Error = anyhow::Error;

    async fn channel_open_session(
        self,
        channel: Channel<Msg>,
        session: Session,
    ) -> Result<(Self, bool, Session), Self::Error> {
        {
            log::debug!("channel_open_session");
            let mut clients = self.clients.lock().await;
            clients.insert((self.id, channel.id()), channel);
        }
        Ok((self, true, session))
    }
    async fn shell_request(
        self,
        channel_id: ChannelId,
        mut session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        if self.options.no_shell {
            anyhow::bail!("Shell access disabled");
        }

        log::debug!("shell_request channel_id = {channel_id}");

        // create pty
        let pty = pty_process::Pty::new().unwrap();
        if let Err(e) = pty.resize(pty_process::Size::new(24, 80)) {
            log::error!("pty.resize failed: {:?}", e);
        }

        // get pts from pty
        let pts = pty.pts()?;

        // split pty into reader + writer
        let (mut pty_reader, pty_writer) = pty.into_split();

        // insert pty_reader
        self.channel_pty_writers
            .lock()
            .await
            .insert(channel_id, pty_writer);

        // pty_reader.read() -> session_handle.data()
        let session_handle = session.handle().clone();
        tokio::spawn(async move {
            let mut buffer = vec![0; 1024];
            while let Ok(size) = pty_reader.read(&mut buffer).await {
                if size == 0 {
                    log::debug!("pty_reader read 0");
                    // TODO: kill pty + command?
                    let _ = session_handle.close(channel_id).await;
                    break;
                }
                let _ = session_handle
                    .data(channel_id, CryptoVec::from_slice(&buffer[0..size]))
                    .await;
            }
        });

        // Spawn a new shell process in pty
        let mut child = pty_process::Command::new(&self.options.shell)
            .spawn(&pts)
            .map_err(anyhow::Error::new)?;

        // close session when process exits?
        let session_handle = session.handle().clone();
        tokio::spawn(async move {
            let exit_status = child.wait().await.unwrap();
            session_handle
                .exit_status_request(channel_id, exit_status.code().unwrap_or(1) as u32)
                .await
                .unwrap();
            let _ = session_handle.close(channel_id).await;
        });

        // mark request success
        session.request_success();

        Ok((self, session))
    }

    async fn window_change_request(
        self,
        channel_id: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        log::debug!("window_change_request channel_id = {channel_id:?} col_width = {col_width} row_height = {row_height}");
        let mut channel_pty_writers = self.channel_pty_writers.lock().await;
        if let Some(pty_writer) = channel_pty_writers.get_mut(&channel_id) {
            if let Err(e) =
                pty_writer.resize(pty_process::Size::new(row_height as u16, col_width as u16))
            {
                log::error!("pty.resize failed: {:?}", e);
            }
        }
        drop(channel_pty_writers);
        Ok((self, session))
    }

    async fn auth_publickey(
        self,
        user: &str,
        public_key: &russh_keys::key::PublicKey,
    ) -> Result<(Self, Auth), Self::Error> {
        log::info!(
            "auth_publickey: user: {user} public_key: {}",
            public_key.public_key_base64()
        );
        let public_key_is_valid = self.options.pubkeys.contains(public_key);
        if public_key_is_valid {
            Ok((self, server::Auth::Accept))
        } else {
            Ok((
                self,
                Auth::Reject {
                    proceed_with_methods: Some(MethodSet::PASSWORD),
                },
            ))
        }
    }

    async fn auth_none(self, user: &str) -> Result<(Self, Auth), Self::Error> {
        log::debug!("Got authentication attempt (type none) from {user}");
        Ok((
            self,
            Auth::Reject {
                proceed_with_methods: Some(MethodSet::PUBLICKEY | MethodSet::PASSWORD),
            },
        ))
    }

    async fn auth_password(self, user: &str, password: &str) -> Result<(Self, Auth), Self::Error> {
        // if the user wants to authenticate using actual system credentials, let's assume they don't want them logged
        if matches!(self.options.password, Some(Password::Su)) {
            log::info!("auth_password: credentials: {}, [HIDDEN]", user);
        } else {
            log::info!("auth_password: credentials: {}, {}", user, password);
        }

        if user == self.options.user {
            let result = match self.options.password {
                Some(Password::Raw(ref right_password)) => right_password == password,
                Some(Password::Su) => su_login(&self.options.user, password).unwrap(),
                None => false,
            };
            if result {
                return Ok((self, Auth::Accept));
            }
        }

        Ok((
            self,
            Auth::Reject {
                proceed_with_methods: None,
            },
        ))
    }

    async fn channel_close(
        self,
        channel_id: ChannelId,
        session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        log::debug!("channel_close channel_id = {channel_id:?}");
        // TODO: cleanup
        Ok((self, session))
    }

    async fn data(
        self,
        channel_id: ChannelId,
        data: &[u8],
        session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        let mut channel_pty_writers = self.channel_pty_writers.lock().await;
        if let Some(pty_writer) = channel_pty_writers.get_mut(&channel_id) {
            pty_writer
                .write_all(data)
                .await
                .map_err(anyhow::Error::new)?;
        }
        drop(channel_pty_writers);
        Ok((self, session))
    }

    async fn subsystem_request(
        mut self,
        channel_id: ChannelId,
        name: &str,
        mut session: Session,
    ) -> Result<(Self, Session), Self::Error> {
        info!("subsystem: {}", name);

        use super::sftp_events::SftpSession;

        if name == "sftp" {
            if self.options.no_sftp {
                anyhow::bail!("SFTP access disabled");
            }

            let channel = {
                let mut clients = self.clients.lock().await;
                clients.remove(&(self.id, channel_id)).unwrap()
            };

            let sftp = SftpSession::default();
            session.channel_success(channel_id);
            russh_sftp::server::run(channel.into_stream(), sftp).await;
        } else {
            session.channel_failure(channel_id);
        }

        Ok((self, session))
    }
}
