use {
    crate::{
        invoke_context::{BuiltinFunctionWithContext, InvokeContext},
        timings::ExecuteDetailsTimings,
    },
    log::{debug, log_enabled, trace},
    solana_measure::measure::Measure,
    solana_rbpf::{
        elf::Executable,
        program::{BuiltinProgram, FunctionRegistry},
        verifier::RequisiteVerifier,
        vm::Config,
    },
    solana_sdk::{
        bpf_loader, bpf_loader_deprecated, bpf_loader_upgradeable,
        clock::{Epoch, Slot},
        loader_v4,
        pubkey::Pubkey,
        saturating_add_assign,
    },
    std::{
        collections::HashMap,
        fmt::{Debug, Formatter},
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Condvar, Mutex, RwLock,
        },
    },
};

pub type ProgramRuntimeEnvironment = Arc<BuiltinProgram<InvokeContext<'static>>>;
pub const MAX_LOADED_ENTRY_COUNT: usize = 256;
pub const DELAY_VISIBILITY_SLOT_OFFSET: Slot = 1;

/// Relationship between two fork IDs
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockRelation {
    /// The slot is on the same fork and is an ancestor of the other slot
    Ancestor,
    /// The two slots are equal and are on the same fork
    Equal,
    /// The slot is on the same fork and is a descendant of the other slot
    Descendant,
    /// The slots are on two different forks and may have had a common ancestor at some point
    Unrelated,
    /// Either one or both of the slots are either older than the latest root, or are in future
    Unknown,
}

/// Maps relationship between two slots.
pub trait ForkGraph {
    /// Returns the BlockRelation of A to B
    fn relationship(&self, a: Slot, b: Slot) -> BlockRelation;

    /// Returns the epoch of the given slot
    fn slot_epoch(&self, _slot: Slot) -> Option<Epoch> {
        Some(0)
    }
}

/// Actual payload of [LoadedProgram].
#[derive(Default)]
pub enum LoadedProgramType {
    /// Tombstone for programs which currently do not pass the verifier but could if the feature set changed.
    FailedVerification(ProgramRuntimeEnvironment),
    /// Tombstone for programs that were either explicitly closed or never deployed.
    ///
    /// It's also used for accounts belonging to program loaders, that don't actually contain program code (e.g. buffer accounts for LoaderV3 programs).
    #[default]
    Closed,
    /// Tombstone for programs which have recently been modified but the new version is not visible yet.
    DelayVisibility,
    /// Successfully verified but not currently compiled.
    ///
    /// It continues to track usage statistics even when the compiled executable of the program is evicted from memory.
    Unloaded(ProgramRuntimeEnvironment),
    /// Verified and compiled program of loader-v1 or loader-v2
    LegacyV0(Executable<InvokeContext<'static>>),
    /// Verified and compiled program of loader-v3 (aka upgradable loader)
    LegacyV1(Executable<InvokeContext<'static>>),
    /// Verified and compiled program of loader-v4
    Typed(Executable<InvokeContext<'static>>),
    #[cfg(test)]
    TestLoaded(ProgramRuntimeEnvironment),
    /// A built-in program which is not stored on-chain but backed into and distributed with the validator
    Builtin(BuiltinProgram<InvokeContext<'static>>),
}

impl Debug for LoadedProgramType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadedProgramType::FailedVerification(_) => {
                write!(f, "LoadedProgramType::FailedVerification")
            }
            LoadedProgramType::Closed => write!(f, "LoadedProgramType::Closed"),
            LoadedProgramType::DelayVisibility => write!(f, "LoadedProgramType::DelayVisibility"),
            LoadedProgramType::Unloaded(_) => write!(f, "LoadedProgramType::Unloaded"),
            LoadedProgramType::LegacyV0(_) => write!(f, "LoadedProgramType::LegacyV0"),
            LoadedProgramType::LegacyV1(_) => write!(f, "LoadedProgramType::LegacyV1"),
            LoadedProgramType::Typed(_) => write!(f, "LoadedProgramType::Typed"),
            #[cfg(test)]
            LoadedProgramType::TestLoaded(_) => write!(f, "LoadedProgramType::TestLoaded"),
            LoadedProgramType::Builtin(_) => write!(f, "LoadedProgramType::Builtin"),
        }
    }
}

impl LoadedProgramType {
    /// Returns a reference to its environment if it has one
    pub fn get_environment(&self) -> Option<&ProgramRuntimeEnvironment> {
        match self {
            LoadedProgramType::LegacyV0(program)
            | LoadedProgramType::LegacyV1(program)
            | LoadedProgramType::Typed(program) => Some(program.get_loader()),
            LoadedProgramType::FailedVerification(env) | LoadedProgramType::Unloaded(env) => {
                Some(env)
            }
            #[cfg(test)]
            LoadedProgramType::TestLoaded(environment) => Some(environment),
            _ => None,
        }
    }
}

/// Holds a program version at a specific address and on a specific slot / fork.
///
/// It contains the actual program in [LoadedProgramType] and a bunch of meta-data.
#[derive(Debug, Default)]
pub struct LoadedProgram {
    /// The program of this entry
    pub program: LoadedProgramType,
    /// Size of account that stores the program and program data
    pub account_size: usize,
    /// Slot in which the program was (re)deployed
    pub deployment_slot: Slot,
    /// Slot in which this entry will become active (can be in the future)
    pub effective_slot: Slot,
    /// How often this entry was used by a transaction
    pub tx_usage_counter: AtomicU64,
    /// How often this entry was used by an instruction
    pub ix_usage_counter: AtomicU64,
    /// Latest slot in which the entry was used
    pub latest_access_slot: AtomicU64,
}

/// Global cache statistics for [ProgramCache].
#[derive(Debug, Default)]
pub struct Stats {
    /// a program was already in the cache
    pub hits: AtomicU64,
    /// a program was not found and loaded instead
    pub misses: AtomicU64,
    /// a compiled executable was unloaded
    pub evictions: HashMap<Pubkey, u64>,
    /// an unloaded program was loaded again (opposite of eviction)
    pub reloads: AtomicU64,
    /// a program was loaded or un/re/deployed
    pub insertions: AtomicU64,
    /// a program was loaded but can not be extracted on its own fork anymore
    pub lost_insertions: AtomicU64,
    /// a program which was already in the cache was reloaded by mistake
    pub replacements: AtomicU64,
    /// a program was only used once before being unloaded
    pub one_hit_wonders: AtomicU64,
    /// a program became unreachable in the fork graph because of rerooting
    pub prunes_orphan: AtomicU64,
    /// a program got pruned because it was not recompiled for the next epoch
    pub prunes_environment: AtomicU64,
    /// the [SecondLevel] was empty because all slot versions got pruned
    pub empty_entries: AtomicU64,
}

impl Stats {
    /// Logs the measurement values
    pub fn submit(&self, slot: Slot) {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let evictions: u64 = self.evictions.values().sum();
        let reloads = self.reloads.load(Ordering::Relaxed);
        let insertions = self.insertions.load(Ordering::Relaxed);
        let lost_insertions = self.lost_insertions.load(Ordering::Relaxed);
        let replacements = self.replacements.load(Ordering::Relaxed);
        let one_hit_wonders = self.one_hit_wonders.load(Ordering::Relaxed);
        let prunes_orphan = self.prunes_orphan.load(Ordering::Relaxed);
        let prunes_environment = self.prunes_environment.load(Ordering::Relaxed);
        let empty_entries = self.empty_entries.load(Ordering::Relaxed);
        datapoint_info!(
            "loaded-programs-cache-stats",
            ("slot", slot, i64),
            ("hits", hits, i64),
            ("misses", misses, i64),
            ("evictions", evictions, i64),
            ("reloads", reloads, i64),
            ("insertions", insertions, i64),
            ("lost_insertions", lost_insertions, i64),
            ("replace_entry", replacements, i64),
            ("one_hit_wonders", one_hit_wonders, i64),
            ("prunes_orphan", prunes_orphan, i64),
            ("prunes_environment", prunes_environment, i64),
            ("empty_entries", empty_entries, i64),
        );
        debug!(
            "Loaded Programs Cache Stats -- Hits: {}, Misses: {}, Evictions: {}, Reloads: {}, Insertions: {} Lost-Insertions: {}, Replacements: {}, One-Hit-Wonders: {}, Prunes-Orphan: {}, Prunes-Environment: {}, Empty: {}",
            hits, misses, evictions, reloads, insertions, lost_insertions, replacements, one_hit_wonders, prunes_orphan, prunes_environment, empty_entries
        );
        if log_enabled!(log::Level::Trace) && !self.evictions.is_empty() {
            let mut evictions = self.evictions.iter().collect::<Vec<_>>();
            evictions.sort_by_key(|e| e.1);
            let evictions = evictions
                .into_iter()
                .rev()
                .map(|(program_id, evictions)| {
                    format!("  {:<44}  {}", program_id.to_string(), evictions)
                })
                .collect::<Vec<_>>();
            let evictions = evictions.join("\n");
            trace!(
                "Eviction Details:\n  {:<44}  {}\n{}",
                "Program",
                "Count",
                evictions
            );
        }
    }

    pub fn reset(&mut self) {
        *self = Stats::default();
    }
}

/// Time measurements for loading a single [LoadedProgram].
#[derive(Debug, Default)]
pub struct LoadProgramMetrics {
    /// Program address, but as text
    pub program_id: String,
    /// Microseconds it took to `create_program_runtime_environment`
    pub register_syscalls_us: u64,
    /// Microseconds it took to `Executable::<InvokeContext>::load`
    pub load_elf_us: u64,
    /// Microseconds it took to `executable.verify::<RequisiteVerifier>`
    pub verify_code_us: u64,
    /// Microseconds it took to `executable.jit_compile`
    pub jit_compile_us: u64,
}

impl LoadProgramMetrics {
    pub fn submit_datapoint(&self, timings: &mut ExecuteDetailsTimings) {
        saturating_add_assign!(
            timings.create_executor_register_syscalls_us,
            self.register_syscalls_us
        );
        saturating_add_assign!(timings.create_executor_load_elf_us, self.load_elf_us);
        saturating_add_assign!(timings.create_executor_verify_code_us, self.verify_code_us);
        saturating_add_assign!(timings.create_executor_jit_compile_us, self.jit_compile_us);
        datapoint_trace!(
            "create_executor_trace",
            ("program_id", self.program_id, String),
            ("register_syscalls_us", self.register_syscalls_us, i64),
            ("load_elf_us", self.load_elf_us, i64),
            ("verify_code_us", self.verify_code_us, i64),
            ("jit_compile_us", self.jit_compile_us, i64),
        );
    }
}

impl PartialEq for LoadedProgram {
    fn eq(&self, other: &Self) -> bool {
        self.effective_slot == other.effective_slot
            && self.deployment_slot == other.deployment_slot
            && self.is_tombstone() == other.is_tombstone()
    }
}

impl LoadedProgram {
    /// Creates a new user program
    pub fn new(
        loader_key: &Pubkey,
        program_runtime_environment: ProgramRuntimeEnvironment,
        deployment_slot: Slot,
        effective_slot: Slot,
        elf_bytes: &[u8],
        account_size: usize,
        metrics: &mut LoadProgramMetrics,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::new_internal(
            loader_key,
            program_runtime_environment,
            deployment_slot,
            effective_slot,
            elf_bytes,
            account_size,
            metrics,
            false, /* reloading */
        )
    }

    /// Reloads a user program, *without* running the verifier.
    ///
    /// # Safety
    ///
    /// This method is unsafe since it assumes that the program has already been verified. Should
    /// only be called when the program was previously verified and loaded in the cache, but was
    /// unloaded due to inactivity. It should also be checked that the `program_runtime_environment`
    /// hasn't changed since it was unloaded.
    pub unsafe fn reload(
        loader_key: &Pubkey,
        program_runtime_environment: Arc<BuiltinProgram<InvokeContext<'static>>>,
        deployment_slot: Slot,
        effective_slot: Slot,
        elf_bytes: &[u8],
        account_size: usize,
        metrics: &mut LoadProgramMetrics,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::new_internal(
            loader_key,
            program_runtime_environment,
            deployment_slot,
            effective_slot,
            elf_bytes,
            account_size,
            metrics,
            true, /* reloading */
        )
    }

    fn new_internal(
        loader_key: &Pubkey,
        program_runtime_environment: Arc<BuiltinProgram<InvokeContext<'static>>>,
        deployment_slot: Slot,
        effective_slot: Slot,
        elf_bytes: &[u8],
        account_size: usize,
        metrics: &mut LoadProgramMetrics,
        reloading: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let load_elf_time = Measure::start("load_elf_time");
        // The following unused_mut exception is needed for architectures that do not
        // support JIT compilation.
        #[allow(unused_mut)]
        let mut executable = Executable::load(elf_bytes, program_runtime_environment.clone())?;
        metrics.load_elf_us = load_elf_time.end_as_us();

        if !reloading {
            let verify_code_time = Measure::start("verify_code_time");
            executable.verify::<RequisiteVerifier>()?;
            metrics.verify_code_us = verify_code_time.end_as_us();
        }

        #[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
        {
            let jit_compile_time = Measure::start("jit_compile_time");
            executable.jit_compile()?;
            metrics.jit_compile_us = jit_compile_time.end_as_us();
        }

        let program = if bpf_loader_deprecated::check_id(loader_key) {
            LoadedProgramType::LegacyV0(executable)
        } else if bpf_loader::check_id(loader_key) || bpf_loader_upgradeable::check_id(loader_key) {
            LoadedProgramType::LegacyV1(executable)
        } else if loader_v4::check_id(loader_key) {
            LoadedProgramType::Typed(executable)
        } else {
            panic!();
        };

        Ok(Self {
            deployment_slot,
            account_size,
            effective_slot,
            tx_usage_counter: AtomicU64::new(0),
            program,
            ix_usage_counter: AtomicU64::new(0),
            latest_access_slot: AtomicU64::new(0),
        })
    }

    pub fn to_unloaded(&self) -> Option<Self> {
        match &self.program {
            LoadedProgramType::LegacyV0(_)
            | LoadedProgramType::LegacyV1(_)
            | LoadedProgramType::Typed(_) => {}
            #[cfg(test)]
            LoadedProgramType::TestLoaded(_) => {}
            LoadedProgramType::FailedVerification(_)
            | LoadedProgramType::Closed
            | LoadedProgramType::DelayVisibility
            | LoadedProgramType::Unloaded(_)
            | LoadedProgramType::Builtin(_) => {
                return None;
            }
        }
        Some(Self {
            program: LoadedProgramType::Unloaded(self.program.get_environment()?.clone()),
            account_size: self.account_size,
            deployment_slot: self.deployment_slot,
            effective_slot: self.effective_slot,
            tx_usage_counter: AtomicU64::new(self.tx_usage_counter.load(Ordering::Relaxed)),
            ix_usage_counter: AtomicU64::new(self.ix_usage_counter.load(Ordering::Relaxed)),
            latest_access_slot: AtomicU64::new(self.latest_access_slot.load(Ordering::Relaxed)),
        })
    }

    /// Creates a new built-in program
    pub fn new_builtin(
        deployment_slot: Slot,
        account_size: usize,
        builtin_function: BuiltinFunctionWithContext,
    ) -> Self {
        let mut function_registry = FunctionRegistry::default();
        function_registry
            .register_function_hashed(*b"entrypoint", builtin_function)
            .unwrap();
        Self {
            deployment_slot,
            account_size,
            effective_slot: deployment_slot,
            tx_usage_counter: AtomicU64::new(0),
            program: LoadedProgramType::Builtin(BuiltinProgram::new_builtin(function_registry)),
            ix_usage_counter: AtomicU64::new(0),
            latest_access_slot: AtomicU64::new(0),
        }
    }

    pub fn new_tombstone(slot: Slot, reason: LoadedProgramType) -> Self {
        let tombstone = Self {
            program: reason,
            account_size: 0,
            deployment_slot: slot,
            effective_slot: slot,
            tx_usage_counter: AtomicU64::default(),
            ix_usage_counter: AtomicU64::default(),
            latest_access_slot: AtomicU64::new(0),
        };
        debug_assert!(tombstone.is_tombstone());
        tombstone
    }

    pub fn is_tombstone(&self) -> bool {
        matches!(
            self.program,
            LoadedProgramType::FailedVerification(_)
                | LoadedProgramType::Closed
                | LoadedProgramType::DelayVisibility
        )
    }

    fn is_implicit_delay_visibility_tombstone(&self, slot: Slot) -> bool {
        !matches!(self.program, LoadedProgramType::Builtin(_))
            && self.effective_slot.saturating_sub(self.deployment_slot)
                == DELAY_VISIBILITY_SLOT_OFFSET
            && slot >= self.deployment_slot
            && slot < self.effective_slot
    }

    pub fn update_access_slot(&self, slot: Slot) {
        let _ = self.latest_access_slot.fetch_max(slot, Ordering::Relaxed);
    }

    pub fn decayed_usage_counter(&self, now: Slot) -> u64 {
        let last_access = self.latest_access_slot.load(Ordering::Relaxed);
        // Shifting the u64 value for more than 63 will cause an overflow.
        let decaying_for = std::cmp::min(63, now.saturating_sub(last_access));
        self.tx_usage_counter.load(Ordering::Relaxed) >> decaying_for
    }
}

/// Globally shared RBPF config and syscall registry
///
/// This is only valid in an epoch range as long as no feature affecting RBPF is activated.
#[derive(Clone, Debug)]
pub struct ProgramRuntimeEnvironments {
    /// For program runtime V1
    pub program_runtime_v1: ProgramRuntimeEnvironment,
    /// For program runtime V2
    pub program_runtime_v2: ProgramRuntimeEnvironment,
}

impl Default for ProgramRuntimeEnvironments {
    fn default() -> Self {
        let empty_loader = Arc::new(BuiltinProgram::new_loader(
            Config::default(),
            FunctionRegistry::default(),
        ));
        Self {
            program_runtime_v1: empty_loader.clone(),
            program_runtime_v2: empty_loader,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct LoadingTaskCookie(u64);

impl LoadingTaskCookie {
    fn new() -> Self {
        Self(0)
    }

    fn update(&mut self) {
        let LoadingTaskCookie(cookie) = self;
        *cookie = cookie.wrapping_add(1);
    }
}

/// Suspends the thread in case no cooprative loading task was assigned
#[derive(Debug, Default)]
pub struct LoadingTaskWaiter {
    cookie: Mutex<LoadingTaskCookie>,
    cond: Condvar,
}

impl LoadingTaskWaiter {
    pub fn new() -> Self {
        Self {
            cookie: Mutex::new(LoadingTaskCookie::new()),
            cond: Condvar::new(),
        }
    }

    pub fn cookie(&self) -> LoadingTaskCookie {
        *self.cookie.lock().unwrap()
    }

    pub fn notify(&self) {
        let mut cookie = self.cookie.lock().unwrap();
        cookie.update();
        self.cond.notify_all();
    }

    pub fn wait(&self, cookie: LoadingTaskCookie) -> LoadingTaskCookie {
        let cookie_guard = self.cookie.lock().unwrap();
        *self
            .cond
            .wait_while(cookie_guard, |current_cookie| *current_cookie == cookie)
            .unwrap()
    }
}

#[derive(Debug, Default)]
pub struct SecondLevel {
    pub slot_versions: Vec<Arc<LoadedProgram>>,
    pub cooperative_loading_lock: Option<(Slot, std::thread::ThreadId)>,
}
pub struct ProgramCache<FG: ForkGraph> {
    entries: HashMap<Pubkey, SecondLevel>,
    pub latest_root_slot: Slot,
    pub latest_root_epoch: Epoch,
    pub environments: ProgramRuntimeEnvironments,
    pub upcoming_environments: Option<ProgramRuntimeEnvironments>,
    pub programs_to_recompile: Vec<(Pubkey, Arc<LoadedProgram>)>,
    pub stats: Stats,
    pub fork_graph: Option<Arc<RwLock<FG>>>,
    pub loading_task_waiter: Arc<LoadingTaskWaiter>,
}

impl<FG: ForkGraph> Debug for ProgramCache<FG> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramCache")
            .field("root slot", &self.latest_root_slot)
            .field("root epoch", &self.latest_root_epoch)
            .field("stats", &self.stats)
            .field("cache", &self.entries)
            .finish()
    }
}

#[derive(Clone, Debug, Default)]
pub struct LoadedProgramsForTxBatch {
    entries: HashMap<Pubkey, Arc<LoadedProgram>>,
    slot: Slot,
    pub environments: ProgramRuntimeEnvironments,
    pub upcoming_environments: Option<ProgramRuntimeEnvironments>,
    pub latest_root_epoch: Epoch,
    pub hit_max_limit: bool,
}

impl LoadedProgramsForTxBatch {
    pub fn new(
        slot: Slot,
        environments: ProgramRuntimeEnvironments,
        upcoming_environments: Option<ProgramRuntimeEnvironments>,
        latest_root_epoch: Epoch,
    ) -> Self {
        Self {
            entries: HashMap::new(),
            slot,
            environments,
            upcoming_environments,
            latest_root_epoch,
            hit_max_limit: false,
        }
    }

    /// Returns the current environments depending on the given epoch
    pub fn get_environments_for_epoch(&self, epoch: Epoch) -> &ProgramRuntimeEnvironments {
        if epoch != self.latest_root_epoch {
            if let Some(upcoming_environments) = self.upcoming_environments.as_ref() {
                return upcoming_environments;
            }
        }
        &self.environments
    }

    pub fn replenish(
        &mut self,
        key: Pubkey,
        entry: Arc<LoadedProgram>,
    ) -> (bool, Arc<LoadedProgram>) {
        (self.entries.insert(key, entry.clone()).is_some(), entry)
    }

    pub fn find(&self, key: &Pubkey) -> Option<Arc<LoadedProgram>> {
        self.entries.get(key).map(|entry| {
            if entry.is_implicit_delay_visibility_tombstone(self.slot) {
                // Found a program entry on the current fork, but it's not effective
                // yet. It indicates that the program has delayed visibility. Return
                // the tombstone to reflect that.
                Arc::new(LoadedProgram::new_tombstone(
                    entry.deployment_slot,
                    LoadedProgramType::DelayVisibility,
                ))
            } else {
                entry.clone()
            }
        })
    }

    pub fn slot(&self) -> Slot {
        self.slot
    }

    pub fn set_slot_for_tests(&mut self, slot: Slot) {
        self.slot = slot;
    }

    pub fn merge(&mut self, other: &Self) {
        other.entries.iter().for_each(|(key, entry)| {
            self.replenish(*key, entry.clone());
        })
    }
}

pub enum LoadedProgramMatchCriteria {
    DeployedOnOrAfterSlot(Slot),
    Tombstone,
    NoCriteria,
}

impl<FG: ForkGraph> ProgramCache<FG> {
    pub fn new(root_slot: Slot, root_epoch: Epoch) -> Self {
        Self {
            entries: HashMap::new(),
            latest_root_slot: root_slot,
            latest_root_epoch: root_epoch,
            environments: ProgramRuntimeEnvironments::default(),
            upcoming_environments: None,
            programs_to_recompile: Vec::default(),
            stats: Stats::default(),
            fork_graph: None,
            loading_task_waiter: Arc::new(LoadingTaskWaiter::default()),
        }
    }

    pub fn assign_program(&mut self, _key: Pubkey, _entry: Arc<LoadedProgram>) -> bool {
        /*
         * Function simplified for brevity.
         */
        false
    }

    pub fn extract(
        &mut self,
        _search_for: &mut Vec<(Pubkey, (LoadedProgramMatchCriteria, u64))>,
        _loaded_programs_for_tx_batch: &mut LoadedProgramsForTxBatch,
        _is_first_round: bool,
    ) -> Option<(Pubkey, u64)> {
        /*
         * Function simplified for brevity.
         */
        None
    }

    pub fn finish_cooperative_loading_task(
        &mut self,
        _slot: Slot,
        _key: Pubkey,
        _loaded_program: Arc<LoadedProgram>,
    ) -> bool {
        /*
         * Function simplified for brevity.
         */
        false
    }

    pub fn merge(&mut self, tx_batch_cache: &LoadedProgramsForTxBatch) {
        tx_batch_cache.entries.iter().for_each(|(key, entry)| {
            self.assign_program(*key, entry.clone());
        })
    }

    pub fn remove_programs(&mut self, keys: impl Iterator<Item = Pubkey>) {
        for k in keys {
            self.entries.remove(&k);
        }
    }
}
