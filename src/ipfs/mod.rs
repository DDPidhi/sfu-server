use std::path::Path;
use std::time::Duration;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::error::{Result, SfuError};

const DEFAULT_IPFS_API_URL: &str = "http://127.0.0.1:5001";
const DEFAULT_IPFS_GATEWAY_URL: &str = "http://127.0.0.1:8080/ipfs";

#[derive(Debug, Clone)]
pub struct IpfsConfig {
    pub enabled: bool,
    pub api_url: String,
    pub gateway_url: String,
    pub upload_timeout_secs: u64,
}

impl IpfsConfig {
    pub fn from_env() -> Option<Self> {
        let enabled = std::env::var("IPFS_ENABLED")
            .unwrap_or_else(|_| "false".to_string())
            .parse()
            .unwrap_or(false);

        if !enabled {
            return None;
        }

        let api_url = std::env::var("IPFS_API_URL")
            .unwrap_or_else(|_| DEFAULT_IPFS_API_URL.to_string());
        let gateway_url = std::env::var("IPFS_GATEWAY_URL")
            .unwrap_or_else(|_| DEFAULT_IPFS_GATEWAY_URL.to_string());
        let upload_timeout_secs = std::env::var("IPFS_UPLOAD_TIMEOUT_SECS")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .unwrap_or(300);

        Some(Self {
            enabled,
            api_url,
            gateway_url,
            upload_timeout_secs,
        })
    }
}

/// Response from IPFS add API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsAddResponse {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Hash")]
    pub hash: String,
    #[serde(rename = "Size")]
    pub size: String,
}

/// Result of uploading a file to IPFS
#[derive(Debug, Clone)]
pub struct IpfsUploadResult {
    pub cid: String,
    pub gateway_url: String,
    pub size: u64,
}

pub struct IpfsClient {
    config: IpfsConfig,
    client: reqwest::Client,
}

impl IpfsClient {
    pub fn new(config: IpfsConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.upload_timeout_secs))
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, client })
    }

    /// Upload a file to IPFS and return the CID
    pub async fn upload_file(
        &self,
        file_path: &Path,
        room_id: &str,
        peer_id: &str,
    ) -> Result<IpfsUploadResult> {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("recording.webm")
            .to_string();

        // Read file contents
        let mut file = File::open(file_path).await.map_err(|e| {
            SfuError::Internal(format!("Failed to open file for upload: {}", e))
        })?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await.map_err(|e| {
            SfuError::Internal(format!("Failed to read file for upload: {}", e))
        })?;

        // Create multipart form
        let file_part = Part::bytes(buffer)
            .file_name(file_name.clone());

        let form = Form::new()
            .part("file", file_part);

        // IPFS API endpoint for adding files
        let add_url = format!("{}/api/v0/add", self.config.api_url);

        // Send request
        let response = self.client
            .post(&add_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                SfuError::IpfsUploadFailed(format!("Request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(SfuError::IpfsUploadFailed(format!(
                "Upload failed with status {}: {}",
                status, error_text
            )));
        }

        let ipfs_response: IpfsAddResponse = response.json().await.map_err(|e| {
            SfuError::IpfsUploadFailed(format!("Failed to parse response: {}", e))
        })?;

        let gateway_url = format!("{}/{}", self.config.gateway_url, ipfs_response.hash);
        let size: u64 = ipfs_response.size.parse().unwrap_or(0);

        tracing::info!(
            cid = %ipfs_response.hash,
            size = size,
            room_id = %room_id,
            peer_id = %peer_id,
            file_name = %file_name,
            "Successfully uploaded recording to IPFS"
        );

        Ok(IpfsUploadResult {
            cid: ipfs_response.hash,
            gateway_url,
            size,
        })
    }

    /// Check if IPFS node is reachable
    pub async fn health_check(&self) -> Result<bool> {
        let version_url = format!("{}/api/v0/version", self.config.api_url);

        match self.client.post(&version_url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    pub fn gateway_url(&self) -> &str {
        &self.config.gateway_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipfs_config_disabled_by_default() {
        std::env::remove_var("IPFS_ENABLED");
        std::env::remove_var("IPFS_API_URL");

        let config = IpfsConfig::from_env();
        assert!(config.is_none());
    }

    #[test]
    fn test_ipfs_add_response_deserialize() {
        let json = r#"{
            "Name": "test.webm",
            "Hash": "QmTest123456789",
            "Size": "12345"
        }"#;

        let response: IpfsAddResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.hash, "QmTest123456789");
        assert_eq!(response.name, "test.webm");
        assert_eq!(response.size, "12345");
    }
}
