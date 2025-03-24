use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::{caller, canister_balance, time};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap, StableCell};
use ic_stable_structures::storable::Storable;
use std::cell::RefCell;
use strategy_common::types::{
    OrderSplitType, SelfHedgingConfig, StrategyResult, StrategyStatus,
};
use strategy_common::timer::{self, TimerConfig};

// Type definitions for stable storage
type Memory = VirtualMemory<DefaultMemoryImpl>;

// Constant for timer ID
const EXECUTION_TIMER_ID: &str = "self_hedging_execution";

// State structure
#[derive(CandidType, Deserialize, Clone, Debug)]
struct SelfHedgingState {
    owner: Principal,
    config: SelfHedgingConfig,
    status: StrategyStatus,
    last_execution: Option<u64>,
    execution_count: u64,
    volume_generated: u128,
    order_split_type: OrderSplitType,
    transaction_size: u128,
}

// Implement Storable for SelfHedgingState
impl Storable for SelfHedgingState {
    const BOUND: ic_stable_structures::storable::Bound = ic_stable_structures::storable::Bound::Unbounded;
    
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        let bytes = candid::encode_one(self).unwrap();
        std::borrow::Cow::Owned(bytes)
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        candid::decode_one(&bytes).unwrap()
    }
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
                        trading_token: strategy_common::types::TokenMetadata {
                            canister_id: Principal::anonymous(),
                            symbol: "".to_string(),
                            decimals: 0,
                        },
                        transaction_size: 0,
                        order_split_type: OrderSplitType::NoSplit,
                        check_interval_secs: 0,
                        slippage_tolerance: 0.0,
                    },
                    status: StrategyStatus::Created,
                    last_execution: None,
                    execution_count: 0,
                    volume_generated: 0,
                    order_split_type: OrderSplitType::NoSplit,
                    transaction_size: 0,
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
        if config.transaction_size == 0 {
            return StrategyResult::Error("Transaction size must be greater than zero".to_string());
        }
        
        if config.check_interval_secs == 0 {
            return StrategyResult::Error("Check interval cannot be zero".to_string());
        }
        
        // Create new state
        let new_state = SelfHedgingState {
            owner,
            config: config.clone(),
            status: StrategyStatus::Created,
            last_execution: None,
            execution_count: 0,
            volume_generated: 0,
            order_split_type: config.order_split_type.clone(),
            transaction_size: config.transaction_size,
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
    
    // Get the check interval from config
    let check_interval = STATE.with(|state| {
        let state = state.borrow();
        state.get().config.check_interval_secs
    });
    
    // Set up a timer for automatic execution
    let timer_config = TimerConfig {
        id: EXECUTION_TIMER_ID.to_string(),
        interval_seconds: check_interval,
        enabled: true,
    };
    
    // Setup the periodic execution timer
    timer::set_timer(timer_config, || {
        ic_cdk::spawn(async {
            let _ = execute_once().await;
        });
    });
    
    // Update the status to Running
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();
        
        current_state.status = StrategyStatus::Running;
        
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
    
    // Clear the execution timer
    timer::clear_timer(EXECUTION_TIMER_ID);
    
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
    
    // Clear the execution timer
    timer::clear_timer(EXECUTION_TIMER_ID);
    
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
    // Anyone can trigger execution if the strategy is running
    // We'll check the status to ensure it's running
    if let Err(e) = verify_status(&[StrategyStatus::Running]) {
        return StrategyResult::Error(e);
    }
    
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();
        
        // Get current canister balance to ensure we don't exceed available funds
        let canister_balance_value = canister_balance();
        
        // Get the transaction size, limited by available balance
        let transaction_size = std::cmp::min(current_state.transaction_size, canister_balance_value as u128);
        
        if transaction_size == 0 {
            return StrategyResult::Error("Insufficient balance for self-trading".to_string());
        }
        
        // Execute trade based on order split type
        match current_state.order_split_type {
            OrderSplitType::NoSplit => {
                // Execute a single buy and sell order
                execute_trade(transaction_size, false, false);
            },
            OrderSplitType::SplitBuy => {
                // Split buy orders into multiple smaller orders
                execute_trade(transaction_size, true, false);
            },
            OrderSplitType::SplitSell => {
                // Split sell orders into multiple smaller orders
                execute_trade(transaction_size, false, true);
            },
            OrderSplitType::SplitBoth => {
                // Split both buy and sell orders
                execute_trade(transaction_size, true, true);
            }
        }
        
        // Update execution info
        let current_time = time();
        current_state.execution_count += 1;
        current_state.last_execution = Some(current_time);
        current_state.volume_generated += transaction_size * 2; // Both buy and sell count toward volume
        
        // Update state with execution results
        match state.set(current_state) {
            Ok(_) => StrategyResult::Success,
            Err(e) => StrategyResult::Error(format!("Failed to update state: {:?}", e)),
        }
    })
}

// Helper function to execute trades with optional splitting
fn execute_trade(amount: u128, split_buy: bool, split_sell: bool) {
    // TODO: Integrate with exchange API to execute actual trades
    
    if split_buy {
        // Split buy order into 3-5 smaller orders
        let num_splits = 3 + (time() % 3) as usize; // Random between 3-5
        let base_amount = amount / num_splits as u128;
        let remainder = amount % num_splits as u128;
        
        for i in 0..num_splits {
            let split_amount = if i == num_splits - 1 {
                base_amount + remainder // Add remainder to last split
            } else {
                base_amount
            };
            
            // Execute buy order with split_amount
            // TODO: Call exchange API to place buy order
        }
    } else {
        // Execute single buy order
        // TODO: Call exchange API to place buy order with full amount
    }
    
    if split_sell {
        // Split sell order into 3-5 smaller orders
        let num_splits = 3 + (time() % 3) as usize; // Random between 3-5
        let base_amount = amount / num_splits as u128;
        let remainder = amount % num_splits as u128;
        
        for i in 0..num_splits {
            let split_amount = if i == num_splits - 1 {
                base_amount + remainder // Add remainder to last split
            } else {
                base_amount
            };
            
            // Execute sell order with split_amount
            // TODO: Call exchange API to place sell order
        }
    } else {
        // Execute single sell order
        // TODO: Call exchange API to place sell order with full amount
    }
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

#[update]
fn update_volume_config(transaction_size: u128, split_type: OrderSplitType) -> StrategyResult {
    // Verify the caller is the owner
    if let Err(e) = verify_owner() {
        return StrategyResult::Error(e);
    }
    
    // Only allow update if not running
    if let Err(e) = verify_status(&[StrategyStatus::Created, StrategyStatus::Paused]) {
        return StrategyResult::Error(format!("Cannot update configuration while strategy is running. Please pause first: {}", e));
    }
    
    if transaction_size == 0 {
        return StrategyResult::Error("Transaction size must be greater than zero".to_string());
    }
    
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();
        
        // Update configuration
        let mut updated_config = current_state.config.clone();
        updated_config.transaction_size = transaction_size;
        updated_config.order_split_type = split_type.clone();
        
        // Update state
        current_state.config = updated_config;
        current_state.transaction_size = transaction_size;
        current_state.order_split_type = split_type;
        
        match state.set(current_state) {
            Ok(_) => StrategyResult::Success,
            Err(e) => StrategyResult::Error(format!("Failed to update volume configuration: {:?}", e)),
        }
    })
}

#[derive(CandidType, Deserialize, Clone, Debug)]
struct VolumeStats {
    total_volume: u128,
    execution_count: u64,
    last_execution: Option<u64>,
    transaction_size: u128,
    split_type: OrderSplitType,
    token_symbol: String,
}

#[query]
fn get_volume_stats() -> VolumeStats {
    STATE.with(|state| {
        let state = state.borrow();
        let current_state = state.get();
        
        VolumeStats {
            total_volume: current_state.volume_generated,
            execution_count: current_state.execution_count,
            last_execution: current_state.last_execution,
            transaction_size: current_state.transaction_size,
            split_type: current_state.order_split_type.clone(),
            token_symbol: current_state.config.trading_token.symbol.clone(),
        }
    })
} 