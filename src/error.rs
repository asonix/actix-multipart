use std::{io, num::{ParseFloatError, ParseIntError}, string::FromUtf8Error};

use actix_web::error::{MultipartError, PayloadError};

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Error saving file, {}", _0)]
    FsPool(#[cause] io::Error),
    #[fail(display = "Error parsing payload, {}", _0)]
    Payload(#[cause] PayloadError),
    #[fail(display = "Error in multipart creation, {}", _0)]
    Multipart(#[cause] MultipartError),
    #[fail(display = "Failed to parse field, {}", _0)]
    ParseField(#[cause] FromUtf8Error),
    #[fail(display = "Failed to parse int, {}", _0)]
    ParseInt(#[cause] ParseIntError),
    #[fail(display = "Failed to parse float, {}", _0)]
    ParseFloat(#[cause] ParseFloatError),
    #[fail(display = "Failed to generate filename")]
    GenFilename,
    #[fail(display = "Bad Content-Type")]
    ContentType,
    #[fail(display = "Bad Content-Disposition")]
    ContentDisposition,
    #[fail(display = "Failed to make directory for upload")]
    MkDir,
    #[fail(display = "Failed to parse field name")]
    Field,
    #[fail(display = "Too many fields in request")]
    FieldCount,
    #[fail(display = "Field too large")]
    FieldSize,
    #[fail(display = "Found field with unexpected name or type")]
    FieldType,
    #[fail(display = "Failed to parse filename")]
    Filename,
    #[fail(display = "Too many files in request")]
    FileCount,
    #[fail(display = "File too large")]
    FileSize,
}

impl From<MultipartError> for Error {
    fn from(e: MultipartError) -> Self {
        Error::Multipart(e)
    }
}

impl From<PayloadError> for Error {
    fn from(e: PayloadError) -> Self {
        Error::Payload(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::FsPool(e)
    }
}
