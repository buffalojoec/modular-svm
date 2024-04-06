/// There are 10^6 micro-lamports in one lamport
const MICRO_LAMPORTS_PER_LAMPORT: u64 = 1_000_000;

type MicroLamports = u128;

pub enum PrioritizationFeeType {
    ComputeUnitPrice(u64),
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct PrioritizationFeeDetails {
    fee: u64,
    compute_unit_price: u64,
}

impl PrioritizationFeeDetails {
    pub fn new(fee_type: PrioritizationFeeType, compute_unit_limit: u64) -> Self {
        match fee_type {
            PrioritizationFeeType::ComputeUnitPrice(compute_unit_price) => {
                let micro_lamport_fee: MicroLamports =
                    (compute_unit_price as u128).saturating_mul(compute_unit_limit as u128);
                let fee = micro_lamport_fee
                    .saturating_add(MICRO_LAMPORTS_PER_LAMPORT.saturating_sub(1) as u128)
                    .checked_div(MICRO_LAMPORTS_PER_LAMPORT as u128)
                    .and_then(|fee| u64::try_from(fee).ok())
                    .unwrap_or(u64::MAX);

                Self {
                    fee,
                    compute_unit_price,
                }
            }
        }
    }

    pub fn get_fee(&self) -> u64 {
        self.fee
    }

    pub fn get_compute_unit_price(&self) -> u64 {
        self.compute_unit_price
    }
}
