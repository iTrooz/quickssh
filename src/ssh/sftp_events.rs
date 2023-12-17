/// inspired from https://github.com/AspectUnk/russh-sftp/blob/master/examples/server.rs
use async_trait::async_trait;
use log::info;
use russh_sftp::protocol::{File, FileAttributes, Handle, Name, Status, StatusCode, Version};
use std::collections::HashMap;

#[derive(Default)]
pub struct SftpSession {
    version: Option<u32>,
    root_dir_read_done: bool,
}

#[async_trait]
impl russh_sftp::server::Handler for SftpSession {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        version: u32,
        extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        if self.version.is_some() {
            log::error!("duplicate SSH_FXP_VERSION packet");
            return Err(StatusCode::ConnectionLost);
        }

        self.version = Some(version);
        info!("version: {:?}, extensions: {:?}", self.version, extensions);
        Ok(Version::new())
    }

    async fn close(&mut self, id: u32, _handle: String) -> Result<Status, Self::Error> {
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_string(),
            language_tag: "en-US".to_string(),
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        info!("opendir: {}", path);
        self.root_dir_read_done = false;
        Ok(Handle { id, handle: path })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        info!("readdir handle: {}", handle);
        if handle == "/" && !self.root_dir_read_done {
            self.root_dir_read_done = true;
            return Ok(Name {
                id,
                files: vec![
                    File {
                        filename: "foo".to_string(),
                        attrs: FileAttributes::default(),
                    },
                    File {
                        filename: "bar".to_string(),
                        attrs: FileAttributes::default(),
                    },
                ],
            });
        }
        Ok(Name { id, files: vec![] })
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        info!("realpath: {}", path);
        Ok(Name {
            id,
            files: vec![File {
                filename: "/".to_string(),
                attrs: FileAttributes::default(),
            }],
        })
    }
}
