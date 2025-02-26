use std::{ error::Error, io::Write, net::TcpStream };

pub struct Client {
    server: String,
}

impl Client {
    pub fn new(server: String) -> Result<Self, Box<dyn Error>> {
        let mut stream = TcpStream::connect(&server)?;

        let request = "/join1";

        stream.write_all(request.as_bytes())?;

        Ok(Client {
            server,
        })
    }
}
