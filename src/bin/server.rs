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
pub enum ServerError {
    UserAlreadyJoined,
    NonExistentChannel,
    UserNotJoined,
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
                // if we receive message, send them down the stream
                outgoing
                    .send(msg)
                    .await
                    .map_err(|e| ServerError::WebSocketError(e))?
            }

            Ok(())
        });

        let send_message = |msg: &str, tx: &mpsc::UnboundedSender<Message>| {
            tx.send(Message::text(msg))
                .map_err(|e| ServerError::MessagePassing(e))
        };

        while let Some(msg) = incoming.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if text.starts_with("/join") {
                        let channel = text[5..].trim().to_string();

                        if let Err(e) = Self::join(addr, tx.clone(), channel, &channels) {
                            send_message(&format!("Error joining channel: {e:?}"), &tx)?;
                        } else {
                            send_message("Joined!", &tx)?;
                        }
                    } else if text.starts_with("/leave") {
                        let channel = text[6..].trim().to_string();

                        if let Err(e) = Self::leave(&addr, &channel, &channels) {
                            send_message(&format!("Error leaving channel: {e:?}"), &tx)?;
                        } else {
                            send_message("Left!", &tx)?;
                        }
                    } else if text.starts_with("/send") {
                        let parts: Vec<&str> = text[5..].trim().splitn(2, ' ').collect();

                        if parts.len() < 2 {
                            send_message(
                                "Invalid /send usage. Expecting <channel> <message>",
                                &tx,
                            )?;
                        }

                        if let Err(e) = Self::send(&addr, parts[1], parts[0], &channels) {
                            send_message(&format!("Error sending message: {e:?}"), &tx)?;
                        } else {
                            send_message("Sent!", &tx)?;
                        }
                    } else {
                        send_message("Unknown command", &tx)?;
                    }
                }
                Ok(_) => send_message("Unknown command", &tx)?,
                Err(e) => return Err(ServerError::WebSocketError(e)),
            }
        }

        outgoing_task
            .await
            .map_err(|e| ServerError::JoinError(e))??;

        Ok(())
    }

    fn join(
        user_addr: SocketAddr,
        user_tx: User,
        channel: String,
        channels: &Channels,
    ) -> Result<(), ServerError> {
        let mut channels = channels.lock().unwrap();

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

        match channels.get_mut(channel) {
            // Channel has been created
            Some(users) => {
                match users.remove(user_addr) {
                    // User has been removed
                    Some(_) => Ok(()),
                    // Not found user
                    None => Err(ServerError::UserNotJoined),
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

        match channels.get(channel) {
            // Channel has been created
            Some(users) => {
                match users.get(&user_addr) {
                    // If user has joined the channel
                    Some(_) => {
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
                    // If user has not joined the channel
                    None => Err(ServerError::UserNotJoined),
                }
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
