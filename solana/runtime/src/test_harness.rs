//! Solana Validator Runtime Specification Test Harness

use {
    crate::specification::{TransactionBatch, ValidatorRuntime},
    solana_svm::specification::TransactionBatchProcessor,
};

/// The Solana Validator Runtime Specification Test Harness.
pub struct SolanaRuntimeTestHarness;

impl SolanaRuntimeTestHarness {
    pub fn new() -> Self {
        Self
    }

    pub fn case_1<
        TB: TransactionBatch,
        TP: TransactionBatchProcessor,
        T: ValidatorRuntime<TB, TP>,
    >(
        &self,
        _runtime: &T,
    ) {
        //
    }

    pub fn case_2<
        TB: TransactionBatch,
        TP: TransactionBatchProcessor,
        T: ValidatorRuntime<TB, TP>,
    >(
        &self,
        _runtime: &T,
    ) {
        //
    }

    pub fn case_3<
        TB: TransactionBatch,
        TP: TransactionBatchProcessor,
        T: ValidatorRuntime<TB, TP>,
    >(
        &self,
        _runtime: &T,
    ) {
        //
    }

    pub fn case_4<
        TB: TransactionBatch,
        TP: TransactionBatchProcessor,
        T: ValidatorRuntime<TB, TP>,
    >(
        &self,
        _runtime: &T,
    ) {
        //
    }

    pub fn run_all<
        TB: TransactionBatch,
        TP: TransactionBatchProcessor,
        T: ValidatorRuntime<TB, TP>,
    >(
        &self,
        runtime: &T,
    ) {
        self.case_1(runtime);
        self.case_2(runtime);
        self.case_3(runtime);
        self.case_4(runtime);
    }
}
