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

use std::{fmt, collections::{HashMap, VecDeque}, path::PathBuf, sync::Arc};

use bytes::Bytes;
use futures::{Future, future::{ExecuteError, Executor}};
use futures_cpupool::CpuPool;

use super::FilenameGenerator;

/// The result of a succesfull parse through a given multipart stream.
///
/// This type represents all possible variations in structure of a Multipart Form.
///
/// # Example usage
///
/// ```rust
/// # use form_data::Value;
/// # use std::collections::HashMap;
/// # let mut hm = HashMap::new();
/// # hm.insert("field-name".to_owned(), Value::Int(32));
/// # let value = Value::Map(hm);
/// match value {
///     Value::Map(mut hashmap) => {
///         match hashmap.remove("field-name") {
///             Some(value) => match value {
///                 Value::Int(integer) => println!("{}", integer),
///                 _ => (),
///             }
///             None => (),
///         }
///     }
///     _ => (),
/// }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Map(HashMap<String, Value>),
    Array(Vec<Value>),
    File(String, PathBuf),
    Text(String),
    Int(i64),
    Float(f64),
    Bytes(Bytes),
}

impl Value {
    pub(crate) fn merge(&mut self, rhs: Self) {
        match (self, rhs) {
            (&mut Value::Map(ref mut hm), Value::Map(ref other)) => {
                other.into_iter().fold(hm, |hm, (key, value)| {
                    if hm.contains_key(key) {
                        hm.get_mut(key).unwrap().merge(value.clone())
                    } else {
                        hm.insert(key.to_owned(), value.clone());
                    }

                    hm
                });
            }
            (&mut Value::Array(ref mut v), Value::Array(ref other)) => {
                v.extend(other.clone());
            }
            _ => (),
        }
    }
}

impl From<MultipartContent> for Value {
    fn from(mc: MultipartContent) -> Self {
        match mc {
            MultipartContent::File {
                filename,
                stored_as,
            } => Value::File(filename, stored_as),
            MultipartContent::Text(string) => Value::Text(string),
            MultipartContent::Int(i) => Value::Int(i),
            MultipartContent::Float(f) => Value::Float(f),
            MultipartContent::Bytes(b) => Value::Bytes(b),
        }
    }
}

/// The field type represents a field in the form-data that is allowed to be parsed.
#[derive(Clone)]
pub enum Field {
    Array(Array),
    File(Arc<FilenameGenerator>),
    Map(Map),
    Int,
    Float,
    Text,
    Bytes,
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Field::Array(ref arr) => write!(f, "Array({:?})", arr),
            Field::File(_) => write!(f, "File(filename_generator)"),
            Field::Map(ref map) => write!(f, "Map({:?})", map),
            Field::Int => write!(f, "Int"),
            Field::Float => write!(f, "Float"),
            Field::Text => write!(f, "Text"),
            Field::Bytes => write!(f, "Bytes"),
        }
    }
}

impl Field {
    /// Add a File field with a name generator.
    ///
    /// The name generator will be called for each file matching this field's key. Keep in mind
    /// that each key/file pair will have it's own name-generator, so sharing a name-generator
    /// between fields is up to the user.
    ///
    /// # Example
    /// ```rust
    /// # extern crate mime;
    /// # extern crate form_data;
    /// # use std::path::{Path, PathBuf};
    /// # use form_data::{Form, Field, FilenameGenerator};
    ///
    /// struct Gen;
    ///
    /// impl FilenameGenerator for Gen {
    ///     fn next_filename(&self, _: &mime::Mime) -> Option<PathBuf> {
    ///         Some(AsRef::<Path>::as_ref("path.png").to_owned())
    ///     }
    /// }
    ///
    /// fn main() {
    ///     let name_generator = Gen;
    ///     let form = Form::new()
    ///         .field("file-field", Field::file(name_generator));
    /// }
    /// ```
    pub fn file<T>(gen: T) -> Self
    where
        T: FilenameGenerator + 'static,
    {
        Field::File(Arc::new(gen))
    }

    /// Add a Text field to a form
    ///
    /// # Example
    /// ```rust
    /// # extern crate form_data;
    /// # use form_data::{Form, Field};
    /// # fn main() {
    /// let form = Form::new().field("text-field", Field::text());
    /// # }
    pub fn text() -> Self {
        Field::Text
    }

    /// Add an Int field to a form
    ///
    /// # Example
    /// ```rust
    /// # extern crate form_data;
    /// # use form_data::{Form, Field};
    /// # fn main() {
    /// let form = Form::new().field("int-field", Field::int());
    /// # }
    /// ```
    pub fn int() -> Self {
        Field::Int
    }

    /// Add a Float field to a form
    ///
    /// # Example
    /// ```rust
    /// # extern crate form_data;
    /// # use form_data::{Form, Field};
    /// # fn main() {
    /// let form = Form::new().field("float-field", Field::float());
    /// # }
    /// ```
    pub fn float() -> Self {
        Field::Float
    }

    /// Add a Bytes field to a form
    ///
    /// # Example
    /// ```rust
    /// # extern crate form_data;
    /// # use form_data::{Form, Field};
    /// # fn main() {
    /// let form = Form::new().field("bytes-field", Field::bytes());
    /// # }
    /// ```
    pub fn bytes() -> Self {
        Field::Bytes
    }

    /// Add an Array to a form
    ///
    /// # Example
    /// ```rust
    /// # extern crate form_data;
    /// # use form_data::{Form, Field};
    /// # fn main() {
    /// let form = Form::new()
    ///     .field(
    ///         "array-field",
    ///         Field::array(Field::text())
    ///     );
    /// # }
    /// ```
    pub fn array(field: Field) -> Self {
        Field::Array(Array::new(field))
    }

    /// Add a Map to a form
    ///
    /// # Example
    /// ```rust
    /// # extern crate form_data;
    /// # use form_data::{Form, Field};
    /// # fn main() {
    /// let form = Form::new()
    ///     .field(
    ///         "map-field",
    ///         Field::map()
    ///             .field("sub-field", Field::text())
    ///             .field("sub-field-two", Field::text())
    ///             .finalize()
    ///     );
    /// # }
    /// ```
    pub fn map() -> Map {
        Map::new()
    }

    fn valid_field(&self, name: VecDeque<NamePart>) -> Option<FieldTerminator> {
        trace!("Checking {:?} and {:?}", self, name);
        match *self {
            Field::Array(ref arr) => arr.valid_field(name),
            Field::Map(ref map) => map.valid_field(name),
            Field::File(ref gen) => if name.is_empty() {
                Some(FieldTerminator::File(Arc::clone(gen)))
            } else {
                None
            },
            Field::Int => if name.is_empty() {
                Some(FieldTerminator::Int)
            } else {
                None
            },
            Field::Float => if name.is_empty() {
                Some(FieldTerminator::Float)
            } else {
                None
            },
            Field::Text => if name.is_empty() {
                Some(FieldTerminator::Text)
            } else {
                None
            },
            Field::Bytes => if name.is_empty() {
                Some(FieldTerminator::Bytes)
            } else {
                None
            },
        }
    }
}

/// A definition of an array of type `Field` to be parsed from form data.
///
/// The `Array` type should only be constructed in the context of a Form. See the `Form`
/// documentation for more information.
#[derive(Debug, Clone)]
pub struct Array {
    inner: Box<Field>,
}

impl Array {
    fn new(field: Field) -> Self {
        Array {
            inner: Box::new(field),
        }
    }

    fn valid_field(&self, mut name: VecDeque<NamePart>) -> Option<FieldTerminator> {
        trace!("Checking {:?} and {:?}", self, name);
        match name.pop_front() {
            Some(name_part) => match name_part {
                NamePart::Array => self.inner.valid_field(name),
                _ => None,
            },
            None => None,
        }
    }
}

/// A definition of key-value pairs to be parsed from form data.
#[derive(Debug, Clone)]
pub struct Map {
    inner: Vec<(String, Field)>,
}

impl Map {
    fn new() -> Self {
        Map { inner: Vec::new() }
    }

    /// Add a `Field` to a map
    /// # Example
    /// ```rust
    /// # use form_data::Field;
    /// #
    /// Field::map()
    ///     .field("sub-field", Field::text())
    ///     .field("sub-field-two", Field::text())
    ///     .finalize();
    /// ```
    pub fn field(mut self, key: &str, value: Field) -> Self {
        self.inner.push((key.to_owned(), value));

        self
    }

    /// Finalize the map into a `Field`, so it can be added to a Form
    /// ```rust
    /// # use form_data::Field;
    /// #
    /// Field::map()
    ///     .field("sub-field", Field::text())
    ///     .field("sub-field-two", Field::text())
    ///     .finalize();
    /// ```
    pub fn finalize(self) -> Field {
        Field::Map(self)
    }

    fn valid_field(&self, mut name: VecDeque<NamePart>) -> Option<FieldTerminator> {
        trace!("Checking {:?} and {:?}", self, name);
        match name.pop_front() {
            Some(name_part) => match name_part {
                NamePart::Map(part_name) => self.inner
                    .iter()
                    .find(|&&(ref item, _)| *item == part_name)
                    .and_then(|&(_, ref field)| field.valid_field(name)),
                _ => None,
            },
            None => None,
        }
    }
}

/// A structure that defines the fields expected in form data
///
/// # Example
/// ```rust
/// # extern crate mime;
/// # extern crate form_data;
/// # use std::path::{Path, PathBuf};
/// # use form_data::{Form, Field, FilenameGenerator};
/// # struct Gen;
/// # impl FilenameGenerator for Gen {
/// #     fn next_filename(&self, _: &mime::Mime) -> Option<PathBuf> {
/// #         Some(AsRef::<Path>::as_ref("path.png").to_owned())
/// #     }
/// # }
/// # fn main() {
/// # let name_generator = Gen;
/// let form = Form::new()
///     .field("field-name", Field::text())
///     .field("second-field", Field::int())
///     .field("third-field", Field::float())
///     .field("fourth-field", Field::bytes())
///     .field("fifth-field", Field::file(name_generator))
///     .field(
///         "map-field",
///         Field::map()
///             .field("sub-field", Field::text())
///             .field("sub-field-two", Field::text())
///             .finalize()
///     )
///     .field(
///         "array-field",
///         Field::array(Field::text())
///     );
/// # }
/// ```
#[derive(Clone)]
pub struct Form {
    pub max_fields: u32,
    pub max_field_size: usize,
    pub max_files: u32,
    pub max_file_size: usize,
    inner: Map,
    pub pool: ArcExecutor,
}

impl Form {
    /// Create a new form
    ///
    /// This also creates a new `CpuPool` to be used to stream files onto the filesystem. If you
    /// wish to provide your own executor, use the `from_executor` method.
    pub fn new() -> Self {
        Form::from_executor(CpuPool::new_num_cpus())
    }

    /// Set the maximum number of fields allowed in the upload
    ///
    /// The upload will error if too many fields are provided.
    pub fn max_fields(mut self, max: u32) -> Self {
        self.max_fields = max;

        self
    }

    /// Set the maximum size of a field (in bytes)
    ///
    /// The upload will error if a provided field is too large.
    pub fn max_field_size(mut self, max: usize) -> Self {
        self.max_field_size = max;

        self
    }

    /// Set the maximum number of files allowed in the upload
    ///
    /// THe upload will error if too many files are provided.
    pub fn max_files(mut self, max: u32) -> Self {
        self.max_files = max;

        self
    }

    /// Set the maximum size for files (in bytes)
    ///
    /// The upload will error if a provided file is too large.
    pub fn max_file_size(mut self, max: usize) -> Self {
        self.max_file_size = max;

        self
    }

    /// Create a new form with a given executor
    ///
    /// This executor is used to stream files onto the filesystem.
    pub fn from_executor<E>(executor: E) -> Self
    where
        E: Executor<Box<Future<Item = (), Error = ()> + Send>> + Send + Sync + Clone + 'static,
    {
        Form {
            max_fields: 100,
            max_field_size: 10_000,
            max_files: 20,
            max_file_size: 10_000_000,
            inner: Map::new(),
            pool: ArcExecutor::new(executor),
        }
    }

    pub fn field(mut self, name: &str, field: Field) -> Self {
        self.inner = self.inner.field(name, field);

        self
    }

    pub(crate) fn valid_field(&self, name: VecDeque<NamePart>) -> Option<FieldTerminator> {
        self.inner.valid_field(name.clone())
    }
}

impl fmt::Debug for Form {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Form({:?})", self.inner)
    }
}

/// The executor type stored inside a `Form`
///
/// Any executor capable of being shared and executing boxed futures can be stored here.
#[derive(Clone)]
pub struct ArcExecutor {
    inner: Arc<Executor<Box<Future<Item = (), Error = ()> + Send>> + Send + Sync + 'static>,
}

impl ArcExecutor {
    /// Create a new ArcExecutor from an Executor
    ///
    /// This essentially wraps the given executor in an Arc
    pub fn new<E>(executor: E) -> Self
    where
        E: Executor<Box<Future<Item = (), Error = ()> + Send>> + Send + Sync + Clone + 'static,
    {
        ArcExecutor {
            inner: Arc::new(executor),
        }
    }
}

impl Executor<Box<Future<Item = (), Error = ()> + Send>> for ArcExecutor where {
    fn execute(
        &self,
        future: Box<Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), ExecuteError<Box<Future<Item = (), Error = ()> + Send>>> {
        self.inner.execute(future)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContentDisposition {
    pub name: Option<String>,
    pub filename: Option<String>,
}

impl ContentDisposition {
    pub(crate) fn empty() -> Self {
        ContentDisposition {
            name: None,
            filename: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NamePart {
    Map(String),
    Array,
}

impl NamePart {
    pub fn is_map(&self) -> bool {
        match *self {
            NamePart::Map(_) => true,
            _ => false,
        }
    }
}

#[derive(Clone)]
pub(crate) enum FieldTerminator {
    File(Arc<FilenameGenerator>),
    Bytes,
    Int,
    Float,
    Text,
}

impl fmt::Debug for FieldTerminator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            FieldTerminator::File(_) => write!(f, "File(filename_generator)"),
            FieldTerminator::Bytes => write!(f, "Bytes"),
            FieldTerminator::Int => write!(f, "Int"),
            FieldTerminator::Float => write!(f, "Float"),
            FieldTerminator::Text => write!(f, "Text"),
        }
    }
}

pub(crate) type MultipartHash = (Vec<NamePart>, MultipartContent);
pub(crate) type MultipartForm = Vec<MultipartHash>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MultipartContent {
    File {
        filename: String,
        stored_as: PathBuf,
    },
    Bytes(Bytes),
    Text(String),
    Int(i64),
    Float(f64),
}
