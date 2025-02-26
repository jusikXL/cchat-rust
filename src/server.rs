use std::{
    collections::HashSet,
    error::Error,
    io::{ prelude::*, BufReader },
    net::{ TcpListener, TcpStream },
};

pub struct Server {
    addr: String,
    channels: HashSet<String>,
}

impl Server {
    pub fn start(addr: String) -> Result<Self, Box<dyn Error>> {
        let listener = TcpListener::bind(&addr)?;

        for stream in listener.incoming() {
            let stream = stream.unwrap();

            Self::handle_connection(&stream);
        }

        Ok(Server {
            addr,
            channels: HashSet::new(),
        })
    }

    fn handle_connection(stream: &TcpStream) {
        let buf_reader = BufReader::new(stream);

        let request: String = buf_reader
            .lines()
            .map(|result| result.unwrap())
            .take_while(|line| !line.is_empty())
            .collect();

        println!("Server got: {request:?}");

        match request.as_str() {
            "/join" => {
                // handle join
                self::join()
            }
            _ => {
                // skip invalid request
            }
        }
    }

    fn join(&mut self, channel_addr: String) {
        // process join request

        if self.channels.contains(&channel_addr) {
            // try to join the channel
        } else {
            self.channels.insert(channel_addr);
            // create and join channel
        }

        format!("Tried joining channel");
    }
}
