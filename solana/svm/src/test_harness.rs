//! Solana SVM Specification Test Harness

use crate::specification::TransactionBatchProcessor;

/// The Solana SVM Specification Test Harness.
pub struct SolanaSvmTestHarness;

impl SolanaSvmTestHarness {
    pub fn new() -> Self {
        Self
    }

    pub fn case_1<T: TransactionBatchProcessor>(&self, _processor: &T) {
        //
    }

    pub fn case_2<T: TransactionBatchProcessor>(&self, _processor: &T) {
        //
    }

    pub fn case_3<T: TransactionBatchProcessor>(&self, _processor: &T) {
        //
    }

    pub fn case_4<T: TransactionBatchProcessor>(&self, _processor: &T) {
        //
    }

    pub fn run_all<T: TransactionBatchProcessor>(&self, processor: &T) {
        self.case_1(processor);
        self.case_2(processor);
        self.case_3(processor);
        self.case_4(processor);
    }
}
