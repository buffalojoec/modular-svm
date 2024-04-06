use {
    serde::{Deserialize, Serialize},
    solana_measure::measure::Measure,
    solana_program_runtime::{
        invoke_context::InvokeContext,
        timings::{ExecuteDetailsTimings, ExecuteTimings},
    },
    solana_sdk::{
        account::WritableAccount,
        message::SanitizedMessage,
        precompiles::is_precompile,
        saturating_add_assign,
        sysvar::instructions,
        transaction::TransactionError,
        transaction_context::{IndexOfAccount, InstructionAccount},
    },
};

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct MessageProcessor {}

#[cfg(RUSTC_WITH_SPECIALIZATION)]
impl ::solana_frozen_abi::abi_example::AbiExample for MessageProcessor {
    fn example() -> Self {
        // MessageProcessor's fields are #[serde(skip)]-ed and not Serialize
        // so, just rely on Default anyway.
        MessageProcessor::default()
    }
}

impl MessageProcessor {
    /// Process a message.
    /// This method calls each instruction in the message over the set of loaded accounts.
    /// For each instruction it calls the program entrypoint method and verifies that the result of
    /// the call does not violate the bank's accounting rules.
    /// The accounts are committed back to the bank only if every instruction succeeds.
    pub fn process_message(
        message: &SanitizedMessage,
        program_indices: &[Vec<IndexOfAccount>],
        invoke_context: &mut InvokeContext,
        timings: &mut ExecuteTimings,
        accumulated_consumed_units: &mut u64,
    ) -> Result<(), TransactionError> {
        debug_assert_eq!(program_indices.len(), message.instructions().len());
        for (instruction_index, ((program_id, instruction), program_indices)) in message
            .program_instructions_iter()
            .zip(program_indices.iter())
            .enumerate()
        {
            let is_precompile =
                is_precompile(program_id, |id| invoke_context.feature_set.is_active(id));

            // Fixup the special instructions key if present
            // before the account pre-values are taken care of
            if let Some(account_index) = invoke_context
                .transaction_context
                .find_index_of_account(&instructions::id())
            {
                let mut mut_account_ref = invoke_context
                    .transaction_context
                    .get_account_at_index(account_index)
                    .map_err(|_| TransactionError::InvalidAccountIndex)?
                    .borrow_mut();
                instructions::store_current_index(
                    mut_account_ref.data_as_mut_slice(),
                    instruction_index as u16,
                );
            }

            let mut instruction_accounts = Vec::with_capacity(instruction.accounts.len());
            for (instruction_account_index, index_in_transaction) in
                instruction.accounts.iter().enumerate()
            {
                let index_in_callee = instruction
                    .accounts
                    .get(0..instruction_account_index)
                    .ok_or(TransactionError::InvalidAccountIndex)?
                    .iter()
                    .position(|account_index| account_index == index_in_transaction)
                    .unwrap_or(instruction_account_index)
                    as IndexOfAccount;
                let index_in_transaction = *index_in_transaction as usize;
                instruction_accounts.push(InstructionAccount {
                    index_in_transaction: index_in_transaction as IndexOfAccount,
                    index_in_caller: index_in_transaction as IndexOfAccount,
                    index_in_callee,
                    is_signer: message.is_signer(index_in_transaction),
                    is_writable: message.is_writable(index_in_transaction),
                });
            }

            let result = if is_precompile {
                invoke_context
                    .transaction_context
                    .get_next_instruction_context()
                    .map(|instruction_context| {
                        instruction_context.configure(
                            program_indices,
                            &instruction_accounts,
                            &instruction.data,
                        );
                    })
                    .and_then(|_| {
                        invoke_context.transaction_context.push()?;
                        invoke_context.transaction_context.pop()
                    })
            } else {
                let time = Measure::start("execute_instruction");
                let mut compute_units_consumed = 0;
                let result = invoke_context.process_instruction(
                    &instruction.data,
                    &instruction_accounts,
                    program_indices,
                    &mut compute_units_consumed,
                    timings,
                );
                let time = time.end_as_us();
                *accumulated_consumed_units =
                    accumulated_consumed_units.saturating_add(compute_units_consumed);
                timings.details.accumulate_program(
                    program_id,
                    time,
                    compute_units_consumed,
                    result.is_err(),
                );
                invoke_context.timings = {
                    timings.details.accumulate(&invoke_context.timings);
                    ExecuteDetailsTimings::default()
                };
                saturating_add_assign!(
                    timings.execute_accessories.process_instructions.total_us,
                    time
                );
                result
            };

            result
                .map_err(|err| TransactionError::InstructionError(instruction_index as u8, err))?;
        }
        Ok(())
    }
}
