use std::env;
use std::net::{IpAddr, Ipv4Addr};

pub struct Config {
    pub server: ServerConfig,
    pub recording: RecordingConfig,
}

pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

pub struct RecordingConfig {
    pub enabled: bool,
    pub output_dir: String,
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
            recording: RecordingConfig {
                enabled: env::var("RECORDING_ENABLED")
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()
                    .unwrap_or(true),
                output_dir: env::var("RECORDING_OUTPUT_DIR")
                    .unwrap_or_else(|_| "./recordings".to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn default_recording_config() -> RecordingConfig {
        RecordingConfig {
            enabled: true,
            output_dir: "./recordings".to_string(),
        }
    }

    #[test]
    fn test_parse_localhost() {
        let config = Config {
            server: ServerConfig {
                host: "localhost".to_string(),
                port: 8080,
            },
            recording: default_recording_config(),
        };

        let addr = config.bind_address();
        assert_eq!(addr, ([127, 0, 0, 1], 8080));
    }

    #[test]
    fn test_parse_ipv4_address() {
        let config = Config {
            server: ServerConfig {
                host: "192.168.1.1".to_string(),
                port: 3000,
            },
            recording: default_recording_config(),
        };

        let addr = config.bind_address();
        assert_eq!(addr, ([192, 168, 1, 1], 3000));
    }

    #[test]
    fn test_parse_all_interfaces() {
        let config = Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
            },
            recording: default_recording_config(),
        };

        let addr = config.bind_address();
        assert_eq!(addr, ([0, 0, 0, 0], 8080));
    }

    #[test]
    fn test_parse_empty_host() {
        let config = Config {
            server: ServerConfig {
                host: "".to_string(),
                port: 8080,
            },
            recording: default_recording_config(),
        };

        let addr = config.bind_address();
        assert_eq!(addr, ([0, 0, 0, 0], 8080));
    }

    #[test]
    fn test_parse_invalid_hostname_defaults_to_all() {
        let config = Config {
            server: ServerConfig {
                host: "invalid-hostname".to_string(),
                port: 9000,
            },
            recording: default_recording_config(),
        };

        let addr = config.bind_address();
        assert_eq!(addr, ([0, 0, 0, 0], 9000));
    }
}