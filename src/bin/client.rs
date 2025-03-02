use std::error::Error;

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
