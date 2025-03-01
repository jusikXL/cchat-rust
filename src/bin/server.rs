use std::{
    collections::HashMap,
    error::Error,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::mpsc::{self, UnboundedSender},
};

use futures_util::{SinkExt, StreamExt};

use tokio_tungstenite::tungstenite::Message;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>; // addr => tx

pub struct Server<A: ToSocketAddrs> {
    addr: A,
    peer_map: PeerMap, // will block the real thread for some time
}

impl<A> Server<A>
where
    A: ToSocketAddrs,
{
    pub fn new(addr: A) -> Self {
        Self {
            addr,
            peer_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(&self.addr).await?;
        println!("Server: Listening");

        while let Ok((stream, addr)) = listener.accept().await {
            let peer_map = Arc::clone(&self.peer_map);

            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await; // simulate long processing

                Self::handle_connection((stream, addr), peer_map).await;
            });
        }

        Ok(())
    }

    async fn handle_connection(conn: (TcpStream, SocketAddr), peer_map: PeerMap) {
        let (stream, addr) = conn;

        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket handshake");

        let (tx, mut rx) = mpsc::unbounded_channel();
        {
            peer_map.lock().unwrap().insert(addr, tx);
        }

        // let (write, read)
        let (mut outgoing, mut incoming) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                // if we receive message from other clients
                // send them down the stream
                outgoing
                    .send(msg)
                    .await
                    .expect("sending message from other clients");
            }
        });

        while let Some(msg) = incoming.next().await {
            // if we receive message from this client
            // send them to other clients

            let msg = msg.expect("receiving message from this client");

            let peer_map = peer_map.lock().unwrap();
            // must be locked for entier broadcast process

            // We want to broadcast the message to everyone except ourselves.
            let broadcast_recipients = peer_map
                .iter()
                .filter(|(peer_addr, _)| peer_addr != &&addr)
                .map(|(_, tx)| tx);

            for tx in broadcast_recipients {
                // send from each tx to a single rx
                tx.send(msg.clone())
                    .expect("sending message to other clients");
            }
        }
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
