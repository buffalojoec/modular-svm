//! Agave Validator Runtime Implementation.

mod batch;

use {
    crate::batch::AgaveTransactionBatch,
    solana_runtime::specification::{LoadAndExecuteTransactionsOutput, ValidatorRuntime},
    solana_svm::specification::TransactionBatchProcessor,
};

/// The Agave Validator Runtime.
pub struct AgaveValidatorRuntime<BP: TransactionBatchProcessor> {
    /// SVM-agnostic batch processor.
    pub batch_processor: BP,
}

/// Agave Validator Runtime Base Implementation.
impl<'a, BP: TransactionBatchProcessor> ValidatorRuntime<AgaveTransactionBatch<'a>, BP>
    for AgaveValidatorRuntime<BP>
{
    fn batch_processor(&self) -> &BP {
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
        let _batch_processor = self.batch_processor();
        //
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
