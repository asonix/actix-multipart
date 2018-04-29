use std::{fmt, collections::VecDeque, path::PathBuf, sync::Arc};

use futures::{Future, future::{ExecuteError, Executor}};
use futures_cpupool::CpuPool;

use super::FilenameGenerator;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "with-serde", derive(Deserialize, Serialize))]
pub enum NamePart {
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
pub enum FieldTerminator {
    File(Arc<FilenameGenerator>),
    Int,
    Float,
    Text,
}

impl fmt::Debug for FieldTerminator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            FieldTerminator::File(_) => write!(f, "File(filename_generator)"),
            FieldTerminator::Int => write!(f, "Int"),
            FieldTerminator::Float => write!(f, "Float"),
            FieldTerminator::Text => write!(f, "Text"),
        }
    }
}

#[derive(Clone)]
pub enum Field {
    Array(Array),
    File(Arc<FilenameGenerator>),
    Map(Map),
    Int,
    Float,
    Text,
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
        }
    }
}

impl Field {
    pub fn file<T>(gen: T) -> Self
    where
        T: FilenameGenerator + 'static,
    {
        Field::File(Arc::new(gen))
    }

    pub fn text() -> Self {
        Field::Text
    }

    pub fn int() -> Self {
        Field::Int
    }

    pub fn float() -> Self {
        Field::Float
    }

    pub fn array(field: Field) -> Self {
        Field::Array(Array::new(field))
    }

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
        }
    }
}

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

#[derive(Debug, Clone)]
pub struct Map {
    inner: Vec<(String, Field)>,
}

impl Map {
    fn new() -> Self {
        Map { inner: Vec::new() }
    }

    pub fn field(mut self, key: &str, value: Field) -> Self {
        self.inner.push((key.to_owned(), value));

        self
    }

    pub fn finalize(self) -> Field {
        Field::Map(self)
    }

    fn valid_field(&self, mut name: VecDeque<NamePart>) -> Option<FieldTerminator> {
        trace!("Checking {:?} and {:?}", self, name);
        match name.pop_front() {
            Some(name_part) => match name_part {
                NamePart::Map(part_name) => self.inner
                    .iter()
                    .find(|(item, _)| *item == part_name)
                    .and_then(|(_, field)| field.valid_field(name)),
                _ => None,
            },
            None => None,
        }
    }
}

#[derive(Clone)]
pub struct Form {
    pub max_fields: u32,
    pub max_field_size: u32,
    pub max_files: u32,
    pub max_file_size: u32,
    inner: Map,
    pub pool: ArcExecutor,
}

impl Form {
    pub fn new() -> Self {
        Form::from_executor(CpuPool::new_num_cpus())
    }

    pub fn max_fields(mut self, max: u32) -> Self {
        self.max_fields = max;

        self
    }

    pub fn max_field_size(mut self, max: u32) -> Self {
        self.max_field_size = max;

        self
    }

    pub fn max_files(mut self, max: u32) -> Self {
        self.max_files = max;

        self
    }

    pub fn max_file_size(mut self, max: u32) -> Self {
        self.max_file_size = max;

        self
    }

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

    pub fn valid_field(&self, name: VecDeque<NamePart>) -> Option<FieldTerminator> {
        self.inner.valid_field(name.clone())
    }
}

impl fmt::Debug for Form {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Form({:?})", self.inner)
    }
}

#[derive(Clone)]
pub struct ArcExecutor {
    inner: Arc<Executor<Box<Future<Item = (), Error = ()> + Send>> + Send + Sync + 'static>,
}

impl ArcExecutor {
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

pub type MultipartHash = (Vec<NamePart>, MultipartContent);

pub type MultipartForm = Vec<MultipartHash>;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "with-serde", derive(Deserialize, Serialize))]
pub enum MultipartContent {
    File {
        filename: String,
        stored_as: PathBuf,
    },
    Text(String),
    Int(i64),
    Float(f64),
}
