# Actix Form Data
A library for retrieving form data from Actix Web's multipart streams. It can stream uploaded files
onto the filesystem (its main purpose), but it can also parse associated form data.

[documentation](https://docs.rs/actix-form-data)

### Usage

Add it to your dependencies.
```toml
# Cargo.toml

[dependencies]
actix-web = "0.6.0"
actix-form-data = "0.2.2"
```

Require it in your project.
```rust
// src/lib.rs or src/main.rs

extern crate form_data;

use form_data::{Field, Form, Value};
```

#### Overview
First, you'd create a form structure you want to parse from the multipart stream.
```rust
let form = Form::new().field("field-name", Field::text());
```
This creates a form with one required field named "field-name" that will be parsed as text.

Then, pass it to `handle_multipart` in your request handler.
```rust
let future = form_data::handle_multipart(req.multipart, form);
```

This returns a `Future<Item = Value, Error = form_data::Error>`, which can be used to
fetch your data.

```rust
let field_value = match value {
  Value::Map(mut hashmap) => {
    hashmap.remove("field-name")?
  }
  _ => return None,
};
```

#### Example
```rust
/// examples/simple.rs

extern crate actix_web;
extern crate form_data;
extern crate futures;
extern crate mime;

use std::path::PathBuf;

use actix_web::{http, server, App, AsyncResponder, HttpMessage, HttpRequest, HttpResponse, State};
use form_data::{handle_multipart, Error, Field, FilenameGenerator, Form};
use futures::Future;

struct Gen;

impl FilenameGenerator for Gen {
    fn next_filename(&self, _: &mime::Mime) -> Option<PathBuf> {
        let mut p = PathBuf::new();
        p.push("examples/filename.png");
        Some(p)
    }
}

fn upload(
    req: HttpRequest<Form>,
    state: State<Form>,
) -> Box<Future<Item = HttpResponse, Error = Error>> {
    handle_multipart(req.multipart(), state.clone())
        .map(|uploaded_content| {
            println!("Uploaded Content: {:?}", uploaded_content);
            HttpResponse::Created().finish()
        })
        .responder()
}

fn main() {
    let form = Form::new()
        .field("Hey", Field::text())
        .field(
            "Hi",
            Field::map()
                .field("One", Field::int())
                .field("Two", Field::float())
                .finalize(),
        )
        .field("files", Field::array(Field::file(Gen)));

    println!("{:?}", form);

    server::new(move || {
        App::with_state(form.clone())
            .resource("/upload", |r| r.method(http::Method::POST).with2(upload))
    }).bind("127.0.0.1:8080")
        .unwrap()
        .run();
}
```

### Contributing
Feel free to open issues for anything you find an issue with. Please note that any contributed code will be licensed under the GPLv3.

### License

Copyright © 2018 Riley Trautman

Actix Form Data is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

Actix Form Data is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details. This file is part of Actix Form Data.

You should have received a copy of the GNU General Public License along with Actix Form Data. If not, see [http://www.gnu.org/licenses/](http://www.gnu.org/licenses/).
