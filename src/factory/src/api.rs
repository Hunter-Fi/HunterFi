use candid::Principal;
use ic_cdk::api::{caller, time};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use strategy_common::types::{
    DCAConfig, DeploymentRecord, DeploymentRequest, DeploymentResult,
    DeploymentStatus, FixedBalanceConfig, LimitOrderConfig, SelfHedgingConfig,
    StrategyMetadata, StrategyType, ValueAvgConfig,
};

use crate::deployment::{
    confirm_deployment_authorization, create_dca_request, create_fixed_balance_request,
    create_limit_order_request, create_self_hedging_request, create_value_avg_request,
    execute_deployment,
};
use crate::payment::{process_refund, schedule_refunds_for_failed_deployments, withdraw_funds};
use crate::state::{get_all_deployment_records, get_deployment_record, get_deployment_records_by_owner, get_fee, get_strategy_metadata, get_upgrade_data, get_wasm_module, is_admin, require_admin, restore_upgrade_data, set_fee, store_wasm_module, WasmModule};
use crate::timer;

// Initialization
#[init]
fn init() {
    // Set initial admin (caller of init)
    let initial_admin = caller();
    crate::state::ADMINS.with(|admins| {
        admins.borrow_mut().insert(initial_admin);
    });
    
    // Schedule refunds for any failed deployments on startup
    schedule_refunds_for_failed_deployments();
    
    // Start the timer system for processing deployment statuses
    timer::schedule_timers();
}

// Pre/Post upgrade hooks
#[pre_upgrade]
fn pre_upgrade() {
    // Cancel all timers before upgrade
    timer::cancel_timers();
    
    // Save upgrade data
    let upgrade_data = get_upgrade_data();
    let serialized = candid::encode_one(&upgrade_data).expect("Failed to encode upgrade data");
    ic_cdk::storage::stable_save((serialized,)).unwrap();
}

#[post_upgrade]
fn post_upgrade() {
    if let Ok((serialized,)) = ic_cdk::storage::stable_restore::<(Vec<u8>,)>() {
        if let Ok(data) = candid::decode_one(&serialized) {
            restore_upgrade_data(data);
        }
    }
    
    // Schedule refunds for any failed deployments after upgrade
    schedule_refunds_for_failed_deployments();
    
    // Restart the timer system
    timer::schedule_timers();
    
    // Process all deployments that might be in intermediate states
    ic_cdk::spawn(async {
        timer::process_failed_deployments().await;
        timer::process_refunding_deployments().await;
    });
}

// Admin functions
#[update]
async fn add_admin(principal: Principal) -> Result<(), String> {
    require_admin()?;
    
    crate::state::ADMINS.with(|admins| {
        admins.borrow_mut().insert(principal);
    });
    
    Ok(())
}

#[update]
async fn remove_admin(principal: Principal) -> Result<(), String> {
    require_admin()?;
    
    // Prevent removing the last admin
    let is_last_admin = crate::state::ADMINS.with(|admins| {
        let admins_ref = admins.borrow();
        admins_ref.len() == 1 && admins_ref.contains(&principal)
    });
    
    if is_last_admin {
        return Err("Cannot remove the last admin".to_string());
    }
    
    crate::state::ADMINS.with(|admins| {
        admins.borrow_mut().remove(&principal);
    });
    
    Ok(())
}

#[query]
fn get_admins() -> Vec<Principal> {
    crate::state::ADMINS.with(|admins| admins.borrow().iter().cloned().collect())
}

#[query]
fn is_caller_admin() -> bool {
    is_admin()
}

// Strategy WASM module management
#[update]
async fn install_strategy_wasm(wasm_module: WasmModule) -> Result<(), String> {
    require_admin()?;
    store_wasm_module(wasm_module)
}

#[query]
fn get_strategy_wasm(strategy_type: StrategyType) -> Option<Vec<u8>> {
    get_wasm_module(strategy_type)
}

// Fee management
#[update]
fn set_deployment_fee(fee_e8s: u64) -> Result<(), String> {
    set_fee(fee_e8s)
}

#[query]
fn get_deployment_fee() -> u64 {
    get_fee()
}

// Strategy registration queries
#[query]
fn get_strategies_by_owner(owner: Principal) -> Vec<StrategyMetadata> {
    let mut strategies = Vec::new();
    
    crate::state::OWNER_STRATEGIES.with(|owner_strategies| {
        if let Some(canister_ids) = owner_strategies.borrow().get(&owner) {
            for canister_id in canister_ids {
                if let Some(metadata) = get_strategy_metadata(*canister_id) {
                    strategies.push(metadata);
                }
            }
        }
    });
    
    strategies
}

#[query]
fn get_all_strategies() -> Vec<StrategyMetadata> {
    let mut strategies = Vec::new();
    
    crate::state::STRATEGIES.with(|s| {
        for (_, metadata_bytes) in s.borrow().iter() {
            if let Ok(metadata) = candid::decode_one(&metadata_bytes.0) {
                strategies.push(metadata);
            }
        }
    });
    
    strategies
}

#[query]
fn get_strategy(canister_id: Principal) -> Option<StrategyMetadata> {
    get_strategy_metadata(canister_id)
}

#[query]
fn get_strategy_count() -> u64 {
    crate::state::STRATEGIES.with(|s| s.borrow().len() as u64)
}

// Deployment record management
#[query]
fn get_deployment_records() -> Vec<DeploymentRecord> {
    get_all_deployment_records()
}

#[query]
fn get_my_deployment_records() -> Vec<DeploymentRecord> {
    get_deployment_records_by_owner(caller())
}

#[query]
fn get_deployment(deployment_id: String) -> Option<DeploymentRecord> {
    get_deployment_record(&deployment_id)
}

// Strategy deployment requests
#[update]
async fn request_dca_strategy(config: DCAConfig) -> Result<DeploymentRequest, String> {
    create_dca_request(config).await
}

#[update]
async fn request_value_avg_strategy(config: ValueAvgConfig) -> Result<DeploymentRequest, String> {
    create_value_avg_request(config).await
}

#[update]
async fn request_fixed_balance_strategy(config: FixedBalanceConfig) -> Result<DeploymentRequest, String> {
    create_fixed_balance_request(config).await
}

#[update]
async fn request_limit_order_strategy(config: LimitOrderConfig) -> Result<DeploymentRequest, String> {
    create_limit_order_request(config).await
}

#[update]
async fn request_self_hedging_strategy(config: SelfHedgingConfig) -> Result<DeploymentRequest, String> {
    create_self_hedging_request(config).await
}

// Deployment authorization and execution
#[update]
async fn confirm_deployment(deployment_id: String) -> Result<(), String> {
    confirm_deployment_authorization(&deployment_id).await
}

#[update]
async fn force_execute_deployment(deployment_id: String) -> Result<DeploymentResult, String> {
    require_admin()?;
    execute_deployment(&deployment_id).await
}

// Refund management
#[update]
async fn request_refund(deployment_id: String) -> Result<(), String> {
    let record = get_deployment_record(&deployment_id)
        .ok_or_else(|| format!("Deployment record not found for ID: {}", deployment_id))?;
    
    // Verify caller is the owner or an admin
    let caller_principal = caller();
    if record.owner != caller_principal && !is_admin() {
        return Err("Only the owner or an admin can request a refund".to_string());
    }
    
    // Check if eligible for refund
    match record.status {
        DeploymentStatus::DeploymentFailed => {
            // Already failed, start the refund process
            process_refund(&deployment_id).await
        },
        DeploymentStatus::PendingPayment | DeploymentStatus::AuthorizationConfirmed => {
            // Payment not yet collected, just mark as failed and don't refund
            crate::state::update_deployment_status(
                &deployment_id,
                DeploymentStatus::DeploymentFailed,
                None,
                Some("Deployment cancelled by user".to_string())
            ).map(|_| ())
        },
        DeploymentStatus::Refunding => {
            // Already refunding, just trigger another refund attempt
            process_refund(&deployment_id).await
        },
        DeploymentStatus::Refunded => {
            Err("This deployment has already been refunded".to_string())
        },
        _ => {
            // If deployment is past payment received, needs admin approval
            if is_admin() {
                // Mark as failed and process refund
                crate::state::update_deployment_status(
                    &deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    None,
                    Some("Refund authorized by admin".to_string())
                )?;
                
                process_refund(&deployment_id).await
            } else {
                Err("This deployment is in progress or completed and cannot be refunded without admin approval".to_string())
            }
        }
    }
}

// Manual timer control (admin only)
#[update]
async fn restart_timers() -> Result<(), String> {
    require_admin()?;
    timer::reset_timers();
    Ok(())
}

#[update]
async fn trigger_status_processing() -> Result<(), String> {
    require_admin()?;
    ic_cdk::spawn(timer::process_deployment_statuses());
    Ok(())
}

#[update]
async fn trigger_failed_deployment_processing() -> Result<(), String> {
    require_admin()?;
    ic_cdk::spawn(timer::process_failed_deployments());
    Ok(())
}

// Cycles and funds management
#[query]
fn get_cycles_balance() -> u64 {
    ic_cdk::api::canister_balance()
}

#[update]
async fn withdraw_icp(recipient: Principal, amount_e8s: u64) -> Result<(), String> {
    require_admin()?;
    withdraw_funds(recipient, amount_e8s).await
} 