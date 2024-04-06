#[allow(deprecated)]
use solana_sdk::sysvar::last_restart_slot::LastRestartSlot;
use {
    solana_sdk::{
        pubkey::Pubkey,
        sysvar::{
            clock::Clock, epoch_rewards::EpochRewards, epoch_schedule::EpochSchedule, rent::Rent,
            slot_hashes::SlotHashes, stake_history::StakeHistory,
        },
    },
    std::sync::Arc,
};

#[derive(Default, Clone, Debug)]
pub struct SysvarCache {
    pub clock: Option<Arc<Clock>>,
    pub epoch_schedule: Option<Arc<EpochSchedule>>,
    pub epoch_rewards: Option<Arc<EpochRewards>>,
    pub rent: Option<Arc<Rent>>,
    pub slot_hashes: Option<Arc<SlotHashes>>,
    pub stake_history: Option<Arc<StakeHistory>>,
    pub last_restart_slot: Option<Arc<LastRestartSlot>>,
}

impl SysvarCache {
    pub fn fill_missing_entries<F: FnMut(&Pubkey, &mut dyn FnMut(&[u8]))>(
        &mut self,
        mut _get_account_data: F,
    ) {
        /*
         * Function simplified for brevity.
         */
    }
}
