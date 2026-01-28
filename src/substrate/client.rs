use std::sync::Arc;
use std::time::Duration;
use ethers::prelude::*;
use ethers::middleware::NonceManagerMiddleware;
use ethers::providers::{Http, Provider};
use ethers::signers::{LocalWallet, Signer};
use ethers::types::{Address, U256};
use tokio::sync::Mutex;
use tokio::time::timeout;

use super::config::AssetHubConfig;
use crate::error::{Result, SfuError};

/// Role for participants in the proctoring session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Proctor = 0,
    Student = 1,
}

/// Reason for leaving a room
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveReason {
    Normal = 0,
    Kicked = 1,
    Disconnected = 2,
    RoomClosed = 3,
}

/// ID verification status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Valid = 0,
    Invalid = 1,
    Pending = 2,
    Skipped = 3,
}

/// Types of suspicious activity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspiciousActivityType {
    MultipleDevices = 0,
    TabSwitch = 1,
    WindowBlur = 2,
    ScreenShare = 3,
    UnauthorizedPerson = 4,
    AudioAnomaly = 5,
    Other = 6,
}

/// Reason for closing a room
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomCloseReason {
    ProctorLeft = 0,
    SessionCompleted = 1,
    AdminClosed = 2,
    Timeout = 3,
}

// Generate contract bindings from ABI
// Note: All participant/proctor IDs are now wallet addresses
abigen!(
    ProctoringContract,
    r#"[
        function recordRoomCreated(string roomId, address proctor, string proctorName) external
        function recordParticipantJoined(string roomId, address participant, string name, uint8 role) external
        function recordParticipantLeft(string roomId, address participant, uint8 reason) external
        function recordParticipantKicked(string roomId, address proctor, address kicked, string reason) external
        function recordIdVerification(string roomId, address participant, uint8 status, string verifiedBy) external
        function recordSuspiciousActivity(string roomId, address participant, uint8 activityType, string details) external
        function recordRecordingStarted(string roomId, address participant) external
        function recordRecordingStopped(string roomId, address participant, uint64 durationSecs, string ipfsCid) external
        function closeRoom(string roomId, uint8 reason) external
        function createExamResult(string roomId, address participant, uint256 grade, string examName) external returns (uint256)
        function addRecordingToResult(uint256 resultId, string ipfsCid) external
        function addRecordingsToResult(uint256 resultId, string[] ipfsCids) external
        function updateExamResultGrade(uint256 resultId, uint256 newGrade) external
        function markNftMinted(uint256 resultId) external
        function getRoomInfo(string roomId) external view returns (address, string, uint256, uint256, uint32, uint8)
        function getParticipant(string roomId, address participant) external view returns (address, string, uint8, uint256, uint256, uint256)
        function getRoomParticipants(string roomId) external view returns (address[])
        function getParticipantRooms(address participant) external view returns (string[])
        function getParticipantExamResultIds(address participant) external view returns (uint256[])
        function getEventCount(string roomId) external view returns (uint256)
        function getExamResult(uint256 resultId) external view returns (uint256, string, address, uint256, string, uint256, uint256, bool, uint256)
        function getExamResultRecordings(uint256 resultId) external view returns (string[])
        function getRoomParticipantExamResult(string roomId, address participant) external view returns (uint256, uint256, string, uint256, bool, uint256)
        function getStudentsForNft(string roomId) external view returns (address[])
        function getTotalExamResults() external view returns (uint256)
        event RoomCreated(string indexed roomId, address indexed proctor, uint256 timestamp)
        event ParticipantJoined(string indexed roomId, address indexed participant, uint8 role, uint256 timestamp)
        event ParticipantLeft(string indexed roomId, address indexed participant, uint8 reason, uint256 timestamp)
        event ParticipantKicked(string indexed roomId, address indexed kicked, address indexed proctor, uint256 timestamp)
        event RoomClosed(string indexed roomId, uint8 reason, uint256 timestamp)
        event ExamResultCreated(uint256 indexed resultId, string indexed roomId, address indexed participant, uint256 grade, uint256 timestamp)
        event RecordingAdded(uint256 indexed resultId, string ipfsCid, uint256 timestamp)
        event NftMinted(uint256 indexed resultId, address indexed participant, string indexed roomId, uint256 timestamp)
    ]"#
);

type SignerMiddlewareType = NonceManagerMiddleware<SignerMiddleware<Provider<Http>, LocalWallet>>;

/// Client for interacting with the proctoring smart contract on Asset Hub
pub struct ContractClient {
    contract: ProctoringContract<SignerMiddlewareType>,
    submission_timeout: Duration,
    retry_count: u32,
    gas_limit: U256,
    /// Mutex to serialize transaction submissions and avoid nonce conflicts
    tx_mutex: Mutex<()>,
    /// RPC URL for debugging
    rpc_url: String,
}

impl ContractClient {
    /// Creates a new contract client from configuration
    pub async fn new(config: AssetHubConfig) -> Result<Self> {
        tracing::info!(
            rpc_url = %config.rpc_url,
            contract_address = %config.contract_address,
            timeout_secs = config.submission_timeout_secs,
            retry_count = config.retry_count,
            gas_limit = config.gas_limit,
            "Initializing Asset Hub EVM contract client"
        );

        let provider = Provider::<Http>::try_from(&config.rpc_url)
            .map_err(|e| SfuError::SubstrateConnection(format!("Failed to create provider: {}", e)))?;

        // Parse private key
        let wallet: LocalWallet = config.private_key.parse()
            .map_err(|e| SfuError::SubstrateConfig(format!("Invalid private key: {}", e)))?;

        // Get chain ID from provider
        let chain_id = provider.get_chainid().await
            .map_err(|e| SfuError::SubstrateConnection(format!("Failed to get chain ID: {}", e)))?;

        let wallet = wallet.with_chain_id(chain_id.as_u64());

        let wallet_address = wallet.address();
        tracing::info!(
            address = %wallet_address,
            chain_id = %chain_id,
            "Signer wallet initialized"
        );

        // Wrap with NonceManagerMiddleware to properly track nonces and avoid conflicts
        let signer_middleware = SignerMiddleware::new(provider, wallet);
        let nonce_manager = NonceManagerMiddleware::new(signer_middleware, wallet_address);
        let client = Arc::new(nonce_manager);

        // Parse contract address
        let contract_address: Address = config.contract_address.parse()
            .map_err(|e| SfuError::SubstrateConfig(format!("Invalid contract address: {}", e)))?;

        let contract = ProctoringContract::new(contract_address, client);

        tracing::info!(
            contract = %config.contract_address,
            rpc_url = %config.rpc_url,
            "Contract client initialized successfully"
        );

        Ok(Self {
            contract,
            submission_timeout: Duration::from_secs(config.submission_timeout_secs),
            retry_count: config.retry_count,
            gas_limit: U256::from(config.gas_limit),
            tx_mutex: Mutex::new(()),
            rpc_url: config.rpc_url,
        })
    }

    /// Records a room creation event on-chain
    pub async fn record_room_created(
        &self,
        room_id: &str,
        proctor: Address,
        proctor_name: Option<&str>,
    ) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            proctor = %proctor,
            "Recording room creation on-chain"
        );

        let call = self.contract
            .record_room_created(
                room_id.to_string(),
                proctor,
                proctor_name.unwrap_or("").to_string(),
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Records a participant joining on-chain
    pub async fn record_participant_joined(
        &self,
        room_id: &str,
        participant: Address,
        name: Option<&str>,
        role: Role,
    ) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            participant = %participant,
            ?role,
            "Recording participant join on-chain"
        );

        let call = self.contract
            .record_participant_joined(
                room_id.to_string(),
                participant,
                name.unwrap_or("").to_string(),
                role as u8,
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Records a participant leaving on-chain
    pub async fn record_participant_left(
        &self,
        room_id: &str,
        participant: Address,
        reason: LeaveReason,
    ) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            participant = %participant,
            ?reason,
            "Recording participant leave on-chain"
        );

        let call = self.contract
            .record_participant_left(
                room_id.to_string(),
                participant,
                reason as u8,
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Records a participant being kicked on-chain
    pub async fn record_participant_kicked(
        &self,
        room_id: &str,
        proctor: Address,
        kicked: Address,
        reason: Option<&str>,
    ) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            proctor = %proctor,
            kicked = %kicked,
            "Recording participant kick on-chain"
        );

        let call = self.contract
            .record_participant_kicked(
                room_id.to_string(),
                proctor,
                kicked,
                reason.unwrap_or("").to_string(),
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Records an ID verification result on-chain
    pub async fn record_id_verification(
        &self,
        room_id: &str,
        participant: Address,
        status: VerificationStatus,
        verified_by: &str,
    ) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            participant = %participant,
            ?status,
            "Recording ID verification on-chain"
        );

        let call = self.contract
            .record_id_verification(
                room_id.to_string(),
                participant,
                status as u8,
                verified_by.to_string(),
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Records suspicious activity on-chain
    pub async fn record_suspicious_activity(
        &self,
        room_id: &str,
        participant: Address,
        activity_type: SuspiciousActivityType,
        details: Option<&str>,
    ) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            participant = %participant,
            ?activity_type,
            "Recording suspicious activity on-chain"
        );

        let call = self.contract
            .record_suspicious_activity(
                room_id.to_string(),
                participant,
                activity_type as u8,
                details.unwrap_or("").to_string(),
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Records recording started on-chain
    pub async fn record_recording_started(&self, room_id: &str, participant: Address) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            participant = %participant,
            "Recording start event on-chain"
        );

        let call = self.contract
            .record_recording_started(room_id.to_string(), participant)
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Records recording stopped on-chain
    pub async fn record_recording_stopped(
        &self,
        room_id: &str,
        participant: Address,
        duration_secs: u64,
        ipfs_cid: Option<&str>,
    ) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            participant = %participant,
            duration_secs = duration_secs,
            ?ipfs_cid,
            "Recording stop event on-chain"
        );

        let call = self.contract
            .record_recording_stopped(
                room_id.to_string(),
                participant,
                duration_secs,
                ipfs_cid.unwrap_or("").to_string(),
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Closes a room on-chain
    pub async fn close_room(&self, room_id: &str, reason: RoomCloseReason) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            ?reason,
            "Recording room close on-chain"
        );

        let call = self.contract
            .close_room(room_id.to_string(), reason as u8)
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Creates an exam result for a participant (for NFT generation)
    pub async fn create_exam_result(
        &self,
        room_id: &str,
        participant: Address,
        grade: u64,
        exam_name: &str,
    ) -> Result<()> {
        tracing::debug!(
            room_id = %room_id,
            participant = %participant,
            grade = grade,
            exam_name = %exam_name,
            "Creating exam result on-chain"
        );

        let call = self.contract
            .create_exam_result(
                room_id.to_string(),
                participant,
                U256::from(grade),
                exam_name.to_string(),
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry_generic(call).await
    }

    /// Adds a recording CID to an existing exam result
    pub async fn add_recording_to_result(
        &self,
        result_id: u64,
        ipfs_cid: &str,
    ) -> Result<()> {
        tracing::debug!(
            result_id = result_id,
            ipfs_cid = %ipfs_cid,
            "Adding recording to exam result on-chain"
        );

        let call = self.contract
            .add_recording_to_result(
                U256::from(result_id),
                ipfs_cid.to_string(),
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Adds multiple recording CIDs to an existing exam result
    pub async fn add_recordings_to_result(
        &self,
        result_id: u64,
        ipfs_cids: Vec<String>,
    ) -> Result<()> {
        tracing::debug!(
            result_id = result_id,
            cid_count = ipfs_cids.len(),
            "Adding recordings to exam result on-chain"
        );

        let call = self.contract
            .add_recordings_to_result(
                U256::from(result_id),
                ipfs_cids,
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Updates the grade of an exam result
    pub async fn update_exam_result_grade(
        &self,
        result_id: u64,
        new_grade: u64,
    ) -> Result<()> {
        tracing::debug!(
            result_id = result_id,
            new_grade = new_grade,
            "Updating exam result grade on-chain"
        );

        let call = self.contract
            .update_exam_result_grade(
                U256::from(result_id),
                U256::from(new_grade),
            )
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Marks an NFT as minted for an exam result
    pub async fn mark_nft_minted(&self, result_id: u64) -> Result<()> {
        tracing::debug!(
            result_id = result_id,
            "Marking NFT as minted on-chain"
        );

        let call = self.contract
            .mark_nft_minted(U256::from(result_id))
            .gas(self.gas_limit);

        self.send_tx_with_retry(call).await
    }

    /// Sends a transaction with retry logic
    async fn send_tx_with_retry(
        &self,
        call: ContractCall<SignerMiddlewareType, ()>,
    ) -> Result<()> {
        // Acquire lock for the entire retry loop to ensure transactions are serialized
        let _guard = self.tx_mutex.lock().await;
        let mut last_error = None;

        for attempt in 0..self.retry_count {
            match self.try_send_tx(&call).await {
                Ok(()) => {
                    tracing::debug!("Transaction successful");
                    return Ok(());
                }
                Err(e) => {
                    let error_str = e.to_string();
                    let is_rpc_error = error_str.contains("502")
                        || error_str.contains("404")
                        || error_str.contains("503")
                        || error_str.contains("429");
                    let is_nonce_error = error_str.contains("Priority is too low")
                        || error_str.contains("nonce")
                        || error_str.contains("already known");

                    tracing::warn!(
                        attempt = attempt + 1,
                        max_retries = self.retry_count,
                        error = %e,
                        is_rpc_error = is_rpc_error,
                        is_nonce_error = is_nonce_error,
                        rpc_url = %self.rpc_url,
                        "Transaction failed, retrying"
                    );
                    last_error = Some(e);

                    if attempt < self.retry_count - 1 {
                        // Longer backoff for nonce/RPC errors
                        let delay_secs = if is_nonce_error {
                            // Nonce errors: wait longer for pending tx to clear (10s, 20s, 30s...)
                            10 * (attempt as u64 + 1)
                        } else if is_rpc_error {
                            // RPC errors: 5s, 10s, 15s, 20s...
                            5 * (attempt as u64 + 1)
                        } else {
                            // Other errors: 3s, 6s, 9s, 12s...
                            3 * (attempt as u64 + 1)
                        };
                        tracing::info!(delay_secs = delay_secs, "Waiting before retry");
                        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    }
                }
            }
        }

        tracing::error!(
            rpc_url = %self.rpc_url,
            retries = self.retry_count,
            "Transaction failed after all retries"
        );
        Err(last_error.unwrap_or_else(|| {
            SfuError::ContractCallFailed("Transaction failed after retries".to_string())
        }))
    }

    /// Sends a transaction with retry logic for calls that return a value
    async fn send_tx_with_retry_generic<T: ethers::abi::Detokenize>(
        &self,
        call: ContractCall<SignerMiddlewareType, T>,
    ) -> Result<()> {
        // Acquire lock for the entire retry loop to ensure transactions are serialized
        let _guard = self.tx_mutex.lock().await;
        let mut last_error = None;

        for attempt in 0..self.retry_count {
            match self.try_send_tx_generic(&call).await {
                Ok(()) => {
                    tracing::debug!("Transaction successful");
                    return Ok(());
                }
                Err(e) => {
                    let error_str = e.to_string();
                    let is_rpc_error = error_str.contains("502")
                        || error_str.contains("404")
                        || error_str.contains("503")
                        || error_str.contains("429");
                    let is_nonce_error = error_str.contains("Priority is too low")
                        || error_str.contains("nonce")
                        || error_str.contains("already known");

                    tracing::warn!(
                        attempt = attempt + 1,
                        max_retries = self.retry_count,
                        error = %e,
                        is_rpc_error = is_rpc_error,
                        is_nonce_error = is_nonce_error,
                        rpc_url = %self.rpc_url,
                        "Transaction failed, retrying"
                    );
                    last_error = Some(e);

                    if attempt < self.retry_count - 1 {
                        // Longer backoff for nonce/RPC errors
                        let delay_secs = if is_nonce_error {
                            // Nonce errors: wait longer for pending tx to clear (10s, 20s, 30s...)
                            10 * (attempt as u64 + 1)
                        } else if is_rpc_error {
                            // RPC errors: 5s, 10s, 15s, 20s...
                            5 * (attempt as u64 + 1)
                        } else {
                            // Other errors: 3s, 6s, 9s, 12s...
                            3 * (attempt as u64 + 1)
                        };
                        tracing::info!(delay_secs = delay_secs, "Waiting before retry");
                        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    }
                }
            }
        }

        tracing::error!(
            rpc_url = %self.rpc_url,
            retries = self.retry_count,
            "Transaction failed after all retries"
        );
        Err(last_error.unwrap_or_else(|| {
            SfuError::ContractCallFailed("Transaction failed after retries".to_string())
        }))
    }

    /// Attempts a single transaction
    async fn try_send_tx(
        &self,
        call: &ContractCall<SignerMiddlewareType, ()>,
    ) -> Result<()> {
        let send_future = async {
            let pending_tx = call.send().await
                .map_err(|e| SfuError::ContractCallFailed(format!("Failed to send tx: {}", e)))?;

            let receipt = pending_tx.await
                .map_err(|e| SfuError::ContractCallFailed(format!("Failed to confirm tx: {}", e)))?
                .ok_or_else(|| SfuError::ContractCallFailed("No receipt returned".to_string()))?;

            // Check transaction status - status = 1 means success, 0 means revert
            if receipt.status == Some(ethers::types::U64::from(0)) {
                return Err(SfuError::ContractCallFailed(format!(
                    "Transaction reverted: tx_hash={:?}",
                    receipt.transaction_hash
                )));
            }

            tracing::debug!(
                tx_hash = ?receipt.transaction_hash,
                gas_used = ?receipt.gas_used,
                "Transaction confirmed"
            );

            Ok::<(), SfuError>(())
        };

        timeout(self.submission_timeout, send_future)
            .await
            .map_err(|_| SfuError::Timeout("Transaction timed out".to_string()))?
    }

    /// Attempts a single transaction for calls that return a value
    async fn try_send_tx_generic<T: ethers::abi::Detokenize>(
        &self,
        call: &ContractCall<SignerMiddlewareType, T>,
    ) -> Result<()> {
        let send_future = async {
            let pending_tx = call.send().await
                .map_err(|e| SfuError::ContractCallFailed(format!("Failed to send tx: {}", e)))?;

            let receipt = pending_tx.await
                .map_err(|e| SfuError::ContractCallFailed(format!("Failed to confirm tx: {}", e)))?
                .ok_or_else(|| SfuError::ContractCallFailed("No receipt returned".to_string()))?;

            // Check transaction status - status = 1 means success, 0 means revert
            if receipt.status == Some(ethers::types::U64::from(0)) {
                return Err(SfuError::ContractCallFailed(format!(
                    "Transaction reverted: tx_hash={:?}",
                    receipt.transaction_hash
                )));
            }

            tracing::debug!(
                tx_hash = ?receipt.transaction_hash,
                gas_used = ?receipt.gas_used,
                "Transaction confirmed"
            );

            Ok::<(), SfuError>(())
        };

        timeout(self.submission_timeout, send_future)
            .await
            .map_err(|_| SfuError::Timeout("Transaction timed out".to_string()))?
    }

    /// Returns the contract address
    pub fn contract_address(&self) -> Address {
        self.contract.address()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_values() {
        assert_eq!(Role::Proctor as u8, 0);
        assert_eq!(Role::Student as u8, 1);
    }

    #[test]
    fn test_leave_reason_values() {
        assert_eq!(LeaveReason::Normal as u8, 0);
        assert_eq!(LeaveReason::Kicked as u8, 1);
        assert_eq!(LeaveReason::Disconnected as u8, 2);
        assert_eq!(LeaveReason::RoomClosed as u8, 3);
    }

    #[test]
    fn test_suspicious_activity_type_values() {
        assert_eq!(SuspiciousActivityType::MultipleDevices as u8, 0);
        assert_eq!(SuspiciousActivityType::TabSwitch as u8, 1);
        assert_eq!(SuspiciousActivityType::Other as u8, 6);
    }
}
