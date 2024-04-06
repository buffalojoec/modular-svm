use {
    crate::{
        compute_budget::DEFAULT_HEAP_COST,
        prioritization_fee::{PrioritizationFeeDetails, PrioritizationFeeType},
    },
    solana_sdk::{
        entrypoint::HEAP_LENGTH as MIN_HEAP_FRAME_BYTES, fee::FeeBudgetLimits,
        instruction::CompiledInstruction, pubkey::Pubkey, transaction::TransactionError,
    },
};

pub const DEFAULT_INSTRUCTION_COMPUTE_UNIT_LIMIT: u32 = 200_000;
pub const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;

/// The total accounts data a transaction can load is limited to 64MiB to not break
/// anyone in Mainnet-beta today. It can be set by set_loaded_accounts_data_size_limit instruction
pub const MAX_LOADED_ACCOUNTS_DATA_SIZE_BYTES: u32 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeBudgetLimits {
    pub updated_heap_bytes: u32,
    pub compute_unit_limit: u32,
    pub compute_unit_price: u64,
    pub loaded_accounts_bytes: u32,
}

impl Default for ComputeBudgetLimits {
    fn default() -> Self {
        ComputeBudgetLimits {
            updated_heap_bytes: u32::try_from(MIN_HEAP_FRAME_BYTES).unwrap(),
            compute_unit_limit: MAX_COMPUTE_UNIT_LIMIT,
            compute_unit_price: 0,
            loaded_accounts_bytes: MAX_LOADED_ACCOUNTS_DATA_SIZE_BYTES,
        }
    }
}

impl From<ComputeBudgetLimits> for FeeBudgetLimits {
    fn from(val: ComputeBudgetLimits) -> Self {
        let prioritization_fee_details = PrioritizationFeeDetails::new(
            PrioritizationFeeType::ComputeUnitPrice(val.compute_unit_price),
            u64::from(val.compute_unit_limit),
        );
        let prioritization_fee = prioritization_fee_details.get_fee();

        FeeBudgetLimits {
            // NOTE - usize::from(u32).unwrap() may fail if target is 16-bit and
            // `loaded_accounts_bytes` is greater than u16::MAX. In that case, panic is proper.
            loaded_accounts_data_size_limit: usize::try_from(val.loaded_accounts_bytes).unwrap(),
            heap_cost: DEFAULT_HEAP_COST,
            compute_unit_limit: u64::from(val.compute_unit_limit),
            prioritization_fee,
        }
    }
}

/// Processing compute_budget could be part of tx sanitizing, failed to process
/// these instructions will drop the transaction eventually without execution,
/// may as well fail it early.
/// If succeeded, the transaction's specific limits/requests (could be default)
/// are retrieved and returned,
pub fn process_compute_budget_instructions<'a>(
    _instructions: impl Iterator<Item = (&'a Pubkey, &'a CompiledInstruction)>,
) -> Result<ComputeBudgetLimits, TransactionError> {
    /*
     * Function simplified for brevity.
     */
    Ok(ComputeBudgetLimits {
        updated_heap_bytes: 0,
        compute_unit_limit: 0,
        compute_unit_price: 0,
        loaded_accounts_bytes: 0,
    })
}
