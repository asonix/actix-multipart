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

use std::{io, num::{ParseFloatError, ParseIntError}, string::FromUtf8Error};

use actix_web::{HttpResponse, error::{MultipartError, PayloadError, ResponseError}};

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

impl ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        match *self {
            Error::FsPool(_) => HttpResponse::InternalServerError().finish(),
            Error::Payload(ref e) => ResponseError::error_response(e),
            Error::Multipart(ref e) => ResponseError::error_response(e),
            Error::ParseField(_) | Error::ParseInt(_) | Error::ParseFloat(_) => {
                HttpResponse::BadRequest().finish()
            }
            Error::GenFilename | Error::MkDir => HttpResponse::InternalServerError().finish(),
            Error::ContentType
            | Error::ContentDisposition
            | Error::Field
            | Error::FieldCount
            | Error::FieldSize
            | Error::FieldType
            | Error::Filename
            | Error::FileCount
            | Error::FileSize => HttpResponse::BadRequest().finish(),
        }
    }
}
