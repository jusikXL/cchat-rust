use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    sync::mpsc::{self, error::SendError},
    task::{JoinError, JoinHandle},
};

use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as TungsteniteError, Message},
};

#[derive(Debug)]
pub enum ClientError {
    MessagePassing(SendError<Message>),
    Join(JoinError),
    WebSocket(TungsteniteError),
}

pub struct Client {
    server: String,
}

impl Client {
    pub fn new(server: &str) -> Self {
        let server = format!("ws://{server}");

        Client { server }
    }

    pub async fn start(&self) -> Result<(), ClientError> {
        let (tx, mut rx) = mpsc::unbounded_channel();

        let (ws_stream, _) = connect_async(&self.server)
            .await
            .map_err(|e| ClientError::WebSocket(e))?;
        let (mut outgoing, mut incoming) = ws_stream.split();

        println!("Client: Running");

        // read stdin
        let stdin_to_tx: JoinHandle<Result<(), SendError<Message>>> = tokio::spawn(async move {
            let mut stdin = io::stdin();

            loop {
                let mut buf = vec![0; 1024];

                match stdin.read(&mut buf).await {
                    Err(_) | Ok(0) => {
                        continue; // not sure here
                    }
                    Ok(n) => buf.truncate(n),
                };

                let text = match String::from_utf8(buf) {
                    Ok(text) => text,
                    Err(_) => continue,
                };

                tx.send(Message::text(text))?
            }
        });

        // receive messages from stdin and send to sink
        let rx_to_outgoing: JoinHandle<Result<(), TungsteniteError>> = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                outgoing.send(msg).await?
            }

            Ok(())
        });

        let incoming_to_stdout: JoinHandle<Result<(), TungsteniteError>> =
            tokio::spawn(async move {
                while let Some(msg) = incoming.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            let mut stdout = io::stdout();

                            stdout.write_all(text.as_bytes()).await?;
                            stdout.write_all(b"\n").await?;
                            stdout.flush().await?;
                        }
                        Ok(_) => {
                            // ignore message of invalid format
                        }
                        Err(_e) => {
                            // ignore messages that cannot be resolved
                        }
                    }
                }

                Ok(())
            });

        // await each of the tasks, panic if necessary
        stdin_to_tx
            .await
            .map_err(|e| ClientError::Join(e))?
            .map_err(|e| ClientError::MessagePassing(e))?;

        rx_to_outgoing
            .await
            .map_err(|e| ClientError::Join(e))?
            .map_err(|e| ClientError::WebSocket(e))?;

        incoming_to_stdout
            .await
            .map_err(|e| ClientError::Join(e))?
            .map_err(|e| ClientError::WebSocket(e))?;

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let client = Client::new("127.0.0.1:8080");

    if let Err(e) = client.start().await {
        eprint!("Client error: {e:#?}");
    }
}
