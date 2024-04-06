#![cfg_attr(RUSTC_WITH_SPECIALIZATION, feature(min_specialization))]
#![allow(clippy::arithmetic_side_effects)]

pub mod account_loader;
pub mod account_overrides;
pub mod message_processor;
pub mod transaction_error_metrics;
pub mod transaction_processor;
pub mod transaction_results;

#[macro_use]
extern crate solana_metrics;

#[macro_use]
extern crate solana_frozen_abi_macro;
