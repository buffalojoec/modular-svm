use {
    crate::{ic_logger_msg, log_collector::LogCollector},
    base64::{prelude::BASE64_STANDARD, Engine},
    itertools::Itertools,
    solana_sdk::pubkey::Pubkey,
    std::{cell::RefCell, rc::Rc},
};

pub fn program_invoke(
    log_collector: &Option<Rc<RefCell<LogCollector>>>,
    program_id: &Pubkey,
    invoke_depth: usize,
) {
    ic_logger_msg!(
        log_collector,
        "Program {} invoke [{}]",
        program_id,
        invoke_depth
    );
}

pub fn program_log(log_collector: &Option<Rc<RefCell<LogCollector>>>, message: &str) {
    ic_logger_msg!(log_collector, "Program log: {}", message);
}

pub fn program_data(log_collector: &Option<Rc<RefCell<LogCollector>>>, data: &[&[u8]]) {
    ic_logger_msg!(
        log_collector,
        "Program data: {}",
        data.iter().map(|v| BASE64_STANDARD.encode(v)).join(" ")
    );
}

pub fn program_return(
    log_collector: &Option<Rc<RefCell<LogCollector>>>,
    program_id: &Pubkey,
    data: &[u8],
) {
    ic_logger_msg!(
        log_collector,
        "Program return: {} {}",
        program_id,
        BASE64_STANDARD.encode(data)
    );
}

pub fn program_success(log_collector: &Option<Rc<RefCell<LogCollector>>>, program_id: &Pubkey) {
    ic_logger_msg!(log_collector, "Program {} success", program_id);
}

pub fn program_failure<E: std::fmt::Display>(
    log_collector: &Option<Rc<RefCell<LogCollector>>>,
    program_id: &Pubkey,
    err: &E,
) {
    ic_logger_msg!(log_collector, "Program {} failed: {}", program_id, err);
}
