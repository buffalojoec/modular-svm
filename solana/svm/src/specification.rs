//! Solana SVM Specification.

use solana_sdk::{
    inner_instruction::InnerInstructionsList,
    nonce_info::NonceFull,
    rent_debits::RentDebits,
    transaction::{self, SanitizedTransaction, TransactionError},
    transaction_context::{IndexOfAccount, TransactionAccount, TransactionReturnData},
};

/// The Solana SVM Transaction Batch Processor.
/// Primary component of the Solana SVM.
pub trait TransactionBatchProcessor {
    /// The entrypoint to the SVM.
    /// Load and execute a batch of sanitized transactions.
    fn load_and_execute_sanitized_transactions(
        &self,
        sanitized_txs: &[SanitizedTransaction],
    ) -> LoadAndExecuteSanitizedTransactionsOutput;
}

/// The output of the `load_and_execute_sanitized_transactions` method.
pub struct LoadAndExecuteSanitizedTransactionsOutput {
    pub loaded_transactions: Vec<TransactionLoadResult>,
    pub execution_results: Vec<TransactionExecutionResult>,
}

/// A transaction load result, containing the loaded transaction and the nonce.
pub type TransactionLoadResult = (transaction::Result<LoadedTransaction>, Option<NonceFull>);
pub struct LoadedTransaction {
    pub accounts: Vec<TransactionAccount>,
    pub program_indices: Vec<Vec<IndexOfAccount>>,
    pub rent: u64,
    pub rent_debits: RentDebits,
}

/// A transaction execution result, containing the execution details if
/// successful, or the error if unsuccesful.
pub enum TransactionExecutionResult {
    Executed {
        details: TransactionExecutionDetails,
    },
    NotExecuted(TransactionError),
}
pub struct TransactionExecutionDetails {
    pub status: transaction::Result<()>,
    pub log_messages: Option<Vec<String>>,
    pub inner_instructions: Option<InnerInstructionsList>,
    pub durable_nonce_fee: Option<DurableNonceFee>,
    pub return_data: Option<TransactionReturnData>,
    pub executed_units: u64,
    pub accounts_data_len_delta: i64,
}
pub enum DurableNonceFee {
    Valid(u64),
    Invalid,
}
