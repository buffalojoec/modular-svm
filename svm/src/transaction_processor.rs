use {
    crate::{
        account_loader::{
            load_accounts, LoadedTransaction, TransactionCheckResult, TransactionLoadResult,
        },
        account_overrides::AccountOverrides,
        message_processor::MessageProcessor,
        transaction_account_state_info::TransactionAccountStateInfo,
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
            ForkGraph, LoadProgramMetrics, LoadedProgram, LoadedProgramMatchCriteria,
            LoadedProgramType, LoadedProgramsForTxBatch, ProgramCache, ProgramRuntimeEnvironment,
            DELAY_VISIBILITY_SLOT_OFFSET,
        },
        log_collector::LogCollector,
        runtime_config::RuntimeConfig,
        sysvar_cache::SysvarCache,
        timings::{ExecuteDetailsTimings, ExecuteTimingType, ExecuteTimings},
    },
    solana_sdk::{
        account::{AccountSharedData, ReadableAccount, PROGRAM_OWNERS},
        account_utils::StateMut,
        bpf_loader_upgradeable::{self, UpgradeableLoaderState},
        clock::{Epoch, Slot},
        epoch_schedule::EpochSchedule,
        feature_set::FeatureSet,
        fee::FeeStructure,
        hash::Hash,
        inner_instruction::{InnerInstruction, InnerInstructionsList},
        instruction::{CompiledInstruction, InstructionError, TRANSACTION_LEVEL_STACK_HEIGHT},
        loader_v4::{self, LoaderV4State, LoaderV4Status},
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
        collections::{hash_map::Entry, HashMap},
        fmt::{Debug, Formatter},
        rc::Rc,
        sync::{atomic::Ordering, Arc, RwLock},
    },
};

/// A list of log messages emitted during a transaction
pub type TransactionLogMessages = Vec<String>;

pub struct LoadAndExecuteSanitizedTransactionsOutput {
    pub loaded_transactions: Vec<TransactionLoadResult>,
    // Vector of results indicating whether a transaction was executed or could not
    // be executed. Note executed transactions can still have failed!
    pub execution_results: Vec<TransactionExecutionResult>,
}

/// Configuration of the recording capabilities for transaction execution
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

#[derive(Debug)]
enum ProgramAccountLoadResult {
    InvalidAccountData,
    ProgramOfLoaderV1orV2(AccountSharedData),
    ProgramOfLoaderV3(AccountSharedData, AccountSharedData, Slot),
    ProgramOfLoaderV4(AccountSharedData, Slot),
}

#[derive(AbiExample)]
pub struct TransactionBatchProcessor<FG: ForkGraph> {
    /// Bank slot (i.e. block)
    slot: Slot,

    /// Bank epoch
    epoch: Epoch,

    /// initialized from genesis
    epoch_schedule: EpochSchedule,

    /// Transaction fee structure
    fee_structure: FeeStructure,

    /// Optional config parameters that can override runtime behavior
    runtime_config: Arc<RuntimeConfig>,

    /// SysvarCache is a collection of system variables that are
    /// accessible from on chain programs. It is passed to SVM from
    /// client code (e.g. Bank) and forwarded to the MessageProcessor.
    pub sysvar_cache: RwLock<SysvarCache>,

    /// Programs required for transaction batch processing
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

    /// Main entrypoint to the SVM.
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

    /// Returns a hash map of executable program accounts (program accounts that are not writable
    /// in the given transactions), and their owners, for the transactions with a valid
    /// blockhash or nonce.
    fn filter_executable_program_accounts<'a, CB: TransactionProcessingCallback>(
        callbacks: &CB,
        txs: &[SanitizedTransaction],
        check_results: &mut [TransactionCheckResult],
        program_owners: &'a [Pubkey],
    ) -> HashMap<Pubkey, (&'a Pubkey, u64)> {
        let mut result: HashMap<Pubkey, (&'a Pubkey, u64)> = HashMap::new();
        check_results.iter_mut().zip(txs).for_each(|etx| {
            if let ((Ok(()), _nonce, lamports_per_signature), tx) = etx {
                if lamports_per_signature.is_some() {
                    tx.message()
                        .account_keys()
                        .iter()
                        .for_each(|key| match result.entry(*key) {
                            Entry::Occupied(mut entry) => {
                                let (_, count) = entry.get_mut();
                                saturating_add_assign!(*count, 1);
                            }
                            Entry::Vacant(entry) => {
                                if let Some(index) =
                                    callbacks.account_matches_owners(key, program_owners)
                                {
                                    if let Some(owner) = program_owners.get(index) {
                                        entry.insert((owner, 1));
                                    }
                                }
                            }
                        });
                } else {
                    // If the transaction's nonce account was not valid, and blockhash is not found,
                    // the transaction will fail to process. Let's not load any programs from the
                    // transaction, and update the status of the transaction.
                    *etx.0 = (Err(TransactionError::BlockhashNotFound), None, None);
                }
            }
        });
        result
    }

    /// Loads the program with the given pubkey.
    ///
    /// If the account doesn't exist it returns `None`. If the account does exist, it must be a program
    /// account (belong to one of the program loaders). Returns `Some(InvalidAccountData)` if the program
    /// account is `Closed`, contains invalid data or any of the programdata accounts are invalid.
    pub fn load_program_with_pubkey<CB: TransactionProcessingCallback>(
        &self,
        callbacks: &CB,
        pubkey: &Pubkey,
        reload: bool,
        effective_epoch: Epoch,
    ) -> Option<Arc<LoadedProgram>> {
        let program_cache = self.program_cache.read().unwrap();
        let environments = program_cache.get_environments_for_epoch(effective_epoch);
        let mut load_program_metrics = LoadProgramMetrics {
            program_id: pubkey.to_string(),
            ..LoadProgramMetrics::default()
        };

        let mut loaded_program = match self.load_program_accounts(callbacks, pubkey)? {
            ProgramAccountLoadResult::InvalidAccountData => Ok(LoadedProgram::new_tombstone(
                self.slot,
                LoadedProgramType::Closed,
            )),

            ProgramAccountLoadResult::ProgramOfLoaderV1orV2(program_account) => {
                Self::load_program_from_bytes(
                    &mut load_program_metrics,
                    program_account.data(),
                    program_account.owner(),
                    program_account.data().len(),
                    0,
                    environments.program_runtime_v1.clone(),
                    reload,
                )
                .map_err(|_| (0, environments.program_runtime_v1.clone()))
            }

            ProgramAccountLoadResult::ProgramOfLoaderV3(
                program_account,
                programdata_account,
                slot,
            ) => programdata_account
                .data()
                .get(UpgradeableLoaderState::size_of_programdata_metadata()..)
                .ok_or(Box::new(InstructionError::InvalidAccountData).into())
                .and_then(|programdata| {
                    Self::load_program_from_bytes(
                        &mut load_program_metrics,
                        programdata,
                        program_account.owner(),
                        program_account
                            .data()
                            .len()
                            .saturating_add(programdata_account.data().len()),
                        slot,
                        environments.program_runtime_v1.clone(),
                        reload,
                    )
                })
                .map_err(|_| (slot, environments.program_runtime_v1.clone())),

            ProgramAccountLoadResult::ProgramOfLoaderV4(program_account, slot) => program_account
                .data()
                .get(LoaderV4State::program_data_offset()..)
                .ok_or(Box::new(InstructionError::InvalidAccountData).into())
                .and_then(|elf_bytes| {
                    Self::load_program_from_bytes(
                        &mut load_program_metrics,
                        elf_bytes,
                        &loader_v4::id(),
                        program_account.data().len(),
                        slot,
                        environments.program_runtime_v2.clone(),
                        reload,
                    )
                })
                .map_err(|_| (slot, environments.program_runtime_v2.clone())),
        }
        .unwrap_or_else(|(slot, env)| {
            LoadedProgram::new_tombstone(slot, LoadedProgramType::FailedVerification(env))
        });

        let mut timings = ExecuteDetailsTimings::default();
        load_program_metrics.submit_datapoint(&mut timings);
        if !Arc::ptr_eq(
            &environments.program_runtime_v1,
            &program_cache.environments.program_runtime_v1,
        ) || !Arc::ptr_eq(
            &environments.program_runtime_v2,
            &program_cache.environments.program_runtime_v2,
        ) {
            // There can be two entries per program when the environment changes.
            // One for the old environment before the epoch boundary and one for the new environment after the epoch boundary.
            // These two entries have the same deployment slot, so they must differ in their effective slot instead.
            // This is done by setting the effective slot of the entry for the new environment to the epoch boundary.
            loaded_program.effective_slot = loaded_program
                .effective_slot
                .max(self.epoch_schedule.get_first_slot_in_epoch(effective_epoch));
        }
        loaded_program.update_access_slot(self.slot);
        Some(Arc::new(loaded_program))
    }

    fn replenish_program_cache<CB: TransactionProcessingCallback>(
        &self,
        callback: &CB,
        program_accounts_map: &HashMap<Pubkey, (&Pubkey, u64)>,
        limit_to_load_programs: bool,
    ) -> LoadedProgramsForTxBatch {
        let mut missing_programs: Vec<(Pubkey, (LoadedProgramMatchCriteria, u64))> =
            program_accounts_map
                .iter()
                .map(|(pubkey, (_, count))| {
                    (
                        *pubkey,
                        (callback.get_program_match_criteria(pubkey), *count),
                    )
                })
                .collect();

        let mut loaded_programs_for_txs = None;
        let mut program_to_store = None;
        loop {
            let (program_to_load, task_cookie, task_waiter) = {
                // Lock the global cache.
                let mut program_cache = self.program_cache.write().unwrap();
                // Initialize our local cache.
                let is_first_round = loaded_programs_for_txs.is_none();
                if is_first_round {
                    loaded_programs_for_txs = Some(LoadedProgramsForTxBatch::new_from_cache(
                        self.slot,
                        self.epoch,
                        &program_cache,
                    ));
                }
                // Submit our last completed loading task.
                if let Some((key, program)) = program_to_store.take() {
                    if program_cache.finish_cooperative_loading_task(self.slot, key, program)
                        && limit_to_load_programs
                    {
                        // This branch is taken when there is an error in assigning a program to a
                        // cache slot. It is not possible to mock this error for SVM unit
                        // tests purposes.
                        let mut ret = LoadedProgramsForTxBatch::new_from_cache(
                            self.slot,
                            self.epoch,
                            &program_cache,
                        );
                        ret.hit_max_limit = true;
                        return ret;
                    }
                }
                // Figure out which program needs to be loaded next.
                let program_to_load = program_cache.extract(
                    &mut missing_programs,
                    loaded_programs_for_txs.as_mut().unwrap(),
                    is_first_round,
                );
                let task_waiter = Arc::clone(&program_cache.loading_task_waiter);
                (program_to_load, task_waiter.cookie(), task_waiter)
                // Unlock the global cache again.
            };

            if let Some((key, count)) = program_to_load {
                // Load, verify and compile one program.
                let program = self
                    .load_program_with_pubkey(callback, &key, false, self.epoch)
                    .expect("called load_program_with_pubkey() with nonexistent account");
                program.tx_usage_counter.store(count, Ordering::Relaxed);
                program_to_store = Some((key, program));
            } else if missing_programs.is_empty() {
                break;
            } else {
                // Sleep until the next finish_cooperative_loading_task() call.
                // Once a task completes we'll wake up and try to load the
                // missing programs inside the tx batch again.
                let _new_cookie = task_waiter.wait(task_cookie);

                // This branch is not tested in the SVM because it requires concurrent threads.
                // In addition, one of them must be holding the mutex while the other must be
                // trying to lock it.
            }
        }

        loaded_programs_for_txs.unwrap()
    }

    /// Execute a transaction using the provided loaded accounts and update
    /// the executors cache if the transaction was successful.
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
        error_counters: &mut TransactionErrorMetrics,
        log_messages_bytes_limit: Option<usize>,
        programs_loaded_for_tx_batch: &LoadedProgramsForTxBatch,
    ) -> TransactionExecutionResult {
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

        let pre_account_state_info = TransactionAccountStateInfo::new(
            &callback.get_rent_collector().rent,
            &transaction_context,
            tx.message(),
        );

        let log_collector = if recording_config.enable_log_recording {
            match log_messages_bytes_limit {
                None => Some(LogCollector::new_ref()),
                Some(log_messages_bytes_limit) => Some(LogCollector::new_ref_with_limit(Some(
                    log_messages_bytes_limit,
                ))),
            }
        } else {
            None
        };

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
            log_collector.clone(),
            compute_budget,
            programs_loaded_for_tx_batch,
            &mut programs_modified_by_tx,
            callback.get_feature_set(),
            blockhash,
            lamports_per_signature,
        );

        let mut process_message_time = Measure::start("process_message_time");
        let process_result = MessageProcessor::process_message(
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

        let mut status = process_result
            .and_then(|info| {
                let post_account_state_info = TransactionAccountStateInfo::new(
                    &callback.get_rent_collector().rent,
                    &transaction_context,
                    tx.message(),
                );
                TransactionAccountStateInfo::verify_changes(
                    &pre_account_state_info,
                    &post_account_state_info,
                    &transaction_context,
                )
                .map(|_| info)
            })
            .map_err(|err| {
                match err {
                    TransactionError::InvalidRentPayingAccount
                    | TransactionError::InsufficientFundsForRent { .. } => {
                        error_counters.invalid_rent_paying_account += 1;
                    }
                    TransactionError::InvalidAccountIndex => {
                        error_counters.invalid_account_index += 1;
                    }
                    _ => {
                        error_counters.instruction_error += 1;
                    }
                }
                err
            });

        let log_messages: Option<TransactionLogMessages> =
            log_collector.and_then(|log_collector| {
                Rc::try_unwrap(log_collector)
                    .map(|log_collector| log_collector.into_inner().into_messages())
                    .ok()
            });

        let inner_instructions = if recording_config.enable_cpi_recording {
            Some(Self::inner_instructions_list_from_instruction_trace(
                &transaction_context,
            ))
        } else {
            None
        };

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
                log_messages,
                inner_instructions,
                durable_nonce_fee,
                return_data,
                executed_units,
                accounts_data_len_delta,
            },
            programs_modified_by_tx: Box::new(programs_modified_by_tx),
        }
    }

    /// Find the slot in which the program was most recently modified.
    /// Returns slot 0 for programs deployed with v1/v2 loaders, since programs deployed
    /// with those loaders do not retain deployment slot information.
    /// Returns an error if the program's account state can not be found or parsed.
    pub fn program_modification_slot<CB: TransactionProcessingCallback>(
        &self,
        callbacks: &CB,
        pubkey: &Pubkey,
    ) -> transaction::Result<Slot> {
        let program = callbacks
            .get_account_shared_data(pubkey)
            .ok_or(TransactionError::ProgramAccountNotFound)?;
        if bpf_loader_upgradeable::check_id(program.owner()) {
            if let Ok(UpgradeableLoaderState::Program {
                programdata_address,
            }) = program.state()
            {
                let programdata = callbacks
                    .get_account_shared_data(&programdata_address)
                    .ok_or(TransactionError::ProgramAccountNotFound)?;
                if let Ok(UpgradeableLoaderState::ProgramData {
                    slot,
                    upgrade_authority_address: _,
                }) = programdata.state()
                {
                    return Ok(slot);
                }
            }
            Err(TransactionError::ProgramAccountNotFound)
        } else if loader_v4::check_id(program.owner()) {
            let state = solana_loader_v4_program::get_state(program.data())
                .map_err(|_| TransactionError::ProgramAccountNotFound)?;
            Ok(state.slot)
        } else {
            Ok(0)
        }
    }

    fn load_program_from_bytes(
        load_program_metrics: &mut LoadProgramMetrics,
        programdata: &[u8],
        loader_key: &Pubkey,
        account_size: usize,
        deployment_slot: Slot,
        program_runtime_environment: ProgramRuntimeEnvironment,
        reloading: bool,
    ) -> std::result::Result<LoadedProgram, Box<dyn std::error::Error>> {
        if reloading {
            // Safety: this is safe because the program is being reloaded in the cache.
            unsafe {
                LoadedProgram::reload(
                    loader_key,
                    program_runtime_environment.clone(),
                    deployment_slot,
                    deployment_slot.saturating_add(DELAY_VISIBILITY_SLOT_OFFSET),
                    programdata,
                    account_size,
                    load_program_metrics,
                )
            }
        } else {
            LoadedProgram::new(
                loader_key,
                program_runtime_environment.clone(),
                deployment_slot,
                deployment_slot.saturating_add(DELAY_VISIBILITY_SLOT_OFFSET),
                programdata,
                account_size,
                load_program_metrics,
            )
        }
    }

    fn load_program_accounts<CB: TransactionProcessingCallback>(
        &self,
        callbacks: &CB,
        pubkey: &Pubkey,
    ) -> Option<ProgramAccountLoadResult> {
        let program_account = callbacks.get_account_shared_data(pubkey)?;

        if loader_v4::check_id(program_account.owner()) {
            return Some(
                solana_loader_v4_program::get_state(program_account.data())
                    .ok()
                    .and_then(|state| {
                        (!matches!(state.status, LoaderV4Status::Retracted)).then_some(state.slot)
                    })
                    .map(|slot| ProgramAccountLoadResult::ProgramOfLoaderV4(program_account, slot))
                    .unwrap_or(ProgramAccountLoadResult::InvalidAccountData),
            );
        }

        if !bpf_loader_upgradeable::check_id(program_account.owner()) {
            return Some(ProgramAccountLoadResult::ProgramOfLoaderV1orV2(
                program_account,
            ));
        }

        if let Ok(UpgradeableLoaderState::Program {
            programdata_address,
        }) = program_account.state()
        {
            if let Some(programdata_account) =
                callbacks.get_account_shared_data(&programdata_address)
            {
                if let Ok(UpgradeableLoaderState::ProgramData {
                    slot,
                    upgrade_authority_address: _,
                }) = programdata_account.state()
                {
                    return Some(ProgramAccountLoadResult::ProgramOfLoaderV3(
                        program_account,
                        programdata_account,
                        slot,
                    ));
                }
            }
        }
        Some(ProgramAccountLoadResult::InvalidAccountData)
    }

    /// Extract the InnerInstructionsList from a TransactionContext
    fn inner_instructions_list_from_instruction_trace(
        transaction_context: &TransactionContext,
    ) -> InnerInstructionsList {
        debug_assert!(transaction_context
            .get_instruction_context_at_index_in_trace(0)
            .map(|instruction_context| instruction_context.get_stack_height()
                == TRANSACTION_LEVEL_STACK_HEIGHT)
            .unwrap_or(true));
        let mut outer_instructions = Vec::new();
        for index_in_trace in 0..transaction_context.get_instruction_trace_length() {
            if let Ok(instruction_context) =
                transaction_context.get_instruction_context_at_index_in_trace(index_in_trace)
            {
                let stack_height = instruction_context.get_stack_height();
                if stack_height == TRANSACTION_LEVEL_STACK_HEIGHT {
                    outer_instructions.push(Vec::new());
                } else if let Some(inner_instructions) = outer_instructions.last_mut() {
                    let stack_height = u8::try_from(stack_height).unwrap_or(u8::MAX);
                    let instruction = CompiledInstruction::new_from_raw_parts(
                        instruction_context
                            .get_index_of_program_account_in_transaction(
                                instruction_context
                                    .get_number_of_program_accounts()
                                    .saturating_sub(1),
                            )
                            .unwrap_or_default() as u8,
                        instruction_context.get_instruction_data().to_vec(),
                        (0..instruction_context.get_number_of_instruction_accounts())
                            .map(|instruction_account_index| {
                                instruction_context
                                    .get_index_of_instruction_account_in_transaction(
                                        instruction_account_index,
                                    )
                                    .unwrap_or_default() as u8
                            })
                            .collect(),
                    );
                    inner_instructions.push(InnerInstruction {
                        instruction,
                        stack_height,
                    });
                } else {
                    debug_assert!(false);
                }
            } else {
                debug_assert!(false);
            }
        }
        outer_instructions
    }

    pub fn fill_missing_sysvar_cache_entries<CB: TransactionProcessingCallback>(
        &self,
        callbacks: &CB,
    ) {
        let mut sysvar_cache = self.sysvar_cache.write().unwrap();
        sysvar_cache.fill_missing_entries(|pubkey, set_sysvar| {
            if let Some(account) = callbacks.get_account_shared_data(pubkey) {
                set_sysvar(account.data());
            }
        });
    }

    pub fn reset_sysvar_cache(&self) {
        let mut sysvar_cache = self.sysvar_cache.write().unwrap();
        sysvar_cache.reset();
    }

    pub fn get_sysvar_cache_for_tests(&self) -> SysvarCache {
        self.sysvar_cache.read().unwrap().clone()
    }
}
