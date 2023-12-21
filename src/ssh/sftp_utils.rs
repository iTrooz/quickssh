use russh_sftp::protocol::FileAttributes;
use std::{
    ffi::CString,
    fs::{Metadata, OpenOptions},
    os::unix::fs::{MetadataExt, PermissionsExt},
};

fn timeval_secs(secs: i64) -> libc::timeval {
    libc::timeval {
        tv_sec: secs,
        tv_usec: 0,
    }
}

pub fn apply_file_attributes(path: String, attrs: &FileAttributes) -> anyhow::Result<()> {
    if let Some(size) = attrs.size {
        let file = OpenOptions::new().read(true).write(true).open(&path)?;
        file.set_len(size)?;
    }

    let md = std::fs::metadata(&path)?;
    let cpath = CString::new(path.as_bytes())?;

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
        std::fs::set_permissions(path, PermissionsExt::from_mode(perms))?;
    }

    let mut times = (md.atime(), md.mtime());
    unsafe {
        if let Some(atime) = attrs.atime {
            times.0 = atime.try_into()?;
        }
        if let Some(mtime) = attrs.mtime {
            times.1 = mtime.try_into()?;
        }
        if times != (md.atime(), md.mtime()) {
            libc::utimes(
                cpath.as_ptr(),
                [timeval_secs(times.0), timeval_secs(times.1)].as_ptr(),
            );
        }
    }
    Ok(())
}

pub fn metadata_to_file_attributes(md: &Metadata) -> FileAttributes {
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
