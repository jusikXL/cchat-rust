use futures_util::StreamExt;
use std::{
    collections::HashSet,
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

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
            let channels = Arc::clone(&self.channels);

            tokio::spawn(async move {
                // process stream

                Self::process_stream(stream, channels).await;
            });
        }

        Ok(())
    }

    async fn process_stream(stream: TcpStream, _channels: Channels) {
        let addr = stream
            .peer_addr()
            .expect("connected streams should have a peer address");
        println!("Peer address: {}", addr);

        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .expect("Error during the websocket handshake occurred");

        println!("New WebSocket connection: {}", addr);

        let (write, read) = ws_stream.split();
        // We should not forward messages other than text or binary.
        read.forward(write)
            .await
            .expect("Failed to forward messages")
    }
}

#[tokio::main]
async fn main() {
    let server = Server::new("127.0.0.1:8080");

    // start server on current thread
    // no specific new thread is created for that
    if let Err(e) = server.start().await {
        eprintln!("Server Error: {e}");
    }
}
