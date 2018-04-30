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

use std::{collections::HashMap, fs::DirBuilder, os::unix::fs::DirBuilderExt, path::Path,
          sync::{Arc, atomic::{AtomicUsize, Ordering}}};

use actix_web::{multipart, error::PayloadError};
use bytes::{Bytes, BytesMut};
use futures::{Future, Stream, future::{lazy, result, Either, Executor}, sync::oneshot};
use futures_fs::FsPool;
use http::header::CONTENT_DISPOSITION;

use error::Error;
use super::FilenameGenerator;
use types::{self, ContentDisposition, MultipartContent, MultipartForm, MultipartHash, NamePart,
            Value};

fn consolidate(mf: MultipartForm) -> Value {
    mf.into_iter().fold(
        Value::Map(HashMap::new()),
        |mut acc, (mut nameparts, content)| {
            let start_value = Value::from(content);

            nameparts.reverse();
            let value = nameparts
                .into_iter()
                .fold(start_value, |acc, namepart| match namepart {
                    NamePart::Map(name) => {
                        let mut hm = HashMap::new();

                        hm.insert(name, acc);

                        Value::Map(hm)
                    }
                    NamePart::Array => Value::Array(vec![acc]),
                });

            acc.merge(value);
            acc
        },
    )
}

fn parse_multipart_name(name: String) -> Result<Vec<NamePart>, Error> {
    name.split('[')
        .map(|part| {
            if part.len() == 1 && part.ends_with(']') {
                NamePart::Array
            } else if part.ends_with(']') {
                NamePart::Map(part.trim_right_matches(']').to_owned())
            } else {
                NamePart::Map(part.to_owned())
            }
        })
        .fold(Ok(vec![]), |acc, part| match acc {
            Ok(mut v) => {
                if v.len() == 0 && !part.is_map() {
                    return Err(Error::ContentDisposition);
                }

                v.push(part);
                Ok(v)
            }
            Err(e) => Err(e),
        })
}

fn parse_content_disposition<S>(field: &multipart::Field<S>) -> Result<ContentDisposition, Error>
where
    S: Stream<Item = Bytes, Error = PayloadError>,
{
    let content_disposition = if let Some(cd) = field.headers().get(CONTENT_DISPOSITION) {
        cd
    } else {
        return Err(Error::ContentDisposition);
    };

    let content_disposition = if let Ok(cd) = content_disposition.to_str() {
        cd
    } else {
        return Err(Error::ContentDisposition);
    };

    Ok(content_disposition
        .split(';')
        .skip(1)
        .filter_map(|section| {
            let mut parts = section.splitn(2, '=');

            let key = if let Some(key) = parts.next() {
                key.trim()
            } else {
                return None;
            };

            let val = if let Some(val) = parts.next() {
                val.trim()
            } else {
                return None;
            };

            Some((key, val.trim_matches('"')))
        })
        .fold(ContentDisposition::empty(), |mut acc, (key, val)| {
            if key == "name" {
                acc.name = Some(val.to_owned());
            } else if key == "filename" {
                acc.filename = Some(val.to_owned());
            }
            acc
        }))
}

fn handle_file_upload<S>(
    field: multipart::Field<S>,
    gen: Arc<FilenameGenerator>,
    filename: Option<String>,
    form: types::Form,
) -> impl Future<Item = MultipartContent, Error = Error>
where
    S: Stream<Item = Bytes, Error = PayloadError>,
{
    let filename = match filename {
        Some(filename) => filename,
        None => return Either::B(result(Err(Error::Filename))),
    };

    let path: &Path = filename.as_ref();
    let filename = path.file_name().and_then(|filename| filename.to_str());

    let filename = if let Some(filename) = filename {
        filename.to_owned()
    } else {
        return Either::B(result(Err(Error::Filename)));
    };

    let stored_as = match gen.next_filename(field.content_type()) {
        Some(file_path) => file_path,
        None => return Either::B(result(Err(Error::GenFilename))),
    };

    let mut stored_dir = stored_as.clone();
    stored_dir.pop();

    let (tx, rx) = oneshot::channel();

    match form.pool.execute(Box::new(lazy(move || {
        let res = DirBuilder::new()
            .recursive(true)
            .mode(0o755)
            .create(stored_dir.clone())
            .map_err(|_| Error::MkDir);

        tx.send(res).map_err(|_| ())
    }))) {
        | Ok(_) => (),
        Err(_) => return Either::B(result(Err(Error::MkDir))),
    };

    let counter = Arc::new(AtomicUsize::new(0));

    Either::A(rx.then(|res| match res {
        Ok(res) => res,
        Err(_) => Err(Error::MkDir),
    }).and_then(move |_| {
        let write =
            FsPool::from_executor(form.pool.clone()).write(stored_as.clone(), Default::default());
        field
            .map_err(Error::Multipart)
            .and_then(move |bytes| {
                let size = counter.fetch_add(bytes.len(), Ordering::Relaxed) + bytes.len();

                if size > form.max_file_size {
                    Err(Error::FileSize)
                } else {
                    Ok(bytes)
                }
            })
            .forward(write)
            .map(move |_| MultipartContent::File {
                filename,
                stored_as,
            })
    }))
}

fn handle_form_data<S>(
    field: multipart::Field<S>,
    term: types::FieldTerminator,
    form: types::Form,
) -> impl Future<Item = MultipartContent, Error = Error>
where
    S: Stream<Item = Bytes, Error = PayloadError>,
{
    trace!("In handle_form_data, term: {:?}", term);
    let term2 = term.clone();

    field
        .from_err()
        .fold(BytesMut::new(), move |mut acc, bytes| {
            if acc.len() + bytes.len() < form.max_field_size {
                acc.extend(bytes);
                Ok(acc)
            } else {
                Err(Error::FieldSize)
            }
        })
        .and_then(move |bytes| match term {
            types::FieldTerminator::Bytes => Ok(MultipartContent::Bytes(bytes.freeze())),
            _ => String::from_utf8(bytes.to_vec())
                .map_err(Error::ParseField)
                .map(MultipartContent::Text),
        })
        .and_then(move |content| {
            trace!("Matching: {:?}", content);
            match content {
                types::MultipartContent::Text(string) => match term2 {
                    types::FieldTerminator::File(_) => Err(Error::FieldType),
                    types::FieldTerminator::Bytes => Err(Error::FieldType),
                    types::FieldTerminator::Float => string
                        .parse::<f64>()
                        .map(MultipartContent::Float)
                        .map_err(Error::ParseFloat),
                    types::FieldTerminator::Int => string
                        .parse::<i64>()
                        .map(MultipartContent::Int)
                        .map_err(Error::ParseInt),
                    types::FieldTerminator::Text => Ok(MultipartContent::Text(string)),
                },
                b @ types::MultipartContent::Bytes(_) => Ok(b),
                _ => Err(Error::FieldType),
            }
        })
}

fn handle_stream_field<S>(
    field: multipart::Field<S>,
    form: types::Form,
) -> impl Future<Item = MultipartHash, Error = Error>
where
    S: Stream<Item = Bytes, Error = PayloadError>,
{
    let content_disposition = match parse_content_disposition(&field) {
        Ok(cd) => cd,
        Err(e) => return Either::B(result(Err(e))),
    };

    let name = match content_disposition.name {
        Some(name) => name,
        None => return Either::B(result(Err(Error::Field))),
    };

    let name = match parse_multipart_name(name) {
        Ok(name) => name,
        Err(e) => return Either::B(result(Err(e))),
    };

    let term = match form.valid_field(name.iter().cloned().collect()) {
        Some(term) => term,
        None => return Either::B(result(Err(Error::FieldType))),
    };

    let fut = match term {
        types::FieldTerminator::File(gen) => Either::A(handle_file_upload(
            field,
            gen,
            content_disposition.filename,
            form,
        )),
        term => Either::B(handle_form_data(field, term, form)),
    };

    Either::A(fut.map(|content| (name, content)))
}

fn handle_stream<S>(
    m: multipart::Multipart<S>,
    form: types::Form,
) -> Box<Stream<Item = MultipartHash, Error = Error>>
where
    S: Stream<Item = Bytes, Error = PayloadError> + 'static,
{
    Box::new(
        m.map_err(Error::from)
            .map(move |item| match item {
                multipart::MultipartItem::Field(field) => {
                    info!("Field: {:?}", field);
                    Box::new(
                        handle_stream_field(field, form.clone())
                            .map(From::from)
                            .into_stream(),
                    ) as Box<Stream<Item = MultipartHash, Error = Error>>
                }
                multipart::MultipartItem::Nested(m) => {
                    info!("Nested");
                    Box::new(handle_stream(m, form.clone()))
                        as Box<Stream<Item = MultipartHash, Error = Error>>
                }
            })
            .flatten(),
    )
}

/// Handle multipart streams from Actix Web
pub fn handle_multipart<S>(
    m: multipart::Multipart<S>,
    form: types::Form,
) -> impl Future<Item = Value, Error = Error>
where
    S: Stream<Item = Bytes, Error = PayloadError> + 'static,
{
    handle_stream(m, form.clone())
        .fold(
            (Vec::new(), 0, 0),
            move |(mut acc, file_count, field_count), (name, content)| match content {
                MultipartContent::File {
                    filename,
                    stored_as,
                } => {
                    let file_count = file_count + 1;

                    if file_count < form.max_files {
                        acc.push((
                            name,
                            MultipartContent::File {
                                filename,
                                stored_as,
                            },
                        ));

                        Ok((acc, file_count, field_count))
                    } else {
                        Err(Error::FileCount)
                    }
                }
                b @ MultipartContent::Bytes(_)
                | b @ MultipartContent::Text(_)
                | b @ MultipartContent::Float(_)
                | b @ MultipartContent::Int(_) => {
                    let field_count = field_count + 1;

                    if field_count < form.max_fields {
                        acc.push((name, b));

                        Ok((acc, file_count, field_count))
                    } else {
                        Err(Error::FieldCount)
                    }
                }
            },
        )
        .map(|(multipart_form, _, _)| consolidate(multipart_form))
}
