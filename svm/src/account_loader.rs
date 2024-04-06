use {
    crate::{
        account_overrides::AccountOverrides, transaction_error_metrics::TransactionErrorMetrics,
        transaction_processor::TransactionProcessingCallback,
    },
    solana_program_runtime::loaded_programs::LoadedProgramsForTxBatch,
    solana_sdk::{
        account::AccountSharedData,
        fee::FeeStructure,
        nonce_info::{NonceFull, NoncePartial},
        pubkey::Pubkey,
        rent_collector::RentCollector,
        rent_debits::RentDebits,
        transaction::{self, Result, SanitizedTransaction},
        transaction_context::{IndexOfAccount, TransactionAccount},
    },
    std::collections::HashMap,
};

// for the load instructions
pub(crate) type TransactionRent = u64;
pub(crate) type TransactionProgramIndices = Vec<Vec<IndexOfAccount>>;
pub type TransactionCheckResult = (transaction::Result<()>, Option<NoncePartial>, Option<u64>);
pub type TransactionLoadResult = (Result<LoadedTransaction>, Option<NonceFull>);

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct LoadedTransaction {
    pub accounts: Vec<TransactionAccount>,
    pub program_indices: TransactionProgramIndices,
    pub rent: TransactionRent,
    pub rent_debits: RentDebits,
}

/// Check whether the payer_account is capable of paying the fee. The
/// side effect is to subtract the fee amount from the payer_account
/// balance of lamports. If the payer_acount is not able to pay the
/// fee, the error_counters is incremented, and a specific error is
/// returned.
pub fn validate_fee_payer(
    _payer_address: &Pubkey,
    _payer_account: &mut AccountSharedData,
    _payer_index: IndexOfAccount,
    _error_counters: &mut TransactionErrorMetrics,
    _rent_collector: &RentCollector,
    _fee: u64,
) -> Result<()> {
    /*
     * Function simplified for brevity.
     */
    Ok(())
}

/// Collect information about accounts used in txs transactions and
/// return vector of tuples, one for each transaction in the
/// batch. Each tuple contains struct of information about accounts as
/// its first element and an optional transaction nonce info as its
/// second element.
pub(crate) fn load_accounts<CB: TransactionProcessingCallback>(
    _callbacks: &CB,
    _txs: &[SanitizedTransaction],
    _lock_results: &[TransactionCheckResult],
    _error_counters: &mut TransactionErrorMetrics,
    _fee_structure: &FeeStructure,
    _account_overrides: Option<&AccountOverrides>,
    _program_accounts: &HashMap<Pubkey, (&Pubkey, u64)>,
    _loaded_programs: &LoadedProgramsForTxBatch,
) -> Vec<TransactionLoadResult> {
    /*
     * Function simplified for brevity.
     */
    vec![]
}
