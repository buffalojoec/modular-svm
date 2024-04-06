//! Agave Validator.

mod callbacks;

use {
    crate::callbacks::AgaveValidatorRuntimeTransactionProcessingCallback,
    agave_program_cache::ForkGraph, agave_runtime::AgaveValidatorRuntime,
    agave_svm::AgaveTransactionBatchProcessor,
};

// This is a grossly over-simplified demonstration of an adapter, bridging the
// SVM-agnostic Agave runtime implementation with the Agave SVM implementation.
// Ideally, this would instead manifest as some module that could be easily
// replaced if another SVM implementation were to be used.
type Svm<FG> =
    AgaveTransactionBatchProcessor<AgaveValidatorRuntimeTransactionProcessingCallback, FG>;

/// A mock Agave Validator.
pub struct AgaveValidator<FG: ForkGraph> {
    pub runtime: AgaveValidatorRuntime<Svm<FG>>,
}
