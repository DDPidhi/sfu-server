use std::env;

/// Default Moonbase Alpha (Moonbeam TestNet) EVM RPC URL
/// Chain ID: 1287
/// Alternative: Paseo Asset Hub - https://testnet-passet-hub-eth-rpc.polkadot.io (chainId: 420420422)
pub const DEFAULT_ASSET_HUB_RPC_URL: &str = "https://rpc.api.moonbase.moonbeam.network";

/// Default submission timeout in seconds (longer for testnet)
pub const DEFAULT_SUBMISSION_TIMEOUT_SECS: u64 = 120;

/// Default retry count for failed transactions (higher for flaky testnet RPC)
pub const DEFAULT_RETRY_COUNT: u32 = 5;

/// Default gas limit for contract calls
/// Set high to avoid "out of gas" errors on complex string operations
pub const DEFAULT_GAS_LIMIT: u64 = 3_000_000;

/// Configuration for Asset Hub EVM interaction
#[derive(Debug, Clone)]
pub struct AssetHubConfig {
    /// Whether blockchain integration is enabled
    pub enabled: bool,
    /// RPC URL for the Asset Hub EVM endpoint
    pub rpc_url: String,
    /// Private key for the signer account (hex with 0x prefix)
    pub private_key: String,
    /// Address of the deployed proctoring contract
    pub contract_address: String,
    /// Timeout for transaction submission in seconds
    pub submission_timeout_secs: u64,
    /// Number of retries for failed transactions
    pub retry_count: u32,
    /// Gas limit for transactions
    pub gas_limit: u64,
}

impl AssetHubConfig {
    /// Creates configuration from environment variables
    ///
    /// Required environment variables when enabled:
    /// - `ASSET_HUB_ENABLED`: "true" to enable
    /// - `ASSET_HUB_PRIVATE_KEY`: Private key (hex with 0x prefix)
    /// - `ASSET_HUB_CONTRACT_ADDRESS`: Deployed contract address
    ///
    /// Optional environment variables:
    /// - `ASSET_HUB_RPC_URL`: RPC URL (default: Paseo Asset Hub / Passet Hub)
    /// - `ASSET_HUB_SUBMISSION_TIMEOUT_SECS`: Timeout in seconds (default: 120)
    /// - `ASSET_HUB_RETRY_COUNT`: Number of retries (default: 3)
    /// - `ASSET_HUB_GAS_LIMIT`: Gas limit (default: 500000)
    pub fn from_env() -> Option<Self> {
        let enabled = env::var("ASSET_HUB_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        if !enabled {
            return None;
        }

        let private_key = match env::var("ASSET_HUB_PRIVATE_KEY") {
            Ok(key) if !key.is_empty() => key,
            _ => {
                tracing::warn!("ASSET_HUB_ENABLED is true but ASSET_HUB_PRIVATE_KEY is not set");
                return None;
            }
        };

        let contract_address = match env::var("ASSET_HUB_CONTRACT_ADDRESS") {
            Ok(addr) if !addr.is_empty() => addr,
            _ => {
                tracing::warn!(
                    "ASSET_HUB_ENABLED is true but ASSET_HUB_CONTRACT_ADDRESS is not set"
                );
                return None;
            }
        };

        let rpc_url = env::var("ASSET_HUB_RPC_URL")
            .unwrap_or_else(|_| DEFAULT_ASSET_HUB_RPC_URL.to_string());

        let submission_timeout_secs = env::var("ASSET_HUB_SUBMISSION_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_SUBMISSION_TIMEOUT_SECS);

        let retry_count = env::var("ASSET_HUB_RETRY_COUNT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_RETRY_COUNT);

        let gas_limit = env::var("ASSET_HUB_GAS_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_GAS_LIMIT);

        Some(Self {
            enabled,
            rpc_url,
            private_key,
            contract_address,
            submission_timeout_secs,
            retry_count,
            gas_limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        assert_eq!(DEFAULT_ASSET_HUB_RPC_URL, "https://rpc.api.moonbase.moonbeam.network");
        assert_eq!(DEFAULT_SUBMISSION_TIMEOUT_SECS, 120);
        assert_eq!(DEFAULT_RETRY_COUNT, 5);
        assert_eq!(DEFAULT_GAS_LIMIT, 500_000);
    }

    #[test]
    fn test_from_env_disabled() {
        env::remove_var("ASSET_HUB_ENABLED");
        assert!(AssetHubConfig::from_env().is_none());
    }
}
