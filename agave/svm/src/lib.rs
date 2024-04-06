//! Agave Solana SVM Implementation.

use {
    agave_program_cache::{ForkGraph, ProgramCache},
    agave_sysvar_cache::SysvarCache,
    solana_sdk::{
        account::AccountSharedData,
        clock::{Epoch, Slot},
        epoch_schedule::EpochSchedule,
        fee::FeeStructure,
        pubkey::Pubkey,
        transaction::SanitizedTransaction,
    },
    solana_svm::specification::{
        LoadAndExecuteSanitizedTransactionsOutput, TransactionBatchProcessor,
    },
    std::{
        collections::HashMap,
        sync::{Arc, RwLock},
    },
};

pub struct AccountOverrides {
    pub accounts: HashMap<Pubkey, AccountSharedData>,
}

pub struct ExecutionRecordingConfig {
    pub enable_cpi_recording: bool,
    pub enable_log_recording: bool,
    pub enable_return_data_recording: bool,
    pub limit_to_load_programs: bool,
    pub log_messages_bytes_limit: Option<usize>,
}

// ============== EVICT ME ==============
pub struct RuntimeConfig;
// ======================================

pub struct RuntimeEnvironment {
    pub builtin_programs: Vec<Pubkey>,
    pub epoch: Epoch,
    pub epoch_schedule: EpochSchedule,
    pub fee_structure: FeeStructure,
    pub slot: Slot,
}

/// The Agave Solana SVM Transaction Batch Processor.
pub struct AgaveTransactionBatchProcessor<FG: ForkGraph> {
    pub account_overrides: AccountOverrides,
    pub recording_config: ExecutionRecordingConfig,
    pub runtime_config: Arc<RuntimeConfig>,
    pub runtime_environment: Arc<RuntimeEnvironment>,
    pub sysvar_cache: RwLock<SysvarCache>,
    pub program_cache: Arc<RwLock<ProgramCache<FG>>>,
}

/// Agave SVM Transaction Batch Processor Implementation.
impl<FG: ForkGraph> TransactionBatchProcessor for AgaveTransactionBatchProcessor<FG> {
    /// The entrypoint to the Agave SVM Implementation.
    /// Load and execute a batch of sanitized transactions.
    fn load_and_execute_sanitized_transactions(
        &self,
        _sanitized_txs: &[SanitizedTransaction],
    ) -> LoadAndExecuteSanitizedTransactionsOutput {
        /*
         * Mock implementation to demonstrate driving other modular components.
         */
        todo!()
    }
}
