use {
    solana_runtime::specification::TransactionBatch,
    solana_sdk::transaction::{self, SanitizedTransaction},
    std::borrow::Cow,
};

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
