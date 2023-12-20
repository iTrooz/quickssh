/// inspired from https://github.com/AspectUnk/russh-sftp/blob/master/examples/server.rs
use async_trait::async_trait;
use log::info;
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use std::{
    collections::HashMap,
    fs::Metadata,
    io::ErrorKind,
    os::unix::fs::{FileExt, MetadataExt},
};

enum ReadDirRequest {
    Todo(String),
    Done,
}

impl SftpSession {
    fn new_handle(&mut self) -> String {
        self.handle_counter += 1;
        self.handle_counter.to_string()
    }
}

#[derive(Default)]
pub struct SftpSession {
    version: Option<u32>,
    readdir_requests: HashMap<String, ReadDirRequest>,
    file_handles: HashMap<String, std::fs::File>,
    handle_counter: u32,
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

        match std::fs::metadata(path) {
            Ok(md) => Ok(Attrs {
                id,
                attrs: metadata_to_file_attributes(&md),
            }),
            Err(err) if err.kind() == ErrorKind::NotFound => Err(StatusCode::NoSuchFile),
            Err(err) => {
                log::error!("Error occured in stat(): {err}");
                Err(StatusCode::Failure)
            }
        }
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

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        info!("close({}, {})", id, handle);
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_string(),
            language_tag: "en-US".to_string(),
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        info!("opendir({}, {})", id, path);
        let handle = self.new_handle();
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
                    files.push(File::new(
                        path.file_name().into_string().unwrap(),
                        FileAttributes::default(),
                    ));
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
            files: vec![File::new(
                std::fs::canonicalize(path) // TODO replace this function, it doesn't have the behaviour the RFC wants
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                FileAttributes::default(),
            )],
        })
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        info!("open({}, {}, {:?}, {:?})", id, filename, pflags, attrs);
        let handle = self.new_handle();
        self.file_handles
            .insert(handle.clone(), std::fs::File::open(filename).unwrap());
        Ok(Handle { id, handle })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        info!("read({}, {}, {}, {})", id, handle, offset, len);
        if let Some(file) = self.file_handles.get(&handle) {
            let len = len.try_into().unwrap();
            let mut data = vec![0u8; len];
            let read_bytes = file.read_at(&mut data, offset).unwrap();
            data.resize(read_bytes, 0);

            if read_bytes == 0 {
                Err(Self::Error::Eof)
            } else {
                Ok(Data { id, data })
            }
        } else {
            // TODO use SSH_FX_INVALID_HANDLE
            Err(Self::Error::Failure)
        }
    }
}
