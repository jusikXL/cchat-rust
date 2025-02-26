use std::{ env, process };

mod config;
mod server;
mod client;

use config::{ Config, Operator };
use client::Client;
use server::Server;

fn main() {
    const SERVER: &str = "127.0.0.1:8080";
    const CLIENT: &str = "127.0.0.1:3000";

    let config = Config::build(env::args()).unwrap_or_else(|e| {
        eprintln!("Problem parsing arguments: {e} \nUsage: cargo run -- [server|client]");
        process::exit(1);
    });

    println!("Starting as: {:?}", config.operator);

    match config.operator {
        Operator::Server => {
            if let Err(e) = Server::start(SERVER.to_string()) {
                eprintln!("Server error: {e}");
                process::exit(1);
            }
        }
        Operator::Client => {
            if let Err(e) = Client::new(SERVER.to_string()) {
                eprintln!("Client error: {e}");
                process::exit(1);
            }
        }
    }
}
