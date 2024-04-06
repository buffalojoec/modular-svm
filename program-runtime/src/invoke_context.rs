use {
    crate::{
        compute_budget::ComputeBudget,
        loaded_programs::{LoadedProgram, LoadedProgramsForTxBatch, ProgramRuntimeEnvironments},
        log_collector::LogCollector,
        sysvar_cache::SysvarCache,
        timings::{ExecuteDetailsTimings, ExecuteTimings},
    },
    solana_rbpf::{ebpf::MM_HEAP_START, program::BuiltinFunction, vm::ContextObject},
    solana_sdk::{
        clock::Slot,
        feature_set::FeatureSet,
        hash::Hash,
        instruction::InstructionError,
        pubkey::Pubkey,
        transaction_context::{IndexOfAccount, InstructionAccount, TransactionContext},
    },
    std::{
        alloc::Layout,
        cell::RefCell,
        fmt::{self, Debug},
        rc::Rc,
        sync::Arc,
    },
};

pub type BuiltinFunctionWithContext = BuiltinFunction<InvokeContext<'static>>;

impl<'a> ContextObject for InvokeContext<'a> {
    fn trace(&mut self, state: [u64; 12]) {
        self.syscall_context
            .last_mut()
            .unwrap()
            .as_mut()
            .unwrap()
            .trace_log
            .push(state);
    }

    fn consume(&mut self, amount: u64) {
        let mut compute_meter = self.compute_meter.borrow_mut();
        *compute_meter = compute_meter.saturating_sub(amount);
    }

    fn get_remaining(&self) -> u64 {
        *self.compute_meter.borrow()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AllocErr;
impl fmt::Display for AllocErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Error: Memory allocation failed")
    }
}

pub struct BpfAllocator {
    len: u64,
    pos: u64,
}

impl BpfAllocator {
    pub fn new(len: u64) -> Self {
        Self { len, pos: 0 }
    }

    pub fn alloc(&mut self, layout: Layout) -> Result<u64, AllocErr> {
        let bytes_to_align = (self.pos as *const u8).align_offset(layout.align()) as u64;
        if self
            .pos
            .saturating_add(bytes_to_align)
            .saturating_add(layout.size() as u64)
            <= self.len
        {
            self.pos = self.pos.saturating_add(bytes_to_align);
            let addr = MM_HEAP_START.saturating_add(self.pos);
            self.pos = self.pos.saturating_add(layout.size() as u64);
            Ok(addr)
        } else {
            Err(AllocErr)
        }
    }
}

pub struct SyscallContext {
    pub allocator: BpfAllocator,
    pub accounts_metadata: Vec<SerializedAccountMetadata>,
    pub trace_log: Vec<[u64; 12]>,
}

#[derive(Debug, Clone)]
pub struct SerializedAccountMetadata {
    pub original_data_len: usize,
    pub vm_data_addr: u64,
    pub vm_key_addr: u64,
    pub vm_lamports_addr: u64,
    pub vm_owner_addr: u64,
}

pub struct InvokeContext<'a> {
    pub transaction_context: &'a mut TransactionContext,
    pub sysvar_cache: &'a SysvarCache,
    pub log_collector: Option<Rc<RefCell<LogCollector>>>,
    pub compute_budget: ComputeBudget,
    pub current_compute_budget: ComputeBudget,
    pub compute_meter: RefCell<u64>,
    pub programs_loaded_for_tx_batch: &'a LoadedProgramsForTxBatch,
    pub programs_modified_by_tx: &'a mut LoadedProgramsForTxBatch,
    pub feature_set: Arc<FeatureSet>,
    pub timings: ExecuteDetailsTimings,
    pub blockhash: Hash,
    pub lamports_per_signature: u64,
    pub syscall_context: Vec<Option<SyscallContext>>,
    pub traces: Vec<Vec<[u64; 12]>>,
}

impl<'a> InvokeContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        transaction_context: &'a mut TransactionContext,
        sysvar_cache: &'a SysvarCache,
        log_collector: Option<Rc<RefCell<LogCollector>>>,
        compute_budget: ComputeBudget,
        programs_loaded_for_tx_batch: &'a LoadedProgramsForTxBatch,
        programs_modified_by_tx: &'a mut LoadedProgramsForTxBatch,
        feature_set: Arc<FeatureSet>,
        blockhash: Hash,
        lamports_per_signature: u64,
    ) -> Self {
        Self {
            transaction_context,
            sysvar_cache,
            log_collector,
            current_compute_budget: compute_budget,
            compute_budget,
            compute_meter: RefCell::new(compute_budget.compute_unit_limit),
            programs_loaded_for_tx_batch,
            programs_modified_by_tx,
            feature_set,
            timings: ExecuteDetailsTimings::default(),
            blockhash,
            lamports_per_signature,
            syscall_context: Vec::new(),
            traces: Vec::new(),
        }
    }

    pub fn find_program_in_cache(&self, pubkey: &Pubkey) -> Option<Arc<LoadedProgram>> {
        self.programs_modified_by_tx
            .find(pubkey)
            .or_else(|| self.programs_loaded_for_tx_batch.find(pubkey))
    }

    pub fn get_environments_for_slot(
        &self,
        _effective_slot: Slot,
    ) -> Result<&ProgramRuntimeEnvironments, InstructionError> {
        /*
         * Function simplified for brevity.
         */
        Ok(self
            .programs_loaded_for_tx_batch
            .get_environments_for_epoch(0))
    }

    pub fn push(&mut self) -> Result<(), InstructionError> {
        /*
         * Function simplified for brevity.
         */
        Ok(())
    }

    pub fn pop(&mut self) -> Result<(), InstructionError> {
        /*
         * Function simplified for brevity.
         */
        Ok(())
    }

    pub fn get_stack_height(&self) -> usize {
        self.transaction_context
            .get_instruction_context_stack_height()
    }

    pub fn process_instruction(
        &mut self,
        instruction_data: &[u8],
        instruction_accounts: &[InstructionAccount],
        program_indices: &[IndexOfAccount],
        compute_units_consumed: &mut u64,
        timings: &mut ExecuteTimings,
    ) -> Result<(), InstructionError> {
        *compute_units_consumed = 0;
        self.transaction_context
            .get_next_instruction_context()?
            .configure(program_indices, instruction_accounts, instruction_data);
        self.push()?;
        self.process_executable_chain(compute_units_consumed, timings)
            .and(self.pop())
    }

    fn process_executable_chain(
        &mut self,
        _compute_units_consumed: &mut u64,
        _timings: &mut ExecuteTimings,
    ) -> Result<(), InstructionError> {
        /*
         * Function simplified for brevity.
         */
        Ok(())
    }
}
