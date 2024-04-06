use {
    agave_svm::callbacks::TransactionProcessingCallback,
    solana_sdk::{
        account::AccountSharedData, feature_set::FeatureSet, hash::Hash, pubkey::Pubkey,
        rent_collector::RentCollector,
    },
    std::sync::Arc,
};

/// Simply a mock runtime callback implementation for the Agave Validator.
pub struct AgaveValidatorRuntimeTransactionProcessingCallback;

impl TransactionProcessingCallback for AgaveValidatorRuntimeTransactionProcessingCallback {
    fn account_matches_owners(&self, _account: &Pubkey, _owners: &[Pubkey]) -> Option<usize> {
        todo!()
    }

    fn get_account_shared_data(&self, _pubkey: &Pubkey) -> Option<AccountSharedData> {
        todo!()
    }

    fn get_last_blockhash_and_lamports_per_signature(&self) -> (Hash, u64) {
        todo!()
    }

    fn get_rent_collector(&self) -> &RentCollector {
        todo!()
    }

    fn get_feature_set(&self) -> Arc<FeatureSet> {
        todo!()
    }
}
