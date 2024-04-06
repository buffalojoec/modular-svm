use {
    crate::{
        account_loader::{
            load_accounts, LoadedTransaction, TransactionCheckResult, TransactionLoadResult,
        },
        account_overrides::AccountOverrides,
        message_processor::MessageProcessor,
        transaction_error_metrics::TransactionErrorMetrics,
        transaction_results::{
            DurableNonceFee, TransactionExecutionDetails, TransactionExecutionResult,
        },
    },
    log::debug,
    percentage::Percentage,
    solana_measure::measure::Measure,
    solana_program_runtime::{
        compute_budget::ComputeBudget,
        invoke_context::InvokeContext,
        loaded_programs::{
            ForkGraph, LoadedProgram, LoadedProgramMatchCriteria, LoadedProgramsForTxBatch,
            ProgramCache,
        },
        runtime_config::RuntimeConfig,
        sysvar_cache::SysvarCache,
        timings::{ExecuteTimingType, ExecuteTimings},
    },
    solana_sdk::{
        account::{AccountSharedData, ReadableAccount, PROGRAM_OWNERS},
        clock::{Epoch, Slot},
        epoch_schedule::EpochSchedule,
        feature_set::FeatureSet,
        fee::FeeStructure,
        hash::Hash,
        message::SanitizedMessage,
        native_loader,
        pubkey::Pubkey,
        rent_collector::RentCollector,
        saturating_add_assign,
        transaction::{self, SanitizedTransaction, TransactionError},
        transaction_context::{ExecutionRecord, TransactionContext},
    },
    std::{
        cell::RefCell,
        collections::HashMap,
        fmt::{Debug, Formatter},
        rc::Rc,
        sync::{Arc, RwLock},
    },
};

pub type TransactionLogMessages = Vec<String>;

pub struct LoadAndExecuteSanitizedTransactionsOutput {
    pub loaded_transactions: Vec<TransactionLoadResult>,
    pub execution_results: Vec<TransactionExecutionResult>,
}

#[derive(Copy, Clone)]
pub struct ExecutionRecordingConfig {
    pub enable_cpi_recording: bool,
    pub enable_log_recording: bool,
    pub enable_return_data_recording: bool,
}

impl ExecutionRecordingConfig {
    pub fn new_single_setting(option: bool) -> Self {
        ExecutionRecordingConfig {
            enable_return_data_recording: option,
            enable_log_recording: option,
            enable_cpi_recording: option,
        }
    }
}

pub trait TransactionProcessingCallback {
    fn account_matches_owners(&self, account: &Pubkey, owners: &[Pubkey]) -> Option<usize>;

    fn get_account_shared_data(&self, pubkey: &Pubkey) -> Option<AccountSharedData>;

    fn get_last_blockhash_and_lamports_per_signature(&self) -> (Hash, u64);

    fn get_rent_collector(&self) -> &RentCollector;

    fn get_feature_set(&self) -> Arc<FeatureSet>;

    fn check_account_access(
        &self,
        _message: &SanitizedMessage,
        _account_index: usize,
        _account: &AccountSharedData,
        _error_counters: &mut TransactionErrorMetrics,
    ) -> transaction::Result<()> {
        Ok(())
    }

    fn get_program_match_criteria(&self, _program: &Pubkey) -> LoadedProgramMatchCriteria {
        LoadedProgramMatchCriteria::NoCriteria
    }
}

pub struct TransactionBatchProcessor<FG: ForkGraph> {
    slot: Slot,
    epoch: Epoch,
    epoch_schedule: EpochSchedule,
    fee_structure: FeeStructure,
    runtime_config: Arc<RuntimeConfig>,
    pub sysvar_cache: RwLock<SysvarCache>,
    pub program_cache: Arc<RwLock<ProgramCache<FG>>>,
}

impl<FG: ForkGraph> Debug for TransactionBatchProcessor<FG> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionBatchProcessor")
            .field("slot", &self.slot)
            .field("epoch", &self.epoch)
            .field("epoch_schedule", &self.epoch_schedule)
            .field("fee_structure", &self.fee_structure)
            .field("runtime_config", &self.runtime_config)
            .field("sysvar_cache", &self.sysvar_cache)
            .field("program_cache", &self.program_cache)
            .finish()
    }
}

impl<FG: ForkGraph> Default for TransactionBatchProcessor<FG> {
    fn default() -> Self {
        Self {
            slot: Slot::default(),
            epoch: Epoch::default(),
            epoch_schedule: EpochSchedule::default(),
            fee_structure: FeeStructure::default(),
            runtime_config: Arc::<RuntimeConfig>::default(),
            sysvar_cache: RwLock::<SysvarCache>::default(),
            program_cache: Arc::new(RwLock::new(ProgramCache::new(
                Slot::default(),
                Epoch::default(),
            ))),
        }
    }
}

impl<FG: ForkGraph> TransactionBatchProcessor<FG> {
    pub fn new(
        slot: Slot,
        epoch: Epoch,
        epoch_schedule: EpochSchedule,
        fee_structure: FeeStructure,
        runtime_config: Arc<RuntimeConfig>,
        program_cache: Arc<RwLock<ProgramCache<FG>>>,
    ) -> Self {
        Self {
            slot,
            epoch,
            epoch_schedule,
            fee_structure,
            runtime_config,
            sysvar_cache: RwLock::<SysvarCache>::default(),
            program_cache,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn load_and_execute_sanitized_transactions<'a, CB: TransactionProcessingCallback>(
        &self,
        callbacks: &CB,
        sanitized_txs: &[SanitizedTransaction],
        check_results: &mut [TransactionCheckResult],
        error_counters: &mut TransactionErrorMetrics,
        recording_config: ExecutionRecordingConfig,
        timings: &mut ExecuteTimings,
        account_overrides: Option<&AccountOverrides>,
        builtin_programs: impl Iterator<Item = &'a Pubkey>,
        log_messages_bytes_limit: Option<usize>,
        limit_to_load_programs: bool,
    ) -> LoadAndExecuteSanitizedTransactionsOutput {
        let mut program_cache_time = Measure::start("program_cache");
        let mut program_accounts_map = Self::filter_executable_program_accounts(
            callbacks,
            sanitized_txs,
            check_results,
            PROGRAM_OWNERS,
        );
        let native_loader = native_loader::id();
        for builtin_program in builtin_programs {
            program_accounts_map.insert(*builtin_program, (&native_loader, 0));
        }

        let programs_loaded_for_tx_batch = Rc::new(RefCell::new(self.replenish_program_cache(
            callbacks,
            &program_accounts_map,
            limit_to_load_programs,
        )));

        if programs_loaded_for_tx_batch.borrow().hit_max_limit {
            return LoadAndExecuteSanitizedTransactionsOutput {
                loaded_transactions: vec![],
                execution_results: vec![],
            };
        }
        program_cache_time.stop();

        let mut load_time = Measure::start("accounts_load");
        let mut loaded_transactions = load_accounts(
            callbacks,
            sanitized_txs,
            check_results,
            error_counters,
            &self.fee_structure,
            account_overrides,
            &program_accounts_map,
            &programs_loaded_for_tx_batch.borrow(),
        );
        load_time.stop();

        let mut execution_time = Measure::start("execution_time");

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
                            let mut compute_budget_process_transaction_time =
                                Measure::start("compute_budget_process_transaction_time");
                            let maybe_compute_budget = ComputeBudget::try_from_instructions(
                                tx.message().program_instructions_iter(),
                            );
                            compute_budget_process_transaction_time.stop();
                            saturating_add_assign!(
                                timings
                                    .execute_accessories
                                    .compute_budget_process_transaction_us,
                                compute_budget_process_transaction_time.as_us()
                            );
                            if let Err(err) = maybe_compute_budget {
                                return TransactionExecutionResult::NotExecuted(err);
                            }
                            maybe_compute_budget.unwrap()
                        };

                    let result = self.execute_loaded_transaction(
                        callbacks,
                        tx,
                        loaded_transaction,
                        compute_budget,
                        nonce.as_ref().map(DurableNonceFee::from),
                        recording_config,
                        timings,
                        error_counters,
                        log_messages_bytes_limit,
                        &programs_loaded_for_tx_batch.borrow(),
                    );

                    if let TransactionExecutionResult::Executed {
                        details,
                        programs_modified_by_tx,
                    } = &result
                    {
                        // Update batch specific cache of the loaded programs with the modifications
                        // made by the transaction, if it executed successfully.
                        if details.status.is_ok() {
                            programs_loaded_for_tx_batch
                                .borrow_mut()
                                .merge(programs_modified_by_tx);
                        }
                    }

                    result
                }
            })
            .collect();

        execution_time.stop();

        const SHRINK_LOADED_PROGRAMS_TO_PERCENTAGE: u8 = 90;
        self.program_cache
            .write()
            .unwrap()
            .evict_using_2s_random_selection(
                Percentage::from(SHRINK_LOADED_PROGRAMS_TO_PERCENTAGE),
                self.slot,
            );

        debug!(
            "load: {}us execute: {}us txs_len={}",
            load_time.as_us(),
            execution_time.as_us(),
            sanitized_txs.len(),
        );

        timings.saturating_add_in_place(
            ExecuteTimingType::ProgramCacheUs,
            program_cache_time.as_us(),
        );
        timings.saturating_add_in_place(ExecuteTimingType::LoadUs, load_time.as_us());
        timings.saturating_add_in_place(ExecuteTimingType::ExecuteUs, execution_time.as_us());

        LoadAndExecuteSanitizedTransactionsOutput {
            loaded_transactions,
            execution_results,
        }
    }

    fn filter_executable_program_accounts<'a, CB: TransactionProcessingCallback>(
        _callbacks: &CB,
        _txs: &[SanitizedTransaction],
        _check_results: &mut [TransactionCheckResult],
        _program_owners: &'a [Pubkey],
    ) -> HashMap<Pubkey, (&'a Pubkey, u64)> {
        /*
         * Function simplified for brevity.
         */
        HashMap::new()
    }

    pub fn load_program_with_pubkey<CB: TransactionProcessingCallback>(
        &self,
        _callbacks: &CB,
        _pubkey: &Pubkey,
        _reload: bool,
        _effective_epoch: Epoch,
    ) -> Option<Arc<LoadedProgram>> {
        /*
         * Function simplified for brevity.
         */
        None
    }

    fn replenish_program_cache<CB: TransactionProcessingCallback>(
        &self,
        _callback: &CB,
        _program_accounts_map: &HashMap<Pubkey, (&Pubkey, u64)>,
        _limit_to_load_programs: bool,
    ) -> LoadedProgramsForTxBatch {
        /*
         * Function simplified for brevity.
         */
        LoadedProgramsForTxBatch::default()
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_loaded_transaction<CB: TransactionProcessingCallback>(
        &self,
        callback: &CB,
        tx: &SanitizedTransaction,
        loaded_transaction: &mut LoadedTransaction,
        compute_budget: ComputeBudget,
        durable_nonce_fee: Option<DurableNonceFee>,
        recording_config: ExecutionRecordingConfig,
        timings: &mut ExecuteTimings,
        _error_counters: &mut TransactionErrorMetrics,
        _log_messages_bytes_limit: Option<usize>,
        programs_loaded_for_tx_batch: &LoadedProgramsForTxBatch,
    ) -> TransactionExecutionResult {
        /*
         * Function simplified for brevity.
         */
        let transaction_accounts = std::mem::take(&mut loaded_transaction.accounts);

        fn transaction_accounts_lamports_sum(
            accounts: &[(Pubkey, AccountSharedData)],
            message: &SanitizedMessage,
        ) -> Option<u128> {
            let mut lamports_sum = 0u128;
            for i in 0..message.account_keys().len() {
                let (_, account) = accounts.get(i)?;
                lamports_sum = lamports_sum.checked_add(u128::from(account.lamports()))?;
            }
            Some(lamports_sum)
        }

        let lamports_before_tx =
            transaction_accounts_lamports_sum(&transaction_accounts, tx.message()).unwrap_or(0);

        let mut transaction_context = TransactionContext::new(
            transaction_accounts,
            callback.get_rent_collector().rent.clone(),
            compute_budget.max_invoke_stack_height,
            compute_budget.max_instruction_trace_length,
        );
        #[cfg(debug_assertions)]
        transaction_context.set_signature(tx.signature());

        let (blockhash, lamports_per_signature) =
            callback.get_last_blockhash_and_lamports_per_signature();

        let mut executed_units = 0u64;
        let mut programs_modified_by_tx = LoadedProgramsForTxBatch::new(
            self.slot,
            programs_loaded_for_tx_batch.environments.clone(),
            programs_loaded_for_tx_batch.upcoming_environments.clone(),
            programs_loaded_for_tx_batch.latest_root_epoch,
        );
        let sysvar_cache = &self.sysvar_cache.read().unwrap();

        let mut invoke_context = InvokeContext::new(
            &mut transaction_context,
            sysvar_cache,
            None,
            compute_budget,
            programs_loaded_for_tx_batch,
            &mut programs_modified_by_tx,
            callback.get_feature_set(),
            blockhash,
            lamports_per_signature,
        );

        let mut process_message_time = Measure::start("process_message_time");
        let _process_result = MessageProcessor::process_message(
            tx.message(),
            &loaded_transaction.program_indices,
            &mut invoke_context,
            timings,
            &mut executed_units,
        );
        process_message_time.stop();

        drop(invoke_context);

        saturating_add_assign!(
            timings.execute_accessories.process_message_us,
            process_message_time.as_us()
        );

        let mut status = Ok(());

        let ExecutionRecord {
            accounts,
            return_data,
            touched_account_count,
            accounts_resize_delta: accounts_data_len_delta,
        } = transaction_context.into();

        if status.is_ok()
            && transaction_accounts_lamports_sum(&accounts, tx.message())
                .filter(|lamports_after_tx| lamports_before_tx == *lamports_after_tx)
                .is_none()
        {
            status = Err(TransactionError::UnbalancedTransaction);
        }
        let status = status.map(|_| ());

        loaded_transaction.accounts = accounts;
        saturating_add_assign!(
            timings.details.total_account_count,
            loaded_transaction.accounts.len() as u64
        );
        saturating_add_assign!(timings.details.changed_account_count, touched_account_count);

        let return_data =
            if recording_config.enable_return_data_recording && !return_data.data.is_empty() {
                Some(return_data)
            } else {
                None
            };

        TransactionExecutionResult::Executed {
            details: TransactionExecutionDetails {
                status,
                log_messages: None,
                inner_instructions: None,
                durable_nonce_fee,
                return_data,
                executed_units,
                accounts_data_len_delta,
            },
            programs_modified_by_tx: Box::new(programs_modified_by_tx),
        }
    }
}
