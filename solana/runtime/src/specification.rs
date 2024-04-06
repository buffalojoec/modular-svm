//! Solana Validator Runtime Specification.

use {
    solana_sdk::transaction::SanitizedTransaction,
    solana_svm::specification::{
        TransactionBatchProcessor, TransactionExecutionResult, TransactionLoadResult,
    },
};

/// The Solana Validator Runtime.
pub trait ValidatorRuntime<TB: TransactionBatch, TP: TransactionBatchProcessor> {
    /// Get the batch processor.
    fn batch_processor(&self) -> &TP;
    /// Load and execute a batch of transactions.
    fn load_and_execute_transactions(&self, batch: &TB) -> LoadAndExecuteTransactionsOutput;
}

/// A batch of Solana transactions.
pub trait TransactionBatch {
    /// Get the sanitized transactions.
    fn sanitized_txs(&self) -> &[SanitizedTransaction];
}

/// The output of the `load_and_execute_transactions` method.
pub struct LoadAndExecuteTransactionsOutput {
    pub loaded_transactions: Vec<TransactionLoadResult>,
    pub execution_results: Vec<TransactionExecutionResult>,
    pub retryable_transaction_indexes: Vec<usize>,
    pub executed_transactions_count: usize,
    pub executed_non_vote_transactions_count: usize,
    pub executed_with_successful_result_count: usize,
    pub signature_count: u64,
}
