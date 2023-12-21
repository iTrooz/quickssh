/// inspired from https://github.com/AspectUnk/russh-sftp/blob/master/examples/server.rs
use async_trait::async_trait;
use log::info;
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use std::{
    collections::HashMap,
    ffi::CString,
    fs::{Metadata, OpenOptions, ReadDir},
    io::ErrorKind,
    os::unix::fs::{FileExt, MetadataExt, PermissionsExt},
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
    readdir_requests: HashMap<String, ReadDirRequest>,
    file_handles: HashMap<String, std::fs::File>,
    handle_counter: u32,
}

fn timeval_secs(secs: i64) -> libc::timeval {
    libc::timeval {
        tv_sec: secs,
        tv_usec: 0,
    }
}

fn apply_file_attributes(path: String, attrs: &FileAttributes) {
    if let Some(size) = attrs.size {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(size).unwrap();
    }

    let md = std::fs::metadata(&path).unwrap();
    let cpath = CString::new(path.as_bytes()).unwrap();

    // modify owner/group
    {
        let mut uid_gid = (md.uid(), md.gid());

        if let Some(uid) = attrs.uid {
            uid_gid.0 = uid;
        } else if let Some(ref user) = attrs.user {
            uid_gid.0 = users::get_user_by_name(user).unwrap().uid();
        }

        if let Some(gid) = attrs.gid {
            uid_gid.1 = gid;
        } else if let Some(ref group) = attrs.group {
            uid_gid.1 = users::get_group_by_name(group).unwrap().gid();
        }

        if uid_gid != (md.uid(), md.gid()) {
            unsafe {
                libc::chown(cpath.as_ptr(), uid_gid.0, uid_gid.1);
            }
        }
    }

    if let Some(perms) = attrs.permissions {
        std::fs::set_permissions(path, PermissionsExt::from_mode(perms)).unwrap();
    }

    let mut times = (md.atime(), md.mtime());
    unsafe {
        if let Some(atime) = attrs.atime {
            times.0 = atime.try_into().unwrap();
        }
        if let Some(mtime) = attrs.mtime {
            times.1 = mtime.try_into().unwrap();
        }
        if times != (md.atime(), md.mtime()) {
            libc::utimes(
                cpath.as_ptr(),
                [timeval_secs(times.0), timeval_secs(times.1)].as_ptr(),
            );
        }
    }
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
        apply_file_attributes(path, &attrs);
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_string(),
            language_tag: "en-US".to_string(),
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

        self.readdir_requests
            .insert(handle.clone(), ReadDirRequest::Todo(paths));
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
            Some(ReadDirRequest::Todo(paths)) => {
                let mut files: Vec<File> = vec![];
                for path in paths {
                    let path = path.unwrap();
                    files.push(File::new(
                        path.file_name().into_string().unwrap(),
                        FileAttributes::from(&path.metadata().unwrap()),
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
        // TODO replace std::fs::canonicalize(), it doesn't have the behaviour the RFC wants
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
            OpenOptions::from(pflags).open(filename).unwrap(),
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
            file.write_at(&data, offset).unwrap();

            Ok(Status {
                id,
                status_code: StatusCode::Ok,
                error_message: "Ok".to_string(),
                language_tag: "en-US".to_string(),
            })
        } else {
            // TODO use SSH_FX_INVALID_HANDLE
            Err(Self::Error::Failure)
        }
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        std::fs::remove_file(filename).unwrap();
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_string(),
            language_tag: "en-US".to_string(),
        })
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        std::fs::rename(oldpath, newpath).unwrap();
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_string(),
            language_tag: "en-US".to_string(),
        })
    }

    async fn readlink(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        info!("readlink({}, {})", id, path);
        let real_path = std::fs::read_link(path).unwrap();

        if real_path.exists() {
            Ok(Name {
                id,
                files: vec![File::new(
                    real_path.to_string_lossy().to_string(),
                    FileAttributes::from(&real_path.metadata().unwrap()),
                )],
            })
        } else {
            Err(StatusCode::NoSuchFile)
        }
    }
}
