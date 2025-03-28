use candid::Principal;
use ic_cdk::api::caller;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use strategy_common::types::{
    DCAConfig, DeploymentRecord, DeploymentRequest,
    FixedBalanceConfig, LimitOrderConfig, SelfHedgingConfig,
    StrategyMetadata, StrategyType, ValueAvgConfig,
};
use crate::payment::{
    process_deposit, withdraw_funds, user_withdraw_funds, 
    payment_error_to_string
};
use crate::state::{
    get_all_deployment_records, get_deployment_records_by_owner, get_strategy_metadata,
    is_admin, require_admin, set_fee,
    get_deployment_record, get_user_account, get_user_transaction_records, 
    update_user_balance, record_transaction, TransactionType, get_fee, 
    pre_upgrade as state_pre_upgrade, post_upgrade as state_post_upgrade,
    UserAccount, TransactionRecord, WasmModule,
};
use crate::deployment_manager;
use crate::timer;

// Maximum transaction limit for queries to prevent DoS
const MAX_TRANSACTION_QUERY_LIMIT: usize = 100;

// Initialization
#[init]
fn init() {
    // Set initial admin (caller of init)
    let initial_admin = caller();
    crate::state::ADMINS.with(|admins| {
        admins.borrow_mut().insert(initial_admin);
    });
    
    // Using embedded WASM modules directly, no initialization needed
    ic_cdk::println!("Factory canister initialized with embedded WASM modules");
    
    // Schedule timers
    timer::schedule_timers();
}

// Pre/Post upgrade hooks
#[pre_upgrade]
fn pre_upgrade() {
    // Cancel timers before upgrade
    timer::cancel_timers();
    
    // Save state
    state_pre_upgrade();
}

#[post_upgrade]
fn post_upgrade() {
    // Restore state
    state_post_upgrade();
    
    // Using embedded WASM modules directly, no reinitialization needed
    ic_cdk::println!("Factory canister upgraded with embedded WASM modules");
    
    // Restart timers
    timer::schedule_timers();
}

// Admin functions
#[update]
fn add_admin(principal: Principal) -> Result<(), String> {
    require_admin()?;
    
    if principal == Principal::anonymous() {
        return Err("Cannot add anonymous principal as admin".to_string());
    }
    
    crate::state::ADMINS.with(|admins| {
        admins.borrow_mut().insert(principal);
    });
    
    Ok(())
}

#[update]
fn remove_admin(principal: Principal) -> Result<(), String> {
    require_admin()?;
    
    // Check if attempting to remove the last admin
    let is_last_admin = crate::state::ADMINS.with(|admins| {
        let admins_ref = admins.borrow();
        admins_ref.len() <= 1 && admins_ref.contains(&principal)
    });
    
    if is_last_admin {
        return Err("Cannot remove the last admin".to_string());
    }
    
    // If not the last admin, proceed with removal
    crate::state::ADMINS.with(|admins| {
        admins.borrow_mut().remove(&principal);
    });
    
    Ok(())
}

#[query]
fn get_admins() -> Vec<Principal> {
    crate::state::ADMINS.with(|admins| {
        admins.borrow().iter().cloned().collect()
    })
}

#[query]
fn is_caller_admin() -> bool {
    is_admin()
}

// WASM module management
// WASM query - now returns embedded WASM directly
#[query]
fn get_strategy_wasm(strategy_type: StrategyType) -> Option<Vec<u8>> {
    // Only admins can retrieve WASM modules
    if !is_admin() {
        return None;
    }
    
    deployment_manager::get_embedded_wasm_module(strategy_type)
}

// Deployment fee management
#[update]
fn set_deployment_fee(fee_e8s: u64) -> Result<(), String> {
    require_admin()?;
    let _ = set_fee(fee_e8s);
    Ok(())
}

#[query]
fn get_deployment_fee() -> u64 {
    get_fee()
}

// Strategy registry queries
#[query]
fn get_strategies_by_owner(owner: Principal) -> Vec<StrategyMetadata> {
    let requestor = caller();
    
    // Only allow users to see their own strategies or admins to see anyone's strategies
    if requestor != owner && !is_admin() {
        return Vec::new();
    }
    
    crate::state::OWNER_STRATEGIES.with(|owner_strategies| {
        if let Some(canister_ids) = owner_strategies.borrow().get(&owner) {
            canister_ids.iter()
                .filter_map(|id| get_strategy_metadata(*id))
                .collect()
        } else {
            Vec::new()
        }
    })
}

#[query]
fn get_all_strategies() -> Vec<StrategyMetadata> {
    crate::state::STRATEGIES.with(|s| {
        s.borrow().iter()
            .filter_map(|(_, metadata_bytes)| {
                candid::decode_one::<StrategyMetadata>(&metadata_bytes.data).ok()
            })
            .collect()
    })
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
    // Admin-only
    if !is_admin() {
        return Vec::new();
    }
    
    get_all_deployment_records()
}

#[query]
fn get_my_deployment_records() -> Vec<DeploymentRecord> {
    get_deployment_records_by_owner(caller())
}

#[query]
fn get_deployment(deployment_id: String) -> Option<DeploymentRecord> {
    let record = get_deployment_record(&deployment_id)?;
    
    // Security: only allow access to own records or admin access
    if record.owner == caller() || is_admin() {
        Some(record)
    } else {
        None
    }
}

// Strategy deployment API
#[update]
async fn request_dca_strategy(config: DCAConfig) -> Result<DeploymentRequest, String> {
    // Verify caller is not anonymous
    if caller() == Principal::anonymous() {
        return Err("Anonymous principal cannot deploy strategies".to_string());
    }
    
    deployment_manager::create_strategy_request(config).await
}

#[update]
async fn request_value_avg_strategy(config: ValueAvgConfig) -> Result<DeploymentRequest, String> {
    // Verify caller is not anonymous
    if caller() == Principal::anonymous() {
        return Err("Anonymous principal cannot deploy strategies".to_string());
    }
    
    deployment_manager::create_strategy_request(config).await
}

#[update]
async fn request_fixed_balance_strategy(config: FixedBalanceConfig) -> Result<DeploymentRequest, String> {
    // Verify caller is not anonymous
    if caller() == Principal::anonymous() {
        return Err("Anonymous principal cannot deploy strategies".to_string());
    }
    
    deployment_manager::create_strategy_request(config).await
}

#[update]
async fn request_limit_order_strategy(config: LimitOrderConfig) -> Result<DeploymentRequest, String> {
    // Verify caller is not anonymous
    if caller() == Principal::anonymous() {
        return Err("Anonymous principal cannot deploy strategies".to_string());
    }
    
    deployment_manager::create_strategy_request(config).await
}

#[update]
async fn request_self_hedging_strategy(config: SelfHedgingConfig) -> Result<DeploymentRequest, String> {
    // Verify caller is not anonymous
    if caller() == Principal::anonymous() {
        return Err("Anonymous principal cannot deploy strategies".to_string());
    }
    
    deployment_manager::create_strategy_request(config).await
}

// Admin-only force execution
#[update]
async fn force_execute_deployment(deployment_id: String) -> Result<deployment_manager::DeploymentResult, String> {
    require_admin()?;
    deployment_manager::execute_deployment(&deployment_id).await
}

// Cycles management
#[query]
fn get_cycles_balance() -> u64 {
    ic_cdk::api::canister_balance()
}

#[update]
async fn withdraw_icp(recipient: Principal, amount_e8s: u64) -> Result<(), String> {
    require_admin()?;
    withdraw_funds(recipient, amount_e8s).await
}

// User balance management API
#[update]
async fn deposit_icp(amount: u64) -> Result<u64, String> {
    let user = caller();
    
    // Anonymous users cannot deposit
    if user == Principal::anonymous() {
        return Err("Anonymous identity cannot make deposits".to_string());
    }
    
    // Process the deposit and handle errors
    match process_deposit(user, amount).await {
        Ok(new_balance) => Ok(new_balance),
        Err(e) => Err(payment_error_to_string(e))
    }
}

#[update]
async fn withdraw_user_icp(amount: u64) -> Result<u64, String> {
    let user = caller();
    
    // Anonymous users cannot withdraw
    if user == Principal::anonymous() {
        return Err("Anonymous identity cannot withdraw funds".to_string());
    }
    
    // Process the withdrawal and handle errors
    match user_withdraw_funds(user, amount).await {
        Ok(new_balance) => Ok(new_balance),
        Err(e) => Err(payment_error_to_string(e))
    }
}

#[query]
fn get_balance() -> u64 {
    let user = caller();
    get_user_account(user).balance
}

#[query]
fn get_account_info() -> UserAccount {
    let user = caller();

    get_user_account(user)
}

#[query]
fn get_transaction_history() -> Vec<TransactionRecord> {
    let user = caller();
    
    // Get transactions with limit
    let transactions = get_user_transaction_records(user);
    
    // Apply limit to prevent DoS attacks
    if transactions.len() > MAX_TRANSACTION_QUERY_LIMIT {
        transactions.into_iter().take(MAX_TRANSACTION_QUERY_LIMIT).collect()
    } else {
        transactions
    }
}

#[update]
async fn adjust_balance(user: Principal, amount: u64, reason: String) -> Result<(), String> {
    // Verify caller is admin
    require_admin()?;
    
    // Add to user's balance
    update_user_balance(user, amount, true)?;
    
    // Record transaction
    record_transaction(
        user,
        amount,
        TransactionType::AdminAdjustment,
        reason
    ).await;
    
    Ok(())
}

// System maintenance functions
#[update]
fn reset_system_timers() -> Result<(), String> {
    require_admin()?;
    timer::cancel_timers();
    timer::schedule_timers();
    Ok(())
}

#[query]
fn get_timer_status() -> String {
    timer::get_timer_status()
}

// Version information
#[query]
fn get_version() -> String {
    "1.0.3".to_string()
} 