use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::{caller, canister_balance, time};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap, StableCell};
use std::cell::RefCell;
use strategy_common::types::{
    SelfHedgingConfig, StrategyResult, StrategyStatus,
};

// Type definitions for stable storage
type Memory = VirtualMemory<DefaultMemoryImpl>;

// State structure
#[derive(CandidType, Deserialize, Clone, Debug)]
struct SelfHedgingState {
    owner: Principal,
    config: SelfHedgingConfig,
    status: StrategyStatus,
    last_execution: Option<u64>,
    execution_count: u64,
    current_primary_price: Option<u128>,
    initial_price: Option<u128>,
    hedge_position_size: u128,
}

// Thread-local storage for state
thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    static STATE: RefCell<StableCell<SelfHedgingState, Memory>> = RefCell::new(
        MEMORY_MANAGER.with(|mm| {
            let memory = mm.borrow().get(MemoryId::new(0));
            StableCell::init(
                memory,
                SelfHedgingState {
                    owner: Principal::anonymous(),
                    config: SelfHedgingConfig {
                        exchange: strategy_common::types::Exchange::ICPSwap,
                        primary_token: strategy_common::types::TokenMetadata {
                            canister_id: Principal::anonymous(),
                            symbol: "".to_string(),
                            decimals: 0,
                        },
                        hedge_token: strategy_common::types::TokenMetadata {
                            canister_id: Principal::anonymous(),
                            symbol: "".to_string(),
                            decimals: 0,
                        },
                        hedge_ratio: 0.0,
                        price_change_threshold: 0.0,
                        check_interval_secs: 0,
                        slippage_tolerance: 0.0,
                    },
                    status: StrategyStatus::Created,
                    last_execution: None,
                    execution_count: 0,
                    current_primary_price: None,
                    initial_price: None,
                    hedge_position_size: 0,
                }
            ).expect("Failed to initialize stable cell")
        })
    );
}

// Helper function to check if caller is the owner
fn verify_owner() -> Result<(), String> {
    let caller = caller();
    STATE.with(|state| {
        let state = state.borrow();
        let state_data = state.get();
        if caller != state_data.owner {
            return Err("Caller is not the owner".to_string());
        }
        Ok(())
    })
}

// Helper function to check if strategy status is appropriate for the operation
fn verify_status(expected_statuses: &[StrategyStatus]) -> Result<(), String> {
    STATE.with(|state| {
        let state = state.borrow();
        let current_status = state.get().status.clone();
        
        if !expected_statuses.contains(&current_status) {
            return Err(format!(
                "Invalid strategy status: {:?}, expected one of: {:?}",
                current_status, expected_statuses
            ));
        }
        Ok(())
    })
}

// Initialize canister
#[init]
fn init() {
    // Initialization will be handled by init_self_hedging
}

// Initialize the Self-Hedging strategy
#[update]
fn init_self_hedging(owner: Principal, config: SelfHedgingConfig) -> StrategyResult {
    let caller_id = caller();
    
    // Only allow initialization once
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let current_state = state.get();
        
        // Check if already initialized
        if current_state.owner != Principal::anonymous() {
            return StrategyResult::Error("Strategy already initialized".to_string());
        }
        
        // Validate configuration
        if config.hedge_ratio <= 0.0 || config.hedge_ratio > 1.0 {
            return StrategyResult::Error("Hedge ratio must be between 0 and 1".to_string());
        }
        
        if config.price_change_threshold <= 0.0 {
            return StrategyResult::Error("Price change threshold must be positive".to_string());
        }
        
        if config.check_interval_secs == 0 {
            return StrategyResult::Error("Check interval cannot be zero".to_string());
        }
        
        // Create new state
        let new_state = SelfHedgingState {
            owner,
            config,
            status: StrategyStatus::Created,
            last_execution: None,
            execution_count: 0,
            current_primary_price: None,
            initial_price: None,
            hedge_position_size: 0,
        };
        
        // Store the new state
        match state.set(new_state) {
            Ok(_) => StrategyResult::Success,
            Err(e) => StrategyResult::Error(format!("Failed to initialize: {:?}", e)),
        }
    })
}

// Start the strategy
#[update]
async fn start() -> StrategyResult {
    // Verify the caller is the owner
    if let Err(e) = verify_owner() {
        return StrategyResult::Error(e);
    }
    
    // Verify the current status allows starting
    if let Err(e) = verify_status(&[StrategyStatus::Created, StrategyStatus::Paused]) {
        return StrategyResult::Error(e);
    }
    
    // TODO: Fetch initial price from exchange for the primary token
    // For now, we will use a placeholder value
    let initial_price = 100_000_000; // Placeholder value
    
    // Update the status to Running and set initial price
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();
        
        current_state.status = StrategyStatus::Running;
        current_state.initial_price = Some(initial_price);
        current_state.current_primary_price = Some(initial_price);
        
        match state.set(current_state) {
            Ok(_) => StrategyResult::Success,
            Err(e) => StrategyResult::Error(format!("Failed to start strategy: {:?}", e)),
        }
    })
}

// Pause the strategy
#[update]
fn pause() -> StrategyResult {
    // Verify the caller is the owner
    if let Err(e) = verify_owner() {
        return StrategyResult::Error(e);
    }
    
    // Verify the current status allows pausing
    if let Err(e) = verify_status(&[StrategyStatus::Running]) {
        return StrategyResult::Error(e);
    }
    
    // Update the status to Paused
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();
        current_state.status = StrategyStatus::Paused;
        
        match state.set(current_state) {
            Ok(_) => StrategyResult::Success,
            Err(e) => StrategyResult::Error(format!("Failed to pause strategy: {:?}", e)),
        }
    })
}

// Stop the strategy
#[update]
fn stop() -> StrategyResult {
    // Verify the caller is the owner
    if let Err(e) = verify_owner() {
        return StrategyResult::Error(e);
    }
    
    // Any status can be stopped
    
    // Update the status to Terminated
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();
        current_state.status = StrategyStatus::Terminated;
        
        match state.set(current_state) {
            Ok(_) => StrategyResult::Success,
            Err(e) => StrategyResult::Error(format!("Failed to stop strategy: {:?}", e)),
        }
    })
}

// Execute the strategy once
#[update]
async fn execute_once() -> StrategyResult {
    // Verify the caller is the owner
    if let Err(e) = verify_owner() {
        return StrategyResult::Error(e);
    }
    
    // Verify the current status allows execution
    if let Err(e) = verify_status(&[StrategyStatus::Running]) {
        return StrategyResult::Error(e);
    }
    
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();
        
        // Check if we have initial price
        let initial_price = match current_state.initial_price {
            Some(price) => price,
            None => return StrategyResult::Error("Initial price not set. Please restart the strategy.".to_string()),
        };
        
        // TODO: Fetch current market price from exchange
        // For now, simulate a price change
        let current_time = time();
        let new_price = initial_price + (current_state.execution_count as u128 * 1_000_000);
        
        // Calculate price change percentage
        let price_change_percentage = if initial_price > 0 {
            (new_price as f64 - initial_price as f64) / initial_price as f64
        } else {
            0.0
        };
        
        // Check if price change exceeds threshold
        let should_hedge = price_change_percentage.abs() >= current_state.config.price_change_threshold;
        
        if should_hedge {
            // TODO: Calculate hedge size based on hedge ratio and price change
            // TODO: Execute hedge trade
            // For now, just update the state
            current_state.hedge_position_size = (new_price as f64 * current_state.config.hedge_ratio) as u128;
        }
        
        // Update execution info
        current_state.execution_count += 1;
        current_state.last_execution = Some(current_time);
        current_state.current_primary_price = Some(new_price);
        
        // Update state with execution results
        match state.set(current_state) {
            Ok(_) => {
                if should_hedge {
                    StrategyResult::Error("Hedge execution not implemented yet".to_string())
                } else {
                    StrategyResult::Error("Price change below threshold, no hedging needed".to_string())
                }
            },
            Err(e) => StrategyResult::Error(format!("Failed to update state: {:?}", e)),
        }
    })
}

// Get the current status of the strategy
#[query]
fn get_status() -> StrategyStatus {
    STATE.with(|state| {
        let state = state.borrow();
        state.get().status.clone()
    })
}

// Get the full strategy state (for owner only)
#[query]
fn get_state() -> Result<SelfHedgingState, String> {
    if let Err(e) = verify_owner() {
        return Err(e);
    }
    
    STATE.with(|state| {
        let state = state.borrow();
        Ok(state.get().clone())
    })
}

// Pre-upgrade hook to preserve state during upgrades
#[pre_upgrade]
fn pre_upgrade() {
    // State is already stored in stable storage via StableCell
}

// Post-upgrade hook to restore state after upgrades
#[post_upgrade]
fn post_upgrade() {
    // State is already restored from stable storage via StableCell
} 