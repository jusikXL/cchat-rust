use std::{
    collections::HashMap,
    error::Error,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::mpsc::{self, error::SendError, UnboundedSender},
    task::JoinError,
};

use futures_util::{SinkExt, StreamExt};

use tokio_tungstenite::tungstenite::{Error as TungsteniteError, Message};

#[derive(Debug)]
enum ServerError {
    UserAlreadyJoined,
    NonExistentChannel,
    NonExistentUser,
    MessagePassing(SendError<Message>),
    WebSocketError(TungsteniteError),
    JoinError(JoinError),
}

type User = UnboundedSender<Message>;
type Users = HashMap<SocketAddr, User>;
type Channels = Arc<Mutex<HashMap<String, Users>>>; // addr => tx

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

            // Spawn a separate task
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection((stream, addr), channels).await {
                    eprintln!("Error handling connection from {addr}: {e:#?}");
                }
            });
        }

        Ok(())
    }

    async fn handle_connection(
        (stream, addr): (TcpStream, SocketAddr),
        channels: Channels,
    ) -> Result<(), ServerError> {
        let ws_stream = tokio_tungstenite::accept_async(stream)
            .await
            .map_err(|e| ServerError::WebSocketError(e))?;

        let (mut outgoing, mut incoming) = ws_stream.split();

        let (tx, mut rx) = mpsc::unbounded_channel();

        let outgoing_task = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                // if we receive message from other clients
                // send them down the stream
                if let Err(e) = outgoing.send(msg).await {
                    return Err(ServerError::WebSocketError(e));
                }
            }

            Ok(())
        });

        while let Some(msg) = incoming.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if text.starts_with("/join") {
                        let channel = text[5..].trim().to_string();

                        if let Err(e) = Self::join(addr, tx.clone(), channel, &channels) {
                            tx.send(Message::text(format!("Error joining channel: {e:?}")))
                                .map_err(|e| ServerError::MessagePassing(e))?;
                        }
                    } else if text.starts_with("/leave") {
                        let channel = text[6..].trim().to_string();

                        if let Err(e) = Self::leave(&addr, &channel, &channels) {
                            tx.send(Message::text(format!("Error leaving channel: {e:?}")))
                                .map_err(|e| ServerError::MessagePassing(e))?;
                        }
                    } else if text.starts_with("/send") {
                        let parts: Vec<&str> = text[5..].trim().splitn(2, ' ').collect();

                        if let Err(e) = Self::send(&addr, parts[1], parts[0], &channels) {
                            tx.send(Message::text(format!("Error sending message: {e:?}")))
                                .map_err(|e| ServerError::MessagePassing(e))?;
                        }
                    } else {
                        tx.send(Message::text(format!("Unknown command")))
                            .map_err(|e| ServerError::MessagePassing(e))?;
                    }
                }
                Ok(_) => {
                    tx.send(Message::text(format!("Unknown command")))
                        .map_err(|e| ServerError::MessagePassing(e))?;
                }
                Err(e) => return Err(ServerError::WebSocketError(e)),
            }
        }

        outgoing_task
            .await
            .map_err(|e| ServerError::JoinError(e))??; // not sure how to handle it here
        Ok(())
    }

    fn join(
        user_addr: SocketAddr,
        user_tx: User,
        channel: String,
        channels: &Channels,
    ) -> Result<(), ServerError> {
        let mut channels = channels.lock().unwrap();
        println!("{user_addr} joins {channel}");

        match channels.get_mut(&channel) {
            // Channel has already been created
            Some(users) => {
                match users.get_mut(&user_addr) {
                    // User has already joined
                    Some(_) => {
                        // Return error
                        Err(ServerError::UserAlreadyJoined)
                    }
                    // User has not joined
                    None => {
                        // Add user
                        users.insert(user_addr, user_tx);
                        Ok(())
                    }
                }
            }
            // Channel does not exist
            None => {
                // Create channel, add user
                let mut users = HashMap::new();
                users.insert(user_addr, user_tx);

                channels.insert(channel, users);
                Ok(())
            }
        }
    }

    fn leave(
        user_addr: &SocketAddr,
        channel: &str,
        channels: &Channels,
    ) -> Result<(), ServerError> {
        let mut channels = channels.lock().unwrap();
        println!("{user_addr} leaves {channel}");

        match channels.get_mut(channel) {
            // Channel has been created
            Some(users) => {
                match users.remove(user_addr) {
                    // User has been removed
                    Some(_) => Ok(()),
                    // Not found user
                    None => Err(ServerError::NonExistentUser),
                }
            }
            // Channel does not exist
            None => {
                // Return error
                Err(ServerError::NonExistentChannel)
            }
        }
    }

    fn send(
        user_addr: &SocketAddr,
        message: &str,
        channel: &str,
        channels: &Channels,
    ) -> Result<(), ServerError> {
        let channels = channels.lock().unwrap();
        println!("{user_addr} sends {message} to {channel}");

        match channels.get(channel) {
            // Channel has been created
            Some(users) => {
                // Broadcast message to all users except this
                for (peer_addr, peer_tx) in users {
                    if peer_addr != user_addr {
                        peer_tx
                            .send(Message::text(message.to_string()))
                            .map_err(|e| ServerError::MessagePassing(e))?
                    }
                }
                Ok(())
            }
            // Channel does not exist
            None => {
                // Return error
                Err(ServerError::NonExistentChannel)
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
