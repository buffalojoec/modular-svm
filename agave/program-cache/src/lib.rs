//! Agave Program Cache.

use {
    solana_sdk::{
        clock::{Epoch, Slot},
        pubkey::Pubkey,
    },
    std::{
        collections::HashMap,
        sync::{atomic::AtomicU64, Arc, Condvar, Mutex, RwLock},
    },
};

pub enum BlockRelation {
    Ancestor,
    Equal,
    Descendant,
    Unrelated,
    Unknown,
}

pub trait ForkGraph {
    fn relationship(&self, a: Slot, b: Slot) -> BlockRelation;

    fn slot_epoch(&self, _slot: Slot) -> Option<Epoch> {
        Some(0)
    }
}

pub enum LoadedProgramType {
    // FailedVerification(ProgramRuntimeEnvironment),
    Closed,
    DelayVisibility,
    // Unloaded(ProgramRuntimeEnvironment),
    // LegacyV0(Executable<InvokeContext<'static>>),
    // LegacyV1(Executable<InvokeContext<'static>>),
    // Typed(Executable<InvokeContext<'static>>),
    // Builtin(BuiltinProgram<InvokeContext<'static>>),
}

pub struct LoadedProgram {
    pub program: LoadedProgramType,
    pub account_size: usize,
    pub deployment_slot: Slot,
    pub effective_slot: Slot,
    pub tx_usage_counter: AtomicU64,
    pub ix_usage_counter: AtomicU64,
    pub latest_access_slot: AtomicU64,
}

// pub struct ProgramRuntimeEnvironments {
//     pub program_runtime_v1: ProgramRuntimeEnvironment,
//     pub program_runtime_v2: ProgramRuntimeEnvironment,
// }

pub struct ProgramRuntimeEnvironments;

pub struct LoadingTaskCookie(u64);
pub struct LoadingTaskWaiter {
    pub cookie: Mutex<LoadingTaskCookie>,
    pub cond: Condvar,
}

pub struct SecondLevel {
    pub slot_versions: Vec<Arc<LoadedProgram>>,
    pub cooperative_loading_lock: Option<(Slot, std::thread::ThreadId)>,
}

pub struct ProgramCache<FG: ForkGraph> {
    pub entries: HashMap<Pubkey, SecondLevel>,
    pub latest_root_slot: Slot,
    pub latest_root_epoch: Epoch,
    pub environments: ProgramRuntimeEnvironments,
    pub upcoming_environments: Option<ProgramRuntimeEnvironments>,
    pub programs_to_recompile: Vec<(Pubkey, Arc<LoadedProgram>)>,
    // pub stats: Stats,
    pub fork_graph: Option<Arc<RwLock<FG>>>,
    pub loading_task_waiter: Arc<LoadingTaskWaiter>,
}

pub struct LoadedProgramsForTxBatch {
    pub entries: HashMap<Pubkey, Arc<LoadedProgram>>,
    pub slot: Slot,
    pub environments: ProgramRuntimeEnvironments,
    pub upcoming_environments: Option<ProgramRuntimeEnvironments>,
    pub latest_root_epoch: Epoch,
    pub hit_max_limit: bool,
}
