extern crate dotenv;

use actix_web::{
    get,
    middleware::Logger,
    web::{self},
    App, HttpRequest, HttpResponse, HttpServer,
};
use cloud_storage::client::Client;
// use dotenv::dotenv;

use reqwest::{
    header::{HeaderMap, HeaderValue, RANGE},
    StatusCode,
};

trait TryCollect<T> {
    fn collect_tuple(&mut self) -> Option<T>;
}

macro_rules! imp_try_collect_tuple {
    () => {};
    ($A:ident $($I: ident)*) => {
        imp_try_collect_tuple!($($I)*);

        impl<$A: Iterator> TryCollect<($A::Item, $($I::Item),*)> for $A {
            fn collect_tuple(&mut self) -> Option<($A::Item, $($I::Item),*)>{
                let r = (try_opt!(self.next()),
                //hack: we need to use $I in the expansion
                        $({ let a: $I::Item = try_opt!(self.next()); a}), *);

                Some(r)
            }
        }
    };
}

macro_rules! try_opt {
    ($e: expr) => {
        match $e {
            Some(e) => e,
            None => return None,
        }
    };
}

imp_try_collect_tuple!(A A A A A A A A A A A A);
#[get("/uploads/{location}")]
async fn greet(location: web::Path<String>, request: HttpRequest) -> HttpResponse {
    let headers = request.headers();

    // obtain storag object size
    let client = Client::default();

    let object = client
        .object()
        .read(
            &std::env::var("GCS_BUCKET").unwrap_or_default(),
            location.as_str(),
        )
        .await
        .unwrap();

    let size: i32 = object.size as i32;
    let range = headers.get("range").unwrap();

    if headers.get("range").is_some() {
        //
        let range = String::from(range.to_str().unwrap());

        let (start, end) = range
            .replace("bytes=", "")
            .split('-')
            .map(|s| s.parse::<i32>())
            .collect_tuple()
            .expect("collect as tuple failed");

        let mut start = if start.is_ok() {
            Some(start.unwrap())
        } else {
            None
        };

        let mut end = if end.is_ok() {
            Some(end.unwrap())
        } else {
            None
        };

        if start.is_some() && end.is_none() {
            start = start;
            end = Some(size - 1);
        }

        if start.is_none() && end.is_some() {
            start = Some(size - end.unwrap());
            end = Some(size - 1);
        }

        // Handle unavailable range request
        if start.unwrap() >= size || end.unwrap() >= size {
            // Return the 416 Range Not Satisfiable.
            return HttpResponse::build(StatusCode::NOT_ACCEPTABLE)
                .insert_header(("content-range", "bytes */${size}"))
                .finish();
        }

        let range = format!(
            "bytes={start}-{end}",
            start = start.unwrap(),
            end = end.unwrap()
        );

        let mut headers = HeaderMap::new();
        headers.insert(RANGE, HeaderValue::from_str(range.as_str()).unwrap());

        // get a client builder
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        let client = Client::builder().client(client).build().unwrap();

        let data = client
            .object()
            .download(
                &std::env::var("GCS_BUCKET").unwrap_or_default(),
                location.as_str(),
            )
            .await
            .unwrap();

        let range = format!(
            "bytes={start}-{end}/{size}",
            start = start.unwrap(),
            end = end.unwrap()
        );

        return HttpResponse::build(StatusCode::PARTIAL_CONTENT)
            .insert_header(("content-range", range))
            .insert_header(("accept-ranges", "bytes"))
            .insert_header(("content-length", (end.unwrap() - start.unwrap() + 1)))
            .insert_header(("content-type", "video/mp4"))
            .body(data);
    }

    return HttpResponse::build(StatusCode::NOT_ACCEPTABLE)
        .insert_header(("content-range", "bytes */${size}"))
        .finish();
}

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    // dotenv().ok();
    println!("\nListening on port 3443...");
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let port: u16 = std::env::var("PORT").unwrap_or_default().parse().unwrap();

    println!("{port}");

    HttpServer::new(|| {
        App::new()
            .service(greet)
            .wrap(Logger::new("%r").log_target("=>"))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
