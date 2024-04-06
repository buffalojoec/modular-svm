use {
    log::*,
    solana_sdk::{
        account::{AccountSharedData, ReadableAccount},
        pubkey::Pubkey,
        rent::Rent,
        transaction::{Result, TransactionError},
        transaction_context::{IndexOfAccount, TransactionContext},
    },
};

#[derive(Debug, PartialEq, Eq)]
pub enum RentState {
    /// account.lamports == 0
    Uninitialized,
    /// 0 < account.lamports < rent-exempt-minimum
    RentPaying {
        lamports: u64,    // account.lamports()
        data_size: usize, // account.data().len()
    },
    /// account.lamports >= rent-exempt-minimum
    RentExempt,
}

impl RentState {
    /// Return a new RentState instance for a given account and rent.
    pub fn from_account(account: &AccountSharedData, rent: &Rent) -> Self {
        if account.lamports() == 0 {
            Self::Uninitialized
        } else if rent.is_exempt(account.lamports(), account.data().len()) {
            Self::RentExempt
        } else {
            Self::RentPaying {
                data_size: account.data().len(),
                lamports: account.lamports(),
            }
        }
    }

    /// Check whether a transition from the pre_rent_state to this
    /// state is valid.
    pub fn transition_allowed_from(&self, pre_rent_state: &RentState) -> bool {
        match self {
            Self::Uninitialized | Self::RentExempt => true,
            Self::RentPaying {
                data_size: post_data_size,
                lamports: post_lamports,
            } => {
                match pre_rent_state {
                    Self::Uninitialized | Self::RentExempt => false,
                    Self::RentPaying {
                        data_size: pre_data_size,
                        lamports: pre_lamports,
                    } => {
                        // Cannot remain RentPaying if resized or credited.
                        post_data_size == pre_data_size && post_lamports <= pre_lamports
                    }
                }
            }
        }
    }

    pub(crate) fn check_rent_state(
        pre_rent_state: Option<&Self>,
        post_rent_state: Option<&Self>,
        transaction_context: &TransactionContext,
        index: IndexOfAccount,
    ) -> Result<()> {
        if let Some((pre_rent_state, post_rent_state)) = pre_rent_state.zip(post_rent_state) {
            let expect_msg =
                "account must exist at TransactionContext index if rent-states are Some";
            Self::check_rent_state_with_account(
                pre_rent_state,
                post_rent_state,
                transaction_context
                    .get_key_of_account_at_index(index)
                    .expect(expect_msg),
                &transaction_context
                    .get_account_at_index(index)
                    .expect(expect_msg)
                    .borrow(),
                index,
            )?;
        }
        Ok(())
    }

    pub(super) fn check_rent_state_with_account(
        pre_rent_state: &Self,
        post_rent_state: &Self,
        address: &Pubkey,
        account_state: &AccountSharedData,
        account_index: IndexOfAccount,
    ) -> Result<()> {
        Self::submit_rent_state_metrics(pre_rent_state, post_rent_state);
        if !solana_sdk::incinerator::check_id(address)
            && !post_rent_state.transition_allowed_from(pre_rent_state)
        {
            debug!(
                "Account {} not rent exempt, state {:?}",
                address, account_state,
            );
            let account_index = account_index as u8;
            Err(TransactionError::InsufficientFundsForRent { account_index })
        } else {
            Ok(())
        }
    }

    fn submit_rent_state_metrics(pre_rent_state: &Self, post_rent_state: &Self) {
        match (pre_rent_state, post_rent_state) {
            (&RentState::Uninitialized, &RentState::RentPaying { .. }) => {
                inc_new_counter_info!("rent_paying_err-new_account", 1);
            }
            (&RentState::RentPaying { .. }, &RentState::RentPaying { .. }) => {
                inc_new_counter_info!("rent_paying_ok-legacy", 1);
            }
            (_, &RentState::RentPaying { .. }) => {
                inc_new_counter_info!("rent_paying_err-other", 1);
            }
            _ => {}
        }
    }
}
