use std::{
    collections::HashSet,
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::net::{TcpListener, ToSocketAddrs};

use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper_util::rt::TokioIo;

use hyper::{server::conn::http1, service::service_fn, Method, Request, Response, StatusCode};

use bytes::Bytes;

type Channels = Arc<Mutex<HashSet<String>>>;

pub struct Server<A: ToSocketAddrs> {
    addr: A,
    channels: Channels,
    // will block the thread server and possible other tasks are on for some time
}

impl<A> Server<A>
where
    A: ToSocketAddrs,
{
    pub fn new(addr: A) -> Self {
        Self {
            addr,
            channels: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(&self.addr).await?;
        println!("Server: Listening");

        while let Ok((stream, _)) = listener.accept().await {
            let io = TokioIo::new(stream);
            let channels = Arc::clone(&self.channels);

            tokio::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .serve_connection(
                        io,
                        service_fn(move |req| Self::handle_request(req, Arc::clone(&channels))),
                    )
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            });
        }

        Ok(())
    }

    async fn handle_request(
        req: Request<hyper::body::Incoming>,
        channels: Channels,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
        match (req.method(), req.uri().path()) {
            (&Method::GET, "/") => Ok(Response::new(full("hello"))),
            (&Method::POST, "/join") => {
                // client wants to join
                let channel = String::from_utf8(req.collect().await?.to_bytes().to_vec()).unwrap();

                tokio::time::sleep(Duration::from_secs(5)).await; // simulate long request processing

                let mut channels = channels.lock().unwrap(); // lock channels
                let is_new = channels.insert(channel.clone()); // TODO: remove clone later

                Ok(Response::new(full(format!(
                    "/join, {} \n created channel {}",
                    &channel, is_new
                ))))
            }
            _ => {
                let mut not_found = Response::new(empty());
                *not_found.status_mut() = StatusCode::NOT_FOUND;
                Ok(not_found)
            }
        }
    }
}

fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}
fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

#[tokio::main]
async fn main() {
    let server = Server::new("127.0.0.1:4000");

    // start server on current thread
    // no specific new thread is created for that
    if let Err(e) = server.start().await {
        eprintln!("Server Error: {e}");
    }
}
