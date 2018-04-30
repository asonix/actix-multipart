extern crate actix;
extern crate actix_web;
extern crate env_logger;
#[macro_use]
extern crate failure;
extern crate form_data;
extern crate futures;
#[macro_use]
extern crate log;
extern crate mime;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use std::{env, path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};

use actix_web::{http, server, App, AsyncResponder, HttpMessage, HttpRequest, HttpResponse, State,
                error::ResponseError, middleware::Logger};
use form_data::*;
use futures::Future;

struct Gen(AtomicUsize);

impl Gen {
    pub fn new() -> Self {
        Gen(AtomicUsize::new(0))
    }
}

impl FilenameGenerator for Gen {
    fn next_filename(&self, _: &mime::Mime) -> Option<PathBuf> {
        let mut p = PathBuf::new();
        p.push("examples");
        p.push(&format!(
            "filename{}.png",
            self.0.fetch_add(1, Ordering::Relaxed)
        ));
        Some(p)
    }
}

#[derive(Clone, Debug)]
struct AppState {
    form: Form,
}

#[derive(Clone, Debug, Deserialize, Fail, Serialize)]
#[fail(display = "{}", msg)]
struct JsonError {
    msg: String,
}

impl From<Error> for JsonError {
    fn from(e: Error) -> Self {
        JsonError {
            msg: format!("{}", e),
        }
    }
}

impl ResponseError for JsonError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::BadRequest().json(Errors::from(self.clone()))
    }
}

#[derive(Clone, Debug, Deserialize, Fail, Serialize)]
#[fail(display = "Errors occurred")]
struct Errors {
    errors: Vec<JsonError>,
}

impl From<JsonError> for Errors {
    fn from(e: JsonError) -> Self {
        Errors { errors: vec![e] }
    }
}

impl ResponseError for Errors {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::BadRequest().json(self)
    }
}

fn upload(
    req: HttpRequest<AppState>,
    state: State<AppState>,
) -> Box<Future<Item = HttpResponse, Error = Errors>> {
    handle_multipart(req.multipart(), state.form.clone())
        .map(|uploaded_content| {
            info!("Uploaded Content: {:?}", uploaded_content);
            HttpResponse::Created().finish()
        })
        .map_err(JsonError::from)
        .map_err(Errors::from)
        .responder()
}

fn main() {
    env::set_var("RUST_LOG", "upload=info");
    env_logger::init();

    let sys = actix::System::new("upload-test");

    let form = Form::new()
        .field("Hey", Field::text())
        .field(
            "Hi",
            Field::map()
                .field("One", Field::int())
                .field("Two", Field::float())
                .finalize(),
        )
        .field("files", Field::array(Field::file(Gen::new())));

    info!("{:?}", form);

    let state = AppState { form };

    server::new(move || {
        App::with_state(state.clone())
            .middleware(Logger::default())
            .resource("/upload", |r| r.method(http::Method::POST).with2(upload))
    }).bind("127.0.0.1:8080")
        .unwrap()
        .start();

    sys.run();
}
