use {
    crate::compute_budget_processor,
    solana_sdk::{instruction::CompiledInstruction, pubkey::Pubkey, transaction},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComputeBudget {
    pub compute_unit_limit: u64,
    pub log_64_units: u64,
    pub create_program_address_units: u64,
    pub invoke_units: u64,
    pub max_invoke_stack_height: usize,
    pub max_instruction_trace_length: usize,
    pub sha256_base_cost: u64,
    pub sha256_byte_cost: u64,
    pub sha256_max_slices: u64,
    pub max_call_depth: usize,
    pub stack_frame_size: usize,
    pub log_pubkey_units: u64,
    pub max_cpi_instruction_size: usize,
    pub cpi_bytes_per_unit: u64,
    pub sysvar_base_cost: u64,
    pub secp256k1_recover_cost: u64,
    pub syscall_base_cost: u64,
    pub curve25519_edwards_validate_point_cost: u64,
    pub curve25519_edwards_add_cost: u64,
    pub curve25519_edwards_subtract_cost: u64,
    pub curve25519_edwards_multiply_cost: u64,
    pub curve25519_edwards_msm_base_cost: u64,
    pub curve25519_edwards_msm_incremental_cost: u64,
    pub curve25519_ristretto_validate_point_cost: u64,
    pub curve25519_ristretto_add_cost: u64,
    pub curve25519_ristretto_subtract_cost: u64,
    pub curve25519_ristretto_multiply_cost: u64,
    pub curve25519_ristretto_msm_base_cost: u64,
    pub curve25519_ristretto_msm_incremental_cost: u64,
    pub heap_size: u32,
    pub heap_cost: u64,
    pub mem_op_base_cost: u64,
    pub alt_bn128_addition_cost: u64,
    pub alt_bn128_multiplication_cost: u64,
    pub alt_bn128_pairing_one_pair_cost_first: u64,
    pub alt_bn128_pairing_one_pair_cost_other: u64,
    pub big_modular_exponentiation_cost: u64,
    pub poseidon_cost_coefficient_a: u64,
    pub poseidon_cost_coefficient_c: u64,
    pub get_remaining_compute_units_cost: u64,
    pub alt_bn128_g1_compress: u64,
    pub alt_bn128_g1_decompress: u64,
    pub alt_bn128_g2_compress: u64,
    pub alt_bn128_g2_decompress: u64,
}

impl Default for ComputeBudget {
    fn default() -> Self {
        Self::new(compute_budget_processor::MAX_COMPUTE_UNIT_LIMIT as u64)
    }
}

impl ComputeBudget {
    pub fn new(compute_unit_limit: u64) -> Self {
        ComputeBudget {
            compute_unit_limit,
            log_64_units: 100,
            create_program_address_units: 1500,
            invoke_units: 1000,
            max_invoke_stack_height: 5,
            max_instruction_trace_length: 64,
            sha256_base_cost: 85,
            sha256_byte_cost: 1,
            sha256_max_slices: 20_000,
            max_call_depth: 64,
            stack_frame_size: 4_096,
            log_pubkey_units: 100,
            max_cpi_instruction_size: 1280, // IPv6 Min MTU size
            cpi_bytes_per_unit: 250,        // ~50MB at 200,000 units
            sysvar_base_cost: 100,
            secp256k1_recover_cost: 25_000,
            syscall_base_cost: 100,
            curve25519_edwards_validate_point_cost: 159,
            curve25519_edwards_add_cost: 473,
            curve25519_edwards_subtract_cost: 475,
            curve25519_edwards_multiply_cost: 2_177,
            curve25519_edwards_msm_base_cost: 2_273,
            curve25519_edwards_msm_incremental_cost: 758,
            curve25519_ristretto_validate_point_cost: 169,
            curve25519_ristretto_add_cost: 521,
            curve25519_ristretto_subtract_cost: 519,
            curve25519_ristretto_multiply_cost: 2_208,
            curve25519_ristretto_msm_base_cost: 2303,
            curve25519_ristretto_msm_incremental_cost: 788,
            heap_size: u32::try_from(solana_sdk::entrypoint::HEAP_LENGTH).unwrap(),
            heap_cost: compute_budget_processor::DEFAULT_HEAP_COST,
            mem_op_base_cost: 10,
            alt_bn128_addition_cost: 334,
            alt_bn128_multiplication_cost: 3_840,
            alt_bn128_pairing_one_pair_cost_first: 36_364,
            alt_bn128_pairing_one_pair_cost_other: 12_121,
            big_modular_exponentiation_cost: 33,
            poseidon_cost_coefficient_a: 61,
            poseidon_cost_coefficient_c: 542,
            get_remaining_compute_units_cost: 100,
            alt_bn128_g1_compress: 30,
            alt_bn128_g1_decompress: 398,
            alt_bn128_g2_compress: 86,
            alt_bn128_g2_decompress: 13610,
        }
    }

    pub fn try_from_instructions<'a>(
        instructions: impl Iterator<Item = (&'a Pubkey, &'a CompiledInstruction)>,
    ) -> transaction::Result<Self> {
        let compute_budget_limits =
            compute_budget_processor::process_compute_budget_instructions(instructions)?;
        Ok(ComputeBudget {
            compute_unit_limit: u64::from(compute_budget_limits.compute_unit_limit),
            heap_size: compute_budget_limits.updated_heap_bytes,
            ..ComputeBudget::default()
        })
    }
}
