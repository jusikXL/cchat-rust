#[derive(Debug)]
pub enum Operator {
    Client,
    Server,
}

pub struct Config {
    pub operator: Operator,
}

impl Config {
    pub fn build(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        args.next(); // Skip program name

        let operator = match args.next() {
            Some(op) if op == "server" => Operator::Server,
            Some(op) if op == "client" => Operator::Client,
            Some(op) => {
                return Err(format!(
                    "Invalid operator: '{}'. Use 'server' or 'client'.",
                    op
                ))
            }
            None => return Err("Missing operator argument. Use 'server' or 'client'.".to_string()),
        };

        Ok(Self { operator })
    }
}
