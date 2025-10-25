use std::env;
use std::net::{IpAddr, Ipv4Addr};

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
                host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
                port: env::var("SERVER_PORT")
                    .unwrap_or_else(|_| "8080".to_string())
                    .parse()
                    .expect("Invalid SERVER_PORT"),
            },
        }
    }

    pub fn bind_address(&self) -> ([u8; 4], u16) {
        let ip_addr = self.parse_host_to_ipv4();
        (ip_addr.octets(), self.server.port)
    }

    fn parse_host_to_ipv4(&self) -> Ipv4Addr {
        // Try to parse as IP address first
        if let Ok(addr) = self.server.host.parse::<IpAddr>() {
            match addr {
                IpAddr::V4(ipv4) => return ipv4,
                IpAddr::V6(_) => {
                    tracing::warn!(
                        host = %self.server.host,
                        "IPv6 address provided but only IPv4 supported, using 0.0.0.0"
                    );
                    return Ipv4Addr::new(0, 0, 0, 0);
                }
            }
        }

        // Handle common hostnames
        match self.server.host.as_str() {
            "localhost" => Ipv4Addr::new(127, 0, 0, 1),
            "" | "0.0.0.0" => Ipv4Addr::new(0, 0, 0, 0),
            _ => {
                tracing::warn!(
                    host = %self.server.host,
                    "Unable to parse host as IPv4, using 0.0.0.0"
                );
                Ipv4Addr::new(0, 0, 0, 0)
            }
        }
    }
}