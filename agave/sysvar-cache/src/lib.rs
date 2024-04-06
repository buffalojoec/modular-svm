//! Agave Sysvar Cache.

use {
    solana_sdk::sysvar::{
        clock::Clock, epoch_rewards::EpochRewards, epoch_schedule::EpochSchedule,
        last_restart_slot::LastRestartSlot, rent::Rent, slot_hashes::SlotHashes,
        stake_history::StakeHistory,
    },
    std::sync::Arc,
};

pub struct SysvarCache {
    pub clock: Option<Arc<Clock>>,
    pub epoch_schedule: Option<Arc<EpochSchedule>>,
    pub epoch_rewards: Option<Arc<EpochRewards>>,
    pub rent: Option<Arc<Rent>>,
    pub slot_hashes: Option<Arc<SlotHashes>>,
    pub stake_history: Option<Arc<StakeHistory>>,
    pub last_restart_slot: Option<Arc<LastRestartSlot>>,
}
