use {
    solana_measure::measure::Measure,
    solana_program_runtime::{
        invoke_context::InvokeContext,
        timings::{ExecuteDetailsTimings, ExecuteTimings},
    },
    solana_sdk::{
        message::SanitizedMessage, precompiles::is_precompile, saturating_add_assign,
        transaction::TransactionError, transaction_context::IndexOfAccount,
    },
};

pub struct MessageProcessor {}

impl MessageProcessor {
    pub fn process_message(
        message: &SanitizedMessage,
        program_indices: &[Vec<IndexOfAccount>],
        invoke_context: &mut InvokeContext,
        timings: &mut ExecuteTimings,
        accumulated_consumed_units: &mut u64,
    ) -> Result<(), TransactionError> {
        /*
         * Function simplified for brevity.
         */
        for (instruction_index, ((program_id, instruction), program_indices)) in message
            .program_instructions_iter()
            .zip(program_indices.iter())
            .enumerate()
        {
            let instruction_accounts = Vec::with_capacity(instruction.accounts.len());

            let is_precompile =
                is_precompile(program_id, |id| invoke_context.feature_set.is_active(id));

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
