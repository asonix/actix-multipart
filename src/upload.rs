use std::{fs::DirBuilder, os::unix::fs::DirBuilderExt, path::{Path, PathBuf}, sync::Arc};

use actix_web::{multipart, error::PayloadError};
use bytes::{Bytes, BytesMut};
use futures::{Future, Stream, future::{lazy, result, Either, Executor}, sync::oneshot};
use futures_fs::FsPool;
use http::header::CONTENT_DISPOSITION;

use error::Error;
use super::FilenameGenerator;
use types::{self, NamePart};

type MultipartHash = (Vec<NamePart>, MultipartContent);

#[derive(Clone, Debug, PartialEq)]
pub enum MultipartContent {
    File {
        filename: String,
        stored_as: PathBuf,
    },
    Text(String),
    Int(i64),
    Float(f64),
}

pub type MultipartForm = Vec<MultipartHash>;

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

pub struct ContentDisposition {
    name: Option<String>,
    filename: Option<String>,
}

impl ContentDisposition {
    fn empty() -> Self {
        ContentDisposition {
            name: None,
            filename: None,
        }
    }
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
        Ok(_) => (),
        Err(_) => return Either::B(result(Err(Error::MkDir))),
    };

    Either::A(rx.then(|res| match res {
        Ok(res) => res,
        Err(_) => Err(Error::MkDir),
    }).and_then(move |_| {
        let write =
            FsPool::from_executor(form.pool.clone()).write(stored_as.clone(), Default::default());
        field
            .map_err(Error::Multipart)
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
) -> impl Future<Item = MultipartContent, Error = Error>
where
    S: Stream<Item = Bytes, Error = PayloadError>,
{
    trace!("In handle_form_data, term: {:?}", term);
    let max_body_size = 80000;

    field
        .from_err()
        .fold(BytesMut::new(), move |mut acc, bytes| {
            if acc.len() + bytes.len() < max_body_size {
                acc.extend(bytes);
                Ok(acc)
            } else {
                Err(Error::FieldSize)
            }
        })
        .and_then(|bytes| String::from_utf8(bytes.to_vec()).map_err(Error::ParseField))
        .and_then(move |string| {
            trace!("Matching: {:?}", string);
            match term {
                types::FieldTerminator::File(_) => Err(Error::FieldType),
                types::FieldTerminator::Float => string
                    .parse::<f64>()
                    .map(MultipartContent::Float)
                    .map_err(Error::ParseFloat),
                types::FieldTerminator::Int => string
                    .parse::<i64>()
                    .map(MultipartContent::Int)
                    .map_err(Error::ParseInt),
                types::FieldTerminator::Text => Ok(MultipartContent::Text(string)),
            }
        })
}

fn handle_multipart_field<S>(
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
        term => Either::B(handle_form_data(field, term)),
    };

    Either::A(fut.map(|content| (name, content)))
}

pub fn handle_multipart<S>(
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
                        handle_multipart_field(field, form.clone())
                            .map(From::from)
                            .into_stream(),
                    ) as Box<Stream<Item = MultipartHash, Error = Error>>
                }
                multipart::MultipartItem::Nested(m) => {
                    info!("Nested");
                    Box::new(handle_multipart(m, form.clone()))
                        as Box<Stream<Item = MultipartHash, Error = Error>>
                }
            })
            .flatten(),
    )
}

pub fn handle_upload<S>(
    m: multipart::Multipart<S>,
    form: types::Form,
) -> impl Future<Item = MultipartForm, Error = Error>
where
    S: Stream<Item = Bytes, Error = PayloadError> + 'static,
{
    let max_files = 10;
    let max_fields = 100;

    handle_multipart(m, form)
        .fold(
            (Vec::new(), 0, 0),
            move |(mut acc, file_count, field_count), (name, content)| match content {
                MultipartContent::File {
                    filename,
                    stored_as,
                } => {
                    let file_count = file_count + 1;

                    if file_count < max_files {
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
                b @ MultipartContent::Text(_)
                | b @ MultipartContent::Float(_)
                | b @ MultipartContent::Int(_) => {
                    let field_count = field_count + 1;

                    if field_count < max_fields {
                        acc.push((name, b));

                        Ok((acc, file_count, field_count))
                    } else {
                        Err(Error::FieldCount)
                    }
                }
            },
        )
        .map(|(multipart_form, _, _)| multipart_form)
}
