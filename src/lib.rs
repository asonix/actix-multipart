/*
 * This file is part of Actix Form Data.
 *
 * Copyright © 2018 Riley Trautman
 *
 * Actix Form Data is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * Actix Form Data is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with Actix Form Data.  If not, see <http://www.gnu.org/licenses/>.
 */

//! # Actix Form Data
//! A library for retrieving form data from Actix Web's multipart streams. It can stream
//! uploaded files onto the filesystem (its main purpose), but it can also parse associated
//! form data.
//!
//! # Example
//!
//!```rust
//! extern crate actix_web;
//! extern crate form_data;
//! extern crate futures;
//! extern crate mime;
//!
//! use std::path::PathBuf;
//!
//! use actix_web::{http, server, App, AsyncResponder, HttpMessage, HttpRequest, HttpResponse, State};
//! use form_data::{handle_multipart, Error, Field, FilenameGenerator, Form};
//! use futures::Future;
//!
//! struct Gen;
//!
//! impl FilenameGenerator for Gen {
//!     fn next_filename(&self, _: &mime::Mime) -> Option<PathBuf> {
//!         let mut p = PathBuf::new();
//!         p.push("examples/filename.png");
//!         Some(p)
//!     }
//! }
//!
//! fn upload(
//!     req: HttpRequest<Form>,
//!     state: State<Form>,
//! ) -> Box<Future<Item = HttpResponse, Error = Error>> {
//!     handle_multipart(req.multipart(), state.clone())
//!         .map(|uploaded_content| {
//!             println!("Uploaded Content: {:?}", uploaded_content);
//!             HttpResponse::Created().finish()
//!         })
//!         .responder()
//! }
//!
//! fn main() {
//!     let form = Form::new()
//!         .field("Hey", Field::text())
//!         .field(
//!             "Hi",
//!             Field::map()
//!                 .field("One", Field::int())
//!                 .field("Two", Field::float())
//!                 .finalize(),
//!         )
//!         .field("files", Field::array(Field::file(Gen)));
//!
//!     println!("{:?}", form);
//!
//!     server::new(move || {
//!         App::with_state(form.clone())
//!             .resource("/upload", |r| r.method(http::Method::POST).with2(upload))
//!     }).bind("127.0.0.1:8080")
//!         .unwrap();
//!         // .run()
//! }
//!```
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
#[cfg(feature = "with-serde")]
extern crate serde;
#[cfg(feature = "with-serde")]
#[macro_use]
extern crate serde_derive;

use std::path::PathBuf;

mod error;
mod types;
mod upload;
pub use self::error::Error;
pub use self::types::*;
pub use self::upload::handle_multipart;

/// A trait for types that produce filenames for uploade files
///
/// Currently, the mime type provided to the `next_filename` method is guessed from the uploaded
/// file's original filename, so relying on this to be 100% accurate is probably a bad idea.
pub trait FilenameGenerator: Send + Sync {
    fn next_filename(&self, &mime::Mime) -> Option<PathBuf>;
}
