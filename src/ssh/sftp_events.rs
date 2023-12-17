/// inspired from https://github.com/AspectUnk/russh-sftp/blob/master/examples/server.rs
use async_trait::async_trait;
use log::info;
use russh_sftp::protocol::{
    Attrs, File, FileAttributes, Handle, Name, Status, StatusCode, Version,
};
use std::{collections::HashMap, os::unix::fs::MetadataExt};

enum ReadDirRequest {
    Todo(String),
    Done,
}

impl SftpSession {
    fn new_readdir_handle(&mut self) -> String {
        self.readdir_counter += 1;
        self.readdir_counter.to_string()
    }
}

#[derive(Default)]
pub struct SftpSession {
    version: Option<u32>,
    readdir_requests: HashMap<String, ReadDirRequest>,
    readdir_counter: u32,
}

#[async_trait]
impl russh_sftp::server::Handler for SftpSession {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let md = std::fs::metadata(path).unwrap();

        Ok(Attrs {
            id,
            attrs: FileAttributes {
                // TODO finish
                size: Some(md.size()),
                uid: Some(md.uid()),
                user: None,
                gid: Some(md.gid()),
                group: None,
                permissions: None,
                atime: Some(md.atime().try_into().unwrap()),
                mtime: Some(md.mtime().try_into().unwrap()),
            },
        })
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
        let handle = self.new_readdir_handle();
        self.readdir_requests
            .insert(handle.clone(), ReadDirRequest::Todo(path.clone()));
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        info!("readdir handle: {}, id: {}", handle, id);

        let request = self.readdir_requests.get_mut(&handle);
        match request {
            None => {
                // TODO use SSH_FX_INVALID_HANDLE
                Err(Self::Error::Failure)
            }
            Some(ReadDirRequest::Todo(path)) => {
                let paths = std::fs::read_dir(path).unwrap();

                let mut files: Vec<File> = vec![];
                for path in paths {
                    let path = path.unwrap();
                    files.push(File {
                        filename: path.file_name().into_string().unwrap(),
                        attrs: FileAttributes::default(),
                    });
                }

                *request.unwrap() = ReadDirRequest::Done;

                Ok(Name { id, files })
            }
            Some(ReadDirRequest::Done) => {
                self.readdir_requests.remove(&handle);
                Ok(Name { id, files: vec![] })
            }
        }
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
