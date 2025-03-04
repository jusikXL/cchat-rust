use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};

use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as TungsteniteError, Message},
};

#[derive(Debug)]
pub enum ClientError {
    WebSocketError(TungsteniteError),
}

pub struct Client {
    server: String,
}

impl Client {
    pub fn new(server: &str) -> Self {
        let server = format!("ws://{server}");

        Client { server }
    }

    pub async fn start(&self) -> Result<(), TungsteniteError> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (ws_stream, _) = connect_async(&self.server).await?;
        let (mut outgoing, mut incoming) = ws_stream.split();

        // read stdin
        tokio::spawn(async move {
            let mut stdin = io::stdin();

            loop {
                let mut buf = vec![0; 1024];

                let n = match stdin.read(&mut buf).await {
                    Err(_) | Ok(0) => break,
                    Ok(n) => n,
                };

                buf.truncate(n);
                let text = String::from_utf8(buf).unwrap();
                tx.send(Message::text(text)).unwrap();
            }
        });

        // receive messages from stdin and send to sink
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                outgoing.send(msg).await.unwrap()
            }
        });

        while let Some(msg) = incoming.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let mut stdout = io::stdout();

                    stdout.write_all(text.as_bytes()).await.unwrap();
                    stdout.write_all(b"\n").await.unwrap();
                    stdout.flush().await.unwrap();
                }
                Ok(_) => {}
                Err(_e) => {}
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let client = Client::new("127.0.0.1:8080");

    if let Err(e) = client.start().await {
        eprint!("Client error: {e}");
    }
}
