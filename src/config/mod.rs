use std::env;

pub struct Config {
    pub server: ServerConfig,
}

pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        dotenv::dotenv().ok();

        Self {
            server: ServerConfig {
                host: env::var("SERVER_HOST").unwrap_or_else(|_| "localhost".to_string()),
                port: env::var("SERVER_PORT")
                    .unwrap_or_else(|_| "8080".to_string())
                    .parse()
                    .expect("Invalid SERVER_PORT"),
            },
        }
    }

    pub fn bind_address(&self) -> ([u8; 4], u16) {
        ([0, 0, 0, 0], self.server.port)
    }
}