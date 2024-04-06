//! Agave Validator Runtime Implementation.

mod callbacks;

use {
    crate::callbacks::AgaveValidatorRuntimeTransactionProcessingCallback,
    agave_program_cache::ForkGraph,
    agave_svm::AgaveTransactionBatchProcessor,
    solana_runtime::specification::{
        LoadAndExecuteTransactionsOutput, TransactionBatch, ValidatorRuntime,
    },
    solana_sdk::transaction::{self, SanitizedTransaction},
    std::borrow::Cow,
};

type AgaveTransactionBatchProcessorWithCallback<FG> =
    AgaveTransactionBatchProcessor<AgaveValidatorRuntimeTransactionProcessingCallback, FG>;

/// The Agave Validator Runtime.
pub struct AgaveValidatorRuntime<FG: ForkGraph> {
    pub batch_processor: AgaveTransactionBatchProcessorWithCallback<FG>,
}

/// Agave Validator Runtime Implementation.
impl<'a, FG: ForkGraph>
    ValidatorRuntime<AgaveTransactionBatch<'a>, AgaveTransactionBatchProcessorWithCallback<FG>>
    for AgaveValidatorRuntime<FG>
{
    /// Get the batch processor.
    fn batch_processor(&self) -> &AgaveTransactionBatchProcessorWithCallback<FG> {
        &self.batch_processor
    }

    /// Load and execute a batch of transactions.
    fn load_and_execute_transactions(
        &self,
        _batch: &AgaveTransactionBatch,
    ) -> LoadAndExecuteTransactionsOutput {
        /*
         * MOCK.
         */
        LoadAndExecuteTransactionsOutput {
            loaded_transactions: vec![],
            execution_results: vec![],
            retryable_transaction_indexes: vec![],
            executed_transactions_count: 0,
            executed_non_vote_transactions_count: 0,
            executed_with_successful_result_count: 0,
            signature_count: 0,
        }
    }
}

/// A transaction batch, as defined by the Agave Validator Runtime.
pub struct AgaveTransactionBatch<'a> {
    pub lock_results: Vec<transaction::Result<()>>,
    pub sanitized_txs: Cow<'a, [SanitizedTransaction]>,
    pub needs_unlock: bool,
}

impl TransactionBatch for AgaveTransactionBatch<'_> {
    fn sanitized_txs(&self) -> &[SanitizedTransaction] {
        &self.sanitized_txs
    }
}
