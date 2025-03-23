use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::call::call;
use ic_cdk::api::management_canister::main::{
    canister_status, deposit_cycles, CanisterIdRecord, CanisterStatusResponse,
};
use ic_cdk::api::{canister_balance, id};
use serde::Serialize;
use std::cell::RefCell;

/// Cycles management configuration
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CyclesConfig {
    pub threshold_warning: u64, // Warning threshold in cycles
    pub threshold_critical: u64, // Critical threshold in cycles
    pub cycles_to_refill: u64,   // Amount to refill when low
    pub refill_source: Option<Principal>, // Source canister for refills
}

thread_local! {
    static CONFIG: RefCell<Option<CyclesConfig>> = RefCell::new(None);
}

/// Initialize cycles management with configuration
pub fn init(config: CyclesConfig) {
    CONFIG.with(|c| *c.borrow_mut() = Some(config));
}

/// Get current cycles balance
pub fn get_balance() -> u64 {
    canister_balance()
}

/// Check if cycles are below warning threshold
pub fn is_below_warning_threshold() -> bool {
    CONFIG.with(|c| {
        if let Some(config) = c.borrow().as_ref() {
            return canister_balance() < config.threshold_warning;
        }
        false
    })
}

/// Check if cycles are below critical threshold
pub fn is_below_critical_threshold() -> bool {
    CONFIG.with(|c| {
        if let Some(config) = c.borrow().as_ref() {
            return canister_balance() < config.threshold_critical;
        }
        false
    })
}

/// Request cycles refill from the source canister
pub async fn request_cycles_refill() -> Result<(), String> {
    let self_id = id();
    
    let config = CONFIG.with(|c| c.borrow().clone());
    if let Some(config) = config {
        if let Some(source) = config.refill_source {
            let args = CanisterIdRecord { canister_id: self_id };
            let result = deposit_cycles(args, config.cycles_to_refill as u128).await;
            match result {
                Ok(_) => Ok(()),
                Err((_, e)) => Err(format!("Failed to request cycles: {}", e)),
            }
        } else {
            Err("No refill source configured".to_string())
        }
    } else {
        Err("Cycles management not initialized".to_string())
    }
}

/// Get canister status including cycles information
pub async fn get_canister_status() -> Result<CanisterStatusResponse, String> {
    let args = CanisterIdRecord { canister_id: id() };
    let result = canister_status(args).await;
    
    match result {
        Ok((response,)) => Ok(response),
        Err((code, msg)) => Err(format!("Error code: {:?}, message: {}", code, msg)),
    }
}

/// Get cycles management summary
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CyclesSummary {
    pub balance: u64,
    pub warning_threshold: u64,
    pub critical_threshold: u64,
    pub status: CyclesStatus,
}

/// Cycles status enum
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CyclesStatus {
    Healthy,
    Warning,
    Critical,
}

/// Get cycles management summary
pub fn get_summary() -> Result<CyclesSummary, String> {
    CONFIG.with(|c| {
        if let Some(config) = c.borrow().as_ref() {
            let balance = canister_balance();
            let status = if balance < config.threshold_critical {
                CyclesStatus::Critical
            } else if balance < config.threshold_warning {
                CyclesStatus::Warning
            } else {
                CyclesStatus::Healthy
            };
            
            Ok(CyclesSummary {
                balance,
                warning_threshold: config.threshold_warning,
                critical_threshold: config.threshold_critical,
                status,
            })
        } else {
            Err("Cycles management not initialized".to_string())
        }
    })
} 