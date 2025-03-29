use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::management_canister::main::{
    canister_status, deposit_cycles, CanisterIdRecord, CanisterStatusResponse,
};
use ic_cdk::api::{canister_balance, id};
use serde::Serialize;
use std::cell::{Cell, RefCell};

/// Cycles management configuration
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CyclesConfig {
    pub warning_threshold: u64,
    pub critical_threshold: u64,
    pub refill_amount: u64,
    pub refill_source: Option<Principal>,
}

thread_local! {
    static WARNING_THRESHOLD: Cell<u64> = Cell::new(500_000_000_000);
    static CRITICAL_THRESHOLD: Cell<u64> = Cell::new(100_000_000_000);
    static CONFIG: RefCell<Option<CyclesConfig>> = RefCell::new(None);
}

/// Initialize cycles management with configuration
pub fn init(config: CyclesConfig) {
    warning_threshold_set(config.warning_threshold);
    critical_threshold_set(config.critical_threshold);
    CONFIG.with(|c| *c.borrow_mut() = Some(config));
}

/// Get current cycles balance
pub fn get_balance() -> u64 {
    canister_balance()
}

/// Check if cycles are below warning threshold
pub fn is_below_warning_threshold() -> bool {
    let balance = get_balance();
    WARNING_THRESHOLD.with(|t| {
        balance < t.get()
    })
}

/// Check if cycles are below critical threshold
pub fn is_below_critical_threshold() -> bool {
    let balance = get_balance();
    CRITICAL_THRESHOLD.with(|t| {
        balance < t.get()
    })
}

/// Request cycles refill from the source canister
pub async fn request_cycles_refill() -> Result<(), String> {
    let self_id = id();
    let config = CONFIG.with(|c| c.borrow().clone());
    
    if let Some(config) = config {
        if let Some(source) = config.refill_source {
            let args = CanisterIdRecord { canister_id: self_id };
            let result = deposit_cycles(args, config.refill_amount as u128).await;
            match result {
                Ok(_) => Ok(()),
                Err((_, e)) => Err(format!("Failed to request cycles: {}", e)),
            }
        } else {
            return Err("No refill source configured".to_string());
        }
    } else {
        Err("Cycles config not initialized".to_string())
    }
}

/// Get canister status including cycles information
pub async fn get_canister_status() -> Result<CanisterStatusResponse, String> {
    let args = CanisterIdRecord { canister_id: id() };
    let result = canister_status(args).await;
    
    match result {
        Ok((response,)) => Ok(response),
        Err((code, msg)) => Err(format!(
            "Failed to get canister status. Code: {:?}, Message: {}",
            code, msg
        )),
    }
}

/// Set warning threshold
fn warning_threshold_set(threshold: u64) {
    WARNING_THRESHOLD.with(|t| t.set(threshold));
}

/// Set critical threshold
fn critical_threshold_set(threshold: u64) {
    CRITICAL_THRESHOLD.with(|t| t.set(threshold));
}

/// Get cycles management summary
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CyclesSummary {
    pub balance: u64,
    pub warning_threshold: u64,
    pub critical_threshold: u64,
    pub is_below_warning: bool,
    pub is_below_critical: bool,
}

/// Get cycles management summary
pub fn get_summary() -> Result<CyclesSummary, String> {
    let balance = get_balance();
    let warning_threshold = WARNING_THRESHOLD.with(|t| t.get());
    let critical_threshold = CRITICAL_THRESHOLD.with(|t| t.get());
    
    Ok(CyclesSummary {
        balance,
        warning_threshold,
        critical_threshold,
        is_below_warning: balance < warning_threshold,
        is_below_critical: balance < critical_threshold,
    })
} 