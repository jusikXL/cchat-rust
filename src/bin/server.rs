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

type Client = UnboundedSender<Message>;
type Clients = HashMap<SocketAddr, Client>;
type Channels = Arc<Mutex<HashMap<String, Clients>>>; // addr => tx

pub struct Server<A: ToSocketAddrs> {
    addr: A,
    channels: Channels, // will block the real thread for some time
}

impl<A> Server<A>
where
    A: ToSocketAddrs,
{
    pub fn new(addr: A) -> Self {
        Self {
            addr,
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(&self.addr).await?;
        println!("Server: Listening");

        while let Ok((stream, addr)) = listener.accept().await {
            let channels = Arc::clone(&self.channels);

            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await; // simulate long processing

                Self::handle_connection((stream, addr), channels).await;
            });
        }

        Ok(())
    }

    async fn handle_connection(conn: (TcpStream, SocketAddr), channels: Channels) {
        let (stream, addr) = conn;

        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket handshake");
        // let (write, read)
        let (mut outgoing, mut incoming) = ws_stream.split();

        let (tx, mut rx) = mpsc::unbounded_channel();

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

            match msg {
                Ok(Message::Text(text)) => {
                    if text.starts_with("/join") {
                        let channel = text[5..].trim().to_string();

                        println!("joining {channel}");

                        let mut channels = channels.lock().unwrap();

                        let clients = channels.entry(channel).or_default();
                        clients.entry(addr).or_insert(tx.clone());
                    } else if text.starts_with("/leave") {
                        let channel = text[6..].trim().to_string();

                        println!("leaving {channel}");

                        let mut channels = channels.lock().unwrap();

                        if let Some(clients) = channels.get_mut(&channel) {
                            match clients.remove(&addr) {
                                None => eprintln!("Client has not joined"),
                                Some(_) => {}
                            }
                        } else {
                            eprintln!("No such channel exists");
                        }
                    } else if text.starts_with("/send") {
                        println!("sending");

                        let parts: Vec<&str> = text[5..].trim().splitn(2, ' ').collect();

                        println!("{:?}", parts);

                        if parts.len() < 2 {
                            eprintln!("Invalid send format")
                        }

                        let (channel, message) = (parts[0], parts[1]);

                        let channels = channels.lock().unwrap();

                        if let Some(clients) = channels.get(channel) {
                            for (peer_addr, peer_tx) in clients {
                                if peer_addr != &addr {
                                    peer_tx
                                        .send(Message::text(message.to_string()))
                                        .expect("sending message to other clients");
                                }
                            }
                        } else {
                            eprintln!("No such channel exists");
                        }
                    }
                }
                Ok(_) => {
                    eprintln!("Non-text message");
                }
                Err(e) => eprintln!("Error receiving message: {}", e),
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
