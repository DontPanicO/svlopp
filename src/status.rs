// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    io,
    os::fd::AsFd,
    path::{Path, PathBuf},
};

use rustix::fs::{Mode, OFlags, fsync, open, rename};

use crate::utils::write_all;

/// Holds the paths used to maintain the status file.
///
/// The status file is written atomically by first writing to a
/// temporary file and then renaming it over the final path.
/// Both paths are precomputed to avoid repeated allocations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusFilePath {
    /// The status file path
    path: PathBuf,
    /// The temporary file path
    tmp_path: PathBuf,
}

impl StatusFilePath {
    /// For example:
    /// ```
    /// use std::path::PathBuf;
    /// use svlopp::status::StatusFilePath;
    ///
    /// const STATUS_FILE_NAME: &str = "status";
    ///
    /// let run_dir = PathBuf::from("/run/svlopp");
    /// let status_file_path = StatusFilePath::new(run_dir.join(STATUS_FILE_NAME));
    /// ```
    #[inline(always)]
    pub fn new(path: PathBuf) -> Self {
        Self {
            tmp_path: path.with_extension("tmp"),
            path,
        }
    }

    #[inline(always)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[inline(always)]
    pub fn tmp_path(&self) -> &Path {
        &self.tmp_path
    }
}

pub fn write_status_file(
    path: &StatusFilePath,
    content: &str,
) -> io::Result<()> {
    let fd = open(
        path.tmp_path(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC | OFlags::CLOEXEC,
        Mode::from_bits_truncate(0o644),
    )?;
    write_all(fd.as_fd(), content.as_bytes())?;
    fsync(&fd)?;
    rename(path.tmp_path(), path.path())?;
    Ok(())
}
