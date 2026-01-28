//! Asset Hub EVM blockchain integration module
//!
//! This module provides functionality for recording proctoring events
//! on Asset Hub (Polkadot) using a Solidity smart contract.
//!
//! # Architecture
//!
//! The module consists of three main components:
//!
//! - `config`: Configuration management for blockchain connection
//! - `client`: Contract client for EVM interaction via ethers
//! - `queue`: Non-blocking event queue for async submission
//!
//! # Wallet-Based Identity
//!
//! All participants (students and proctors) are identified by their wallet addresses.
//! This enables future NFT generation based on exam results.
//!
//! # Usage
//!
//! ```rust,ignore
//! use substrate::{AssetHubConfig, ContractClient, EventQueue, ChainEvent, Address};
//!
//! // Initialize from environment
//! if let Some(config) = AssetHubConfig::from_env() {
//!     let client = ContractClient::new(config).await?;
//!     let queue = EventQueue::new(Arc::new(client));
//!
//!     // Emit events (non-blocking) with wallet addresses
//!     let proctor_wallet: Address = "0x123...".parse().unwrap();
//!     queue.emit(ChainEvent::RoomCreated {
//!         room_id: "123".to_string(),
//!         proctor: proctor_wallet,
//!         proctor_name: Some("Dr. Smith".to_string()),
//!     });
//! }
//! ```

mod config;
mod client;
mod queue;

pub use config::AssetHubConfig;
pub use client::{
    ContractClient,
    Role,
    LeaveReason,
    VerificationStatus,
    SuspiciousActivityType,
    RoomCloseReason,
};
pub use queue::{EventQueue, ChainEvent};

// Re-export Address type for convenience
pub use ethers::types::Address;

use std::sync::Arc;

/// Initializes the substrate module from environment configuration
///
/// Returns `Some((client, queue))` if blockchain integration is enabled and
/// configuration is valid, `None` otherwise.
pub async fn init_from_env() -> Option<(Arc<ContractClient>, EventQueue)> {
    let config = AssetHubConfig::from_env()?;

    tracing::info!("Initializing Asset Hub EVM blockchain integration");

    match ContractClient::new(config).await {
        Ok(client) => {
            let client = Arc::new(client);
            let queue = EventQueue::new(client.clone());
            tracing::info!(
                contract = %client.contract_address(),
                "Asset Hub integration initialized"
            );
            Some((client, queue))
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to initialize Asset Hub client");
            None
        }
    }
}

/// Parses a wallet address from a hex string
///
/// Supports both "0x"-prefixed and raw hex strings.
pub fn parse_address(addr: &str) -> Option<Address> {
    addr.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify that all expected types are exported
        let _: fn() -> AssetHubConfig = || AssetHubConfig {
            enabled: false,
            rpc_url: String::new(),
            private_key: String::new(),
            contract_address: String::new(),
            submission_timeout_secs: 0,
            retry_count: 0,
            gas_limit: 0,
        };
    }

    #[test]
    fn test_parse_address() {
        let addr = parse_address("0x0000000000000000000000000000000000000000");
        assert!(addr.is_some());
        assert_eq!(addr.unwrap(), Address::zero());
    }

    #[test]
    fn test_parse_address_invalid() {
        let addr = parse_address("invalid");
        assert!(addr.is_none());
    }
}
