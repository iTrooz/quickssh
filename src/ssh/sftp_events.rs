/// inspired from https://github.com/AspectUnk/russh-sftp/blob/master/examples/server.rs
use async_trait::async_trait;
use log::info;
use russh_sftp::protocol::{
    Attrs, File, FileAttributes, Handle, Name, Status, StatusCode, Version,
};
use std::{collections::HashMap, fs::Metadata, os::unix::fs::MetadataExt};

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

fn metadata_to_file_attributes(md: &Metadata) -> FileAttributes {
    let user = users::get_user_by_uid(md.uid())
        .unwrap()
        .name()
        .to_string_lossy()
        .to_string();
    let group = users::get_group_by_gid(md.gid())
        .unwrap()
        .name()
        .to_string_lossy()
        .to_string();
    let mut attrs = FileAttributes::from(md);
    attrs.user = Some(user);
    attrs.group = Some(group);

    attrs
}

#[async_trait]
impl russh_sftp::server::Handler for SftpSession {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        info!("stat({}, {})", id, path);

        let md = std::fs::metadata(path).unwrap();
        Ok(Attrs {
            id,
            attrs: metadata_to_file_attributes(&md),
        })
    }

    // does not follow if path is symlink
    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        info!("lstat({}, {})", id, path);

        let md = std::fs::symlink_metadata(path).unwrap();
        Ok(Attrs {
            id,
            attrs: metadata_to_file_attributes(&md),
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
        info!("close({}, {})", id, _handle);
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_string(),
            language_tag: "en-US".to_string(),
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        info!("opendir({}, {})", id, path);
        let handle = self.new_readdir_handle();
        self.readdir_requests
            .insert(handle.clone(), ReadDirRequest::Todo(path.clone()));
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        info!("readdir({}, {})", id, handle);

        let request = self.readdir_requests.get_mut(&handle);
        match request {
            None => {
                // TODO use SSH_FX_INVALID_HANDLE
                Err(Self::Error::Failure)
            }
            Some(ReadDirRequest::Todo(path)) => {
                let paths_res = std::fs::read_dir(path);

                let paths = match paths_res {
                    Ok(paths) => paths,
                    Err(ref err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                        return Err(StatusCode::PermissionDenied);
                    }
                    Err(ref err) => {
                        log::error!("readdir({}, {}) failed: {}", id, handle, err);
                        return Err(StatusCode::Failure);
                    }
                };

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
        info!("realpath({}, {})", id, path);
        Ok(Name {
            id,
            files: vec![File {
                filename: std::fs::canonicalize(path) // TODO replace this function, it doesn't have the behaviour the RFC wants
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                attrs: FileAttributes::default(),
            }],
        })
    }
}
