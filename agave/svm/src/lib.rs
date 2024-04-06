//! Agave Solana SVM Implementation.

pub mod callbacks;

use {
    crate::callbacks::TransactionProcessingCallback,
    agave_program_cache::{ForkGraph, ProgramCache},
    agave_sysvar_cache::SysvarCache,
    solana_compute_budget::compute_budget::ComputeBudget,
    solana_sdk::{
        account::AccountSharedData,
        clock::{Epoch, Slot},
        epoch_schedule::EpochSchedule,
        fee::FeeStructure,
        native_loader,
        pubkey::Pubkey,
        transaction::{SanitizedTransaction, TransactionError},
    },
    solana_svm::specification::{
        DurableNonceFee, LoadAndExecuteSanitizedTransactionsOutput, LoadedTransaction,
        TransactionBatchProcessor, TransactionExecutionResult, TransactionLoadResult,
    },
    std::{
        cell::RefCell,
        collections::HashMap,
        rc::Rc,
        sync::{Arc, RwLock},
    },
};

pub struct AccountOverrides {
    pub accounts: HashMap<Pubkey, AccountSharedData>,
}

pub struct ExecutionRecordingConfig {
    pub enable_cpi_recording: bool,
    pub enable_log_recording: bool,
    pub enable_return_data_recording: bool,
    pub limit_to_load_programs: bool,
    pub log_messages_bytes_limit: Option<usize>,
}

// ============== EVICT ME ==============
pub struct LoadedProgramsForTxBatch;
// ======================================

pub struct RuntimeConfig {
    pub compute_budget: Option<ComputeBudget>,
    pub log_messages_bytes_limit: Option<usize>,
    pub transaction_account_lock_limit: Option<usize>,
}

pub struct RuntimeEnvironment {
    pub builtin_programs: Vec<Pubkey>,
    pub epoch: Epoch,
    pub epoch_schedule: EpochSchedule,
    pub fee_structure: FeeStructure,
    pub slot: Slot,
}

/// The Agave Solana SVM Transaction Batch Processor.
pub struct AgaveTransactionBatchProcessor<CB: TransactionProcessingCallback, FG: ForkGraph> {
    pub account_overrides: Option<AccountOverrides>,
    pub callbacks: CB,
    pub recording_config: ExecutionRecordingConfig,
    pub runtime_config: Arc<RuntimeConfig>,
    pub runtime_environment: Arc<RuntimeEnvironment>,
    pub sysvar_cache: RwLock<SysvarCache>,
    pub program_cache: Arc<RwLock<ProgramCache<FG>>>,
}

/// Agave SVM Transaction Batch Processor Implementation.
impl<CB: TransactionProcessingCallback, FG: ForkGraph> TransactionBatchProcessor
    for AgaveTransactionBatchProcessor<CB, FG>
{
    /// The entrypoint to the Agave SVM Implementation.
    /// Load and execute a batch of sanitized transactions.
    fn load_and_execute_sanitized_transactions(
        &self,
        sanitized_txs: &[SanitizedTransaction],
    ) -> LoadAndExecuteSanitizedTransactionsOutput {
        /*
         * Mock implementation to demonstrate driving other modular components.
         */
        // [METRICS]: [START]: program_cache_time
        let mut program_accounts_map =
            filter_executable_program_accounts(&self.callbacks, sanitized_txs);
        let native_loader = native_loader::id();
        for builtin_program in &self.runtime_environment.builtin_programs {
            program_accounts_map.insert(*builtin_program, (&native_loader, 0));
        }
        let programs_loaded_for_tx_batch = Rc::new(RefCell::new(
            self.replenish_program_cache(&program_accounts_map),
        ));
        // [METRICS]: [STOP]: program_cache_time

        // [METRICS]: [START]: load_time
        let mut loaded_transactions = load_accounts(
            &self.callbacks,
            sanitized_txs,
            &self.runtime_environment.fee_structure,
            self.account_overrides.as_ref(),
            &program_accounts_map,
            &programs_loaded_for_tx_batch.borrow(),
        );
        // [METRICS]: [STOP]: load_time

        // [METRICS]: [START]: execution_time
        let execution_results: Vec<TransactionExecutionResult> = loaded_transactions
            .iter_mut()
            .zip(sanitized_txs.iter())
            .map(|(accs, tx)| match accs {
                (Err(e), _nonce) => TransactionExecutionResult::NotExecuted(e.clone()),
                (Ok(loaded_transaction), nonce) => {
                    let compute_budget =
                        if let Some(compute_budget) = self.runtime_config.compute_budget {
                            compute_budget
                        } else {
                            // [METRICS]: [START]: compute_budget_process_transaction_time
                            let maybe_compute_budget = ComputeBudget::try_from_instructions(
                                tx.message().program_instructions_iter(),
                            );
                            // [METRICS]: [STOP]: compute_budget_process_transaction_time
                            if let Err(err) = maybe_compute_budget {
                                return TransactionExecutionResult::NotExecuted(err);
                            }
                            maybe_compute_budget.unwrap()
                        };

                    let result = self.execute_loaded_transaction(
                        tx,
                        loaded_transaction,
                        compute_budget,
                        nonce.as_ref().map(DurableNonceFee::from),
                        &programs_loaded_for_tx_batch.borrow(),
                    );

                    // if let TransactionExecutionResult::Executed {
                    //     details,
                    //     programs_modified_by_tx,
                    // } = &result
                    // {
                    //     // Update batch specific cache of the loaded programs with the modifications
                    //     // made by the transaction, if it executed successfully.
                    //     if details.status.is_ok() {
                    //         programs_loaded_for_tx_batch
                    //             .borrow_mut()
                    //             .merge(programs_modified_by_tx);
                    //     }
                    // }

                    result
                }
            })
            .collect();
        // [METRICS]: [STOP]: execution_time

        // const SHRINK_LOADED_PROGRAMS_TO_PERCENTAGE: u8 = 90;
        // self.program_cache
        //     .write()
        //     .unwrap()
        //     .evict_using_2s_random_selection(
        //         Percentage::from(SHRINK_LOADED_PROGRAMS_TO_PERCENTAGE),
        //         self.slot,
        //     );

        /* ... */

        LoadAndExecuteSanitizedTransactionsOutput {
            loaded_transactions,
            execution_results,
        }
    }
}

// Mock helpers below.

impl<CB: TransactionProcessingCallback, FG: ForkGraph> AgaveTransactionBatchProcessor<CB, FG> {
    fn replenish_program_cache(
        &self,
        _program_accounts_map: &HashMap<Pubkey, (&Pubkey, u64)>,
    ) -> LoadedProgramsForTxBatch {
        /*
         * MOCK.
         */
        LoadedProgramsForTxBatch
    }

    fn execute_loaded_transaction(
        &self,
        _tx: &SanitizedTransaction,
        _loaded_transaction: &mut LoadedTransaction,
        _compute_budget: ComputeBudget,
        _durable_nonce_fee: Option<DurableNonceFee>,
        _programs_loaded_for_tx_batch: &LoadedProgramsForTxBatch,
    ) -> TransactionExecutionResult {
        /*
         * MOCK.
         */
        TransactionExecutionResult::NotExecuted(TransactionError::UnsupportedVersion)
    }
}

fn filter_executable_program_accounts<'a, CB: TransactionProcessingCallback>(
    _callbacks: &CB,
    _txs: &[SanitizedTransaction],
) -> HashMap<Pubkey, (&'a Pubkey, u64)> {
    /*
     * MOCK.
     */
    HashMap::new()
}

fn load_accounts<CB: TransactionProcessingCallback>(
    _callbacks: &CB,
    _txs: &[SanitizedTransaction],
    _fee_structure: &FeeStructure,
    _account_overrides: Option<&AccountOverrides>,
    _program_accounts: &HashMap<Pubkey, (&Pubkey, u64)>,
    _loaded_programs: &LoadedProgramsForTxBatch,
) -> Vec<TransactionLoadResult> {
    /*
     * MOCK.
     */
    vec![]
}
