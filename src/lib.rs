extern crate actix_web;
extern crate bytes;
#[macro_use]
extern crate failure;
extern crate futures;
extern crate futures_cpupool;
extern crate futures_fs;
extern crate http;
#[macro_use]
extern crate log;
extern crate mime;

use std::path::PathBuf;

mod error;
mod types;
mod upload;
pub use self::error::Error;
pub use self::types::*;
pub use self::upload::handle_upload;

pub trait FilenameGenerator: Send + Sync {
    fn next_filename(&self, &mime::Mime) -> Option<PathBuf>;
}
