/// inspired from https://github.com/AspectUnk/russh-sftp/blob/master/examples/server.rs
use super::sftp_utils::*;
use async_trait::async_trait;
use log::info;
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use std::{
    collections::HashMap,
    fs::{OpenOptions, ReadDir},
    io::ErrorKind,
    os::unix::fs::FileExt,
};

enum ReadDirRequest {
    Todo(ReadDir),
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
    dir_handles: HashMap<String, ReadDirRequest>,
    file_handles: HashMap<String, std::fs::File>,
    handle_counter: u32,
}

/// same as below, but for anyhow
fn tr_ah<T>(res: anyhow::Result<T>) -> Result<T, StatusCode> {
    match res {
        Ok(value) => Ok(value),
        Err(err) => {
            log::error!("An error occured: {err}");
            Err(StatusCode::Failure)
        }
    }
}
/// "tr" means "translate"
/// This functions translates any kind of error into a StatusCode
/// If someone knows how to do this using the ? operator, please open a PR
fn tr<T>(res: Result<T, impl std::error::Error>) -> Result<T, StatusCode> {
    match res {
        Ok(value) => Ok(value),
        Err(err) => {
            log::error!("An error occured: {err}");
            Err(StatusCode::Failure)
        }
    }
}

fn status_ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_string(),
        language_tag: "en-US".to_string(),
    }
}

#[async_trait]
impl russh_sftp::server::Handler for SftpSession {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        let bt = std::backtrace::Backtrace::force_capture();
        log::warn!(
            "Client asked for an unimplemented operation. Stacktrace: {}",
            bt
        );

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

        match std::fs::symlink_metadata(path) {
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

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        log::info!("fstat({}, {})", id, handle);

        if let Some(file) = self.file_handles.get(&handle) {
            match file.metadata() {
                Ok(md) => Ok(Attrs {
                    id,
                    attrs: metadata_to_file_attributes(&md),
                }),
                Err(err) if err.kind() == ErrorKind::NotFound => Err(StatusCode::NoSuchFile),
                Err(err) => {
                    log::error!("Error occured in fstat(): {err}");
                    Err(StatusCode::Failure)
                }
            }
        } else {
            log::warn!("Client requested fstat() on non-existant handle: {handle}");
            // TODO use SSH_FX_INVALID_HANDLE
            Err(Self::Error::Failure)
        }
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        log::info!("setstat({}, {}, {:?})", id, path, attrs);
        tr_ah(apply_file_attributes(path, &attrs))?;
        Ok(status_ok(id))
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
        if self.file_handles.remove(&handle).is_some() || self.dir_handles.remove(&handle).is_some()
        {
            Ok(status_ok(id))
        } else {
            Err(StatusCode::Failure)
        }
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        info!("opendir({}, {})", id, path);
        let handle = self.new_handle();

        let paths_res = std::fs::read_dir(path);

        let paths = match paths_res {
            Ok(paths) => paths,
            Err(ref err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err(StatusCode::PermissionDenied);
            }
            Err(ref err) => {
                log::error!("opendir({}, {}) failed: {}", id, handle, err);
                return Err(StatusCode::Failure);
            }
        };

        self.dir_handles
            .insert(handle.clone(), ReadDirRequest::Todo(paths));
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        info!("readdir({}, {})", id, handle);

        let request = self.dir_handles.get_mut(&handle);
        match request {
            None => {
                log::warn!("Client requested readdir() on non-existant handle: {handle}");
                // TODO use SSH_FX_INVALID_HANDLE
                Err(Self::Error::Failure)
            }
            Some(request) => match request {
                ReadDirRequest::Todo(paths) => {
                    let mut files: Vec<File> = vec![];
                    for path in paths {
                        let path = tr(path)?;
                        match path.file_name().into_string() {
                            Ok(path_str) => {
                                files.push(File::new(
                                    path_str,
                                    FileAttributes::from(&tr(path.metadata())?),
                                ));
                            }
                            Err(_) => {
                                log::error!("Failed to convert file path '{path:?}' to string");
                            }
                        }
                    }

                    *request = ReadDirRequest::Done;

                    Ok(Name { id, files })
                }
                ReadDirRequest::Done => Err(StatusCode::Eof),
            },
        }
    }

    async fn realpath(&mut self, id: u32, mut path: String) -> Result<Name, Self::Error> {
        info!("realpath({}, {})", id, path);
        // TODO replace std::fs::canonicalize(), it doesn't have the behaviour the RFC wants

        if path.is_empty() {
            path = ".".to_string();
        }

        match std::fs::canonicalize(path) {
            Ok(path) => Ok(Name {
                id,
                files: vec![File::new(
                    path.to_string_lossy().to_string(),
                    FileAttributes::default(),
                )],
            }),
            Err(err) => {
                log::error!("error occured in realpath(): {err}");
                Err(StatusCode::Failure)
            }
        }
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

        self.file_handles.insert(
            handle.clone(),
            tr(OpenOptions::from(pflags).open(filename))?,
        );
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
            let len = tr(len.try_into())?;
            let mut data = vec![0u8; len];
            let read_bytes = tr(file.read_at(&mut data, offset))?;
            data.resize(read_bytes, 0);

            if read_bytes == 0 {
                Err(Self::Error::Eof)
            } else {
                Ok(Data { id, data })
            }
        } else {
            log::warn!("Client requested read() on non-existant handle: {handle}");
            // TODO use SSH_FX_INVALID_HANDLE
            Err(Self::Error::Failure)
        }
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        info!(
            "write({}, {}, {}, [data of length {}])",
            id,
            handle,
            offset,
            data.len()
        );
        if let Some(file) = self.file_handles.get(&handle) {
            tr(file.write_at(&data, offset))?;

            Ok(status_ok(id))
        } else {
            log::warn!("Client requested write() on non-existant handle: {handle}");
            // TODO use SSH_FX_INVALID_HANDLE
            Err(Self::Error::Failure)
        }
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        tr(std::fs::remove_file(filename))?;
        Ok(status_ok(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        tr(std::fs::rename(oldpath, newpath))?;
        Ok(status_ok(id))
    }

    async fn readlink(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        info!("readlink({}, {})", id, path);
        let real_path = tr(std::fs::read_link(path))?;

        if real_path.exists() {
            Ok(Name {
                id,
                files: vec![File::new(
                    real_path.to_string_lossy().to_string(),
                    FileAttributes::from(&tr(real_path.metadata())?),
                )],
            })
        } else {
            Err(StatusCode::NoSuchFile)
        }
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        info!("mkdir({}, {}, {:?})", id, path, attrs);
        tr(std::fs::create_dir(path))?;
        Ok(status_ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        info!("rmdir({}, {})", id, path);
        tr(std::fs::remove_dir(path))?;
        Ok(status_ok(id))
    }
}
