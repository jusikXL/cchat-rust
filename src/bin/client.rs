use std::error::Error;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::Request;
use hyper_util::rt::TokioIo;

use tokio::net::{TcpStream, ToSocketAddrs};

pub struct Client<A: ToSocketAddrs> {
    server: A,
}

impl<A> Client<A>
where
    A: ToSocketAddrs,
{
    pub fn new(server: A) -> Self {
        Client { server }
    }

    pub async fn join(&self, channel: String) -> Result<(), Box<dyn Error>> {
        let stream = TcpStream::connect(&self.server).await?;

        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                eprintln!("Connection failed: {:?}", err);
            }
        });

        let req = Request::builder()
            .uri("/") // default one so can be removed
            .body(Empty::<Bytes>::new())?;

        let res = sender.send_request(req).await?;

        let hello = String::from_utf8(res.collect().await?.to_bytes().to_vec()).unwrap();

        println!("Response: {}", hello);

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let client = Client::new("127.0.0.1:4000");

    if let Err(e) = client.join("channel 1".to_string()).await {
        eprint!("Client error: {e}");
    }
}
