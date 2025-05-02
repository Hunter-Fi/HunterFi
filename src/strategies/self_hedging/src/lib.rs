use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::{caller, canister_balance, time};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap, StableCell};
use ic_stable_structures::storable::Storable;
use std::cell::RefCell;
use strategy_common::types::{
    OrderSplitType, SelfHedgingConfig, StrategyResult, StrategyStatus, TradingPair, TokenMetadata
};
use strategy_common::timer::{self, TimerConfig};
use exchange::{types as exchange_types, LiquidityPool, TokenInfo};
use exchange::error as exchange_error;
use exchange::icpswap::ICPSwapConnector;
use exchange::traits::{Exchange, Trading, TokenOperations};

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
    base_token_unused_balance: u128,
    quote_token_unused_balance: u128,
    last_balance_check: Option<u64>,
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
                        trading_pair: TradingPair {
                            base_token: TokenMetadata {
                                canister_id: Principal::anonymous(),
                                symbol: "".to_string(),
                                decimals: 0,
                                standard: "".to_string(),
                                fee: 0,
                            },
                            quote_token: TokenMetadata {
                                canister_id: Principal::anonymous(),
                                symbol: "".to_string(),
                                decimals: 0,
                                standard: "".to_string(),
                                fee: 0,
                            },
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
                    base_token_unused_balance: 0,
                    quote_token_unused_balance: 0,
                    last_balance_check: None,
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

// Helper function to convert TokenMetadata to TokenInfo
// Placed near other helper functions like create_trade_params
fn create_token_info(metadata: &TokenMetadata) -> exchange_types::TokenInfo {
    exchange_types::TokenInfo {
        canister_id: metadata.canister_id,
        symbol: metadata.symbol.clone(),
        decimals: metadata.decimals, // No clone needed for u8 (Copy trait)
        standard: token_standard_from_string(&metadata.standard),
    }
}

// Initialize the Self-Hedging strategy
#[update]
async fn init_self_hedging(owner: Principal, config: SelfHedgingConfig) -> StrategyResult {
    let caller_id = caller();

    ic_cdk::println!("Starting init_self_hedging: caller={}, owner={}", caller_id, owner);
    STATE.with(|state_cell| {
        let mut state_ref_mut = state_cell.borrow_mut();
        let current_state = state_ref_mut.get().clone();

        if current_state.owner != Principal::anonymous() {
            ic_cdk::println!("Error: Strategy already initialized. Current owner: {}", current_state.owner);
            return StrategyResult::Error("Strategy already initialized".to_string());
        }

        ic_cdk::println!("Validating configuration: transaction_size={}, check_interval={}",
                        config.transaction_size, config.check_interval_secs);

        if config.transaction_size == 0 {
            ic_cdk::println!("Error: Transaction size must be greater than zero");
            return StrategyResult::Error("Transaction size must be greater than zero".to_string());
        }

        if config.check_interval_secs == 0 {
            ic_cdk::println!("Error: Check interval cannot be zero");
            return StrategyResult::Error("Check interval cannot be zero".to_string());
        }

        if config.trading_pair.base_token.canister_id == Principal::anonymous() ||
           config.trading_pair.quote_token.canister_id == Principal::anonymous() {
            ic_cdk::println!("Error: Invalid token canister IDs in trading pair");
            return StrategyResult::Error("Invalid token canister IDs in trading pair".to_string());
        }

        if config.slippage_tolerance <= 0.0 || config.slippage_tolerance >= 1.0 {
             ic_cdk::println!("Error: Slippage tolerance must be between 0 and 1 (exclusive)");
            return StrategyResult::Error("Slippage tolerance must be between 0 and 1 (exclusive)".to_string());
        }

        let new_state = SelfHedgingState {
            owner,
            config: config.clone(),
            status: StrategyStatus::Created,
            last_execution: None,
            execution_count: 0,
            volume_generated: 0,
            order_split_type: config.order_split_type.clone(),
            transaction_size: config.transaction_size,
            base_token_unused_balance: 0,
            quote_token_unused_balance: 0,
            last_balance_check: None,
        };

        ic_cdk::println!("Saving new state with owner: {}", owner);

        match state_ref_mut.set(new_state) {
            Ok(_) => {
                ic_cdk::println!("Initialization successful");
                StrategyResult::Success
            },
            Err(e) => {
                ic_cdk::println!("Error saving state: {:?}", e);
                StrategyResult::Error(format!("Failed to initialize: {:?}", e))
            },
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

    // Get the current state
    let state_data = STATE.with(|state| state.borrow().get().clone());

    // --- Start: Added Pool Info Fetching and Token Approval ---
    ic_cdk::println!("Fetching pool info and approving tokens...");
    let connector = create_icpswap_connector(&state_data.config.exchange);
    let base_token_info = create_token_info(&state_data.config.trading_pair.base_token);
    let quote_token_info = create_token_info(&state_data.config.trading_pair.quote_token);

    // Get pool info from ICPSwap connector
    // Assuming the connector trait/impl provides get_pool_info and approve_token
    // And PoolData struct with pool_id field
    let pool_data = match connector.get_pool_info(&base_token_info, &quote_token_info).await {
         Ok(data) => {
             ic_cdk::println!("Successfully fetched pool info: {:?}", data);
             data
         },
         Err(e) => {
             let error_msg = format!("Failed to get pool info: {:?}", e);
             ic_cdk::println!("{}", error_msg);
             return StrategyResult::Error(error_msg);
         },
    };

    // Approve base token for the pool
    ic_cdk::println!("Approving base token ({}) for pool {}", base_token_info.symbol, pool_data.pool_id);
    match connector.approve_token(&base_token_info, &pool_data.pool_id, u128::MAX).await {
        Ok(_) => {
             ic_cdk::println!("Base token approved successfully.");
        },
        Err(e) => {
             let error_msg = format!("Failed to approve base token: {:?}", e);
             ic_cdk::println!("{}", error_msg);
            return StrategyResult::Error(error_msg);
        },
    }

    // Approve quote token for the pool
    ic_cdk::println!("Approving quote token ({}) for pool {}", quote_token_info.symbol, pool_data.pool_id);
    match connector.approve_token(&quote_token_info, &pool_data.pool_id, u128::MAX).await {
         Ok(_) => {
              ic_cdk::println!("Quote token approved successfully.");
         },
         Err(e) => {
             let error_msg = format!("Failed to approve quote token: {:?}", e);
             ic_cdk::println!("{}", error_msg);
             return StrategyResult::Error(error_msg);
         },
    }
    ic_cdk::println!("Token approvals completed.");
    // --- End: Added Pool Info Fetching and Token Approval ---


    // Check if the balance in ICPSwap is sufficient (Existing balance check logic)
    ic_cdk::println!("Checking ICPSwap balance...");
    let (base_balance, quote_balance) = match check_icpswap_balance().await {
        Ok((base, quote)) => {
            ic_cdk::println!("ICPSwap balance check successful: Base={}, Quote={}", base, quote);
            (base, quote)
        },
        Err(e) => {
             let error_msg = format!("Failed to check exchange balance: {}", e);
             ic_cdk::println!("{}", error_msg);
            return StrategyResult::Error(error_msg);
        },
    };

    // Check if there is enough balance to start the strategy
    if base_balance == 0 && quote_balance == 0 {
         let error_msg = "No balance available in ICPSwap. Please deposit tokens first.".to_string();
         ic_cdk::println!("Error starting strategy: {}", error_msg);
        return StrategyResult::Error(error_msg);
    }

    // Get the check interval from config
    let check_interval = state_data.config.check_interval_secs;

    // Set up a timer for automatic execution
    let timer_config = TimerConfig {
        id: EXECUTION_TIMER_ID.to_string(),
        interval_seconds: check_interval,
        enabled: true,
    };

    // Setup the periodic execution timer
    ic_cdk::println!("Setting up execution timer with interval: {}s", check_interval);
    timer::set_timer(timer_config, || {
        ic_cdk::spawn(async {
            ic_cdk::println!("Execution timer triggered.");
            let result = execute_once().await;
            ic_cdk::println!("execute_once result: {:?}", result); // Log execution result
        });
    });

    // Update the status to Running
    ic_cdk::println!("Updating strategy status to Running...");
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();

        current_state.status = StrategyStatus::Running;

        match state.set(current_state) {
            Ok(_) => {
                ic_cdk::println!("Strategy started successfully.");
                StrategyResult::Success
            },
            Err(e) => {
                 let error_msg = format!("Failed to set state to Running: {:?}", e);
                 ic_cdk::println!("Error starting strategy: {}", error_msg);
                 StrategyResult::Error(error_msg)
            },
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

    ic_cdk::println!("execute_once------");
    // Get the current state
    let state_data = STATE.with(|state| state.borrow().get().clone());

    // Check if the previous execution has completed
    if let Some(last_execution) = state_data.last_execution {
        // If last execution time + 5 seconds > current time, it might still be in progress
        if last_execution + 5 > time() {
            return StrategyResult::Error("Previous execution may still be in progress".to_string());
        }
    }

    // Check the available balance in ICPSwap
    let (base_balance, quote_balance) = match check_icpswap_balance().await {
        Ok((base, quote)) => (base, quote),
        Err(e) => return StrategyResult::Error(format!("Failed to check exchange balance: {}", e)),
    };

    // Check if the balance is sufficient to execute the strategy
    // If the balance is less than 5% of the single transaction amount, pause the strategy
    let min_amount = state_data.transaction_size / 20; // 5%
    if base_balance < min_amount && quote_balance < min_amount {
        // Automatically pause the strategy
        match pause() {
            StrategyResult::Success => {
                return StrategyResult::Error("Strategy paused: insufficient balance (less than 5% of transaction size)".to_string());
            },
            StrategyResult::Error(e) => {
                return StrategyResult::Error(format!("Strategy should be paused due to insufficient balance, but failed: {}", e));
            }
        }
    }

    // Determine the trading direction (buy or sell)
    // Determine the initial trade direction based on whether the base token is token0 or token1 and which has a larger balance
    let (use_base_token, amount) = if base_balance >= state_data.transaction_size {
        (true, state_data.transaction_size)
    } else if quote_balance >= state_data.transaction_size {
        (false, state_data.transaction_size)
    } else {
        // Neither balance is sufficient for the full transaction size, use the larger balance
        if base_balance > quote_balance {
            (true, base_balance)
        } else {
            (false, quote_balance)
        }
    };

    // Generate split orders
    let split_count = get_split_order_count(state_data.transaction_size as usize);
    let split_amounts = split_amount(amount, split_count);

    // Execute trades
    match execute_hedge_trades(use_base_token, split_amounts, state_data.order_split_type.clone()).await {
        Ok(volume) => {
            // Update state
            update_state_after_execution(volume).await
        },
        Err(e) => StrategyResult::Error(format!("Failed to execute hedge trades: {}", e))
    }
}

// Check the available balance in ICPSwap
async fn check_icpswap_balance() -> Result<(u128, u128), String> {
    let state_data = STATE.with(|state| state.borrow().get().clone());
    
    // Check for cached balance, return directly if available and not older than 30 seconds
    if let Some(last_check) = state_data.last_balance_check {
        if last_check + 30 > time() && 
           (state_data.base_token_unused_balance > 0 || state_data.quote_token_unused_balance > 0) {
            return Ok((state_data.base_token_unused_balance, state_data.quote_token_unused_balance));
        }
    }
    
    // Create ICPSwap connector
    let connector = create_icpswap_connector(&state_data.config.exchange);
    
    // Create TradeParams
    let params = create_trade_params(&state_data.config);
    
    // Query unused balance
    let user = ic_cdk::id(); // Current canister ID
    match connector.get_unused_balance(&params, &user).await {
        Ok((token0_balance, token1_balance,token0)) => {
            // Return balance
            // Ensure the returned order is base_token, quote_token
            let is_base_token0 = state_data.config.trading_pair.base_token.canister_id.to_string().eq(&token0.to_string());
            
            if is_base_token0 {
                // Update unused balance in the state here
                update_unused_balance(token0_balance, token1_balance).await;
                Ok((token0_balance, token1_balance))
            } else {
                // Update unused balance in the state here
                update_unused_balance(token1_balance,token0_balance).await;
                Ok((token1_balance, token0_balance))
            }
        },
        Err(e) => Err(format!("Failed to get unused balance: {:?}", e))
    }
}

// Update unused balance
async fn update_unused_balance(base_balance: u128, quote_balance: u128) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();
        current_state.base_token_unused_balance = base_balance;
        current_state.quote_token_unused_balance = quote_balance;
        
        current_state.last_balance_check = Some(time());
        
        let _ = state.set(current_state);
    });
}

// Create ICPSwap connector
fn create_icpswap_connector(exchange: &strategy_common::types::Exchange) -> ICPSwapConnector {
    // ICPSwap factory Canister ID (mainnet)
    let factory_canister_id = Principal::from_text("4mmnk-kiaaa-aaaag-qbllq-cai")
        .expect("Failed to parse ICPSwap factory canister ID");
    
    // Create exchange configuration
    let exchange_config = exchange_types::ExchangeConfig {
        exchange_type: exchange_types::ExchangeType::ICPSwap,
        canister_id: factory_canister_id,
        default_slippage: 0.5,
        max_slippage: 1.0,
        timeout_secs: 30,
        retry_count: 3,
    };
    
    // Create connector
    ICPSwapConnector::new(exchange_config)
}

// Create TradeParams
fn create_trade_params(config: &SelfHedgingConfig) -> exchange_types::TradeParams {
    // Convert base_token to exchange module's TokenInfo
    let base_token = exchange_types::TokenInfo {
        canister_id: config.trading_pair.base_token.canister_id,
        symbol: config.trading_pair.base_token.symbol.clone(),
        decimals: config.trading_pair.base_token.decimals,
        standard: token_standard_from_string(&config.trading_pair.base_token.standard),
    };
    
    // Convert quote_token to exchange module's TokenInfo
    let quote_token = exchange_types::TokenInfo {
        canister_id: config.trading_pair.quote_token.canister_id,
        symbol: config.trading_pair.quote_token.symbol.clone(),
        decimals: config.trading_pair.quote_token.decimals,
        standard: token_standard_from_string(&config.trading_pair.quote_token.standard),
    };
    
    // Create trading pair
    let trading_pair = exchange_types::TradingPair {
        base_token,
        quote_token,
        exchange: exchange_types::ExchangeType::ICPSwap,
    };
    
    // Create TradeParams (direction defaults to Buy, will be determined based on balance during execution)
    exchange_types::TradeParams {
        pair: trading_pair,
        direction: exchange_types::TradeDirection::Buy,
        amount: config.transaction_size,
        slippage_tolerance: config.slippage_tolerance,
        deadline_secs: None,
    }
}

// Convert token standard string to TokenStandard enum
fn token_standard_from_string(standard: &str) -> exchange_types::TokenStandard {
    match standard.to_uppercase().as_str() {
        "ICRC1" => exchange_types::TokenStandard::ICRC1,
        "ICRC2" => exchange_types::TokenStandard::ICRC2,
        "DIP20" => exchange_types::TokenStandard::DIP20,
        "EXT" => exchange_types::TokenStandard::EXT,
        "ICP" => exchange_types::TokenStandard::ICP,
        _ => exchange_types::TokenStandard::ICRC1, // Defaults to ICRC1
    }
}

// Calculate the number of orders to split into
fn get_split_order_count(base_count: usize) -> usize {
    let random_addition = (time() % 3) as usize;
    base_count + random_addition
}

// Split amount into multiple small orders
fn split_amount(total_amount: u128, split_count: usize) -> Vec<u128> {
    if split_count <= 1 {
        return vec![total_amount];
    }
    
    let mut result = Vec::with_capacity(split_count);
    let base_amount = total_amount / split_count as u128;
    let remainder = total_amount % split_count as u128;
    
    for i in 0..split_count {
        let amount = if i == split_count - 1 {
            base_amount + remainder // Add remainder to the last order
        } else {
            base_amount
        };
        result.push(amount);
    }
    
    result
}

// Execute hedge trades
async fn execute_hedge_trades(
    use_base_token: bool, 
    split_amounts: Vec<u128>, 
    split_type: OrderSplitType
) -> Result<u128, String> {
    let state_data = STATE.with(|state| state.borrow().get().clone());
    let connector = create_icpswap_connector(&state_data.config.exchange);
    let mut params = create_trade_params(&state_data.config);
    
    let mut total_volume = 0u128;
    
    // First stage trade - sell initial token
    let should_split_first = match split_type {
        OrderSplitType::NoSplit => false,
        OrderSplitType::SplitBuy => !use_base_token, // If selling quote_token (buying base_token) and set to split buy
        OrderSplitType::SplitSell => use_base_token, // If selling base_token and set to split sell
        OrderSplitType::SplitBoth => true,          // Always split
    };
    
    // Set trade direction
    params.direction = if use_base_token {
        exchange_types::TradeDirection::Sell // Sell base_token to get quote_token
    } else {
        exchange_types::TradeDirection::Buy // Buy base_token using quote_token
    };
    
    // Execute first stage trade
    let mut first_stage_outputs = Vec::new();
    
    if should_split_first {
        // Split execute multiple orders
        for amount in &split_amounts {
            params.amount = *amount;
            match connector.execute_trade(&params).await {
                Ok(result) => {
                    total_volume += result.input_amount;
                    first_stage_outputs.push(result.output_amount);
                },
                Err(e) => return Err(format!("First stage trade failed: {:?}", e)),
            }
        }
    } else {
        // Single execute
        params.amount = split_amounts.iter().sum();
        match connector.execute_trade(&params).await {
            Ok(result) => {
                total_volume += result.input_amount;
                first_stage_outputs.push(result.output_amount);
            },
            Err(e) => return Err(format!("First stage trade failed: {:?}", e)),
        }
    }
    
    // Query updated unused balance
    let _ = check_icpswap_balance().await;
    
    // Second stage trade - reverse trade (sell the token just obtained)
    // Reverse trade direction
    params.direction = match params.direction {
        exchange_types::TradeDirection::Buy => exchange_types::TradeDirection::Sell,
        exchange_types::TradeDirection::Sell => exchange_types::TradeDirection::Buy,
    };
    
    let should_split_second = match split_type {
        OrderSplitType::NoSplit => false,
        OrderSplitType::SplitBuy => use_base_token, // If now buying base_token and set to split buy
        OrderSplitType::SplitSell => !use_base_token, // If now selling base_token and set to split sell
        OrderSplitType::SplitBoth => true,          // Always split
    };
    
    if should_split_second {
        // Calculate how to split second stage trade
        let mut second_stage_amounts = Vec::new();
        if first_stage_outputs.len() == 1 {
            // If first stage is single trade, split first stage output
            let total_output = first_stage_outputs[0];
            second_stage_amounts = split_amount(total_output, split_amounts.len());
        } else {
            // If first stage is multiple trades, use each output of first stage as input for second stage
            second_stage_amounts = first_stage_outputs;
        }
        
        // Split execute multiple orders
        for amount in &second_stage_amounts {
            params.amount = *amount;
            match connector.execute_trade(&params).await {
                Ok(result) => {
                    total_volume += result.input_amount;
                },
                Err(e) => return Err(format!("Second stage trade failed: {:?}", e)),
            }
        }
    } else {
        // Single execute
        params.amount = first_stage_outputs.iter().sum();
        match connector.execute_trade(&params).await {
            Ok(result) => {
                total_volume += result.input_amount;
            },
            Err(e) => return Err(format!("Second stage trade failed: {:?}", e)),
        }
    }
    
    // Query updated unused balance again
    let _ = check_icpswap_balance().await;
    
    Ok(total_volume)
}

// Update state after execution
async fn update_state_after_execution(volume: u128) -> StrategyResult {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();

        // Update execution information
        let current_time = time();
        current_state.execution_count += 1;
        current_state.last_execution = Some(current_time);
        current_state.volume_generated += volume;

        // Update state
        match state.set(current_state) {
            Ok(_) => StrategyResult::Success,
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
            token_symbol: current_state.config.trading_pair.base_token.symbol.clone(),
        }
    })
}

// Update configuration information
#[update]
fn update_config(
    transaction_size: u128,
    split_type: OrderSplitType,
    check_interval_secs: u64,
    slippage_tolerance: f64,
) -> StrategyResult {
    // Verify the caller is the owner
    if let Err(e) = verify_owner() {
        return StrategyResult::Error(e);
    }

    // Only allow update if not running
    if let Err(e) = verify_status(&[StrategyStatus::Created, StrategyStatus::Paused]) {
        return StrategyResult::Error(format!("Cannot update configuration while strategy is running. Please pause first: {}", e));
    }

    // Validate parameters
    if transaction_size == 0 {
        return StrategyResult::Error("Transaction size must be greater than zero".to_string());
    }

    if check_interval_secs == 0 {
        return StrategyResult::Error("Check interval cannot be zero".to_string());
    }

    if slippage_tolerance <= 0.0 || slippage_tolerance > 100.0 {
        return StrategyResult::Error("Slippage tolerance must be between 0 and 100".to_string());
    }

    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let mut current_state = state.get().clone();

        // Update configuration
        let mut updated_config = current_state.config.clone();
        updated_config.transaction_size = transaction_size;
        updated_config.order_split_type = split_type.clone();
        updated_config.check_interval_secs = check_interval_secs;
        updated_config.slippage_tolerance = slippage_tolerance;

        // Update state
        current_state.config = updated_config;
        current_state.transaction_size = transaction_size;
        current_state.order_split_type = split_type;

        match state.set(current_state) {
            Ok(_) => StrategyResult::Success,
            Err(e) => StrategyResult::Error(format!("Failed to update configuration: {:?}", e)),
        }
    })
}

// Deposit to ICPSwap
#[update]
async fn deposit_to_exchange(token_type: String, amount: u128) -> StrategyResult {
    // Verify the caller is the owner
    if let Err(e) = verify_owner() {
        return StrategyResult::Error(e);
    }

    let state_data = STATE.with(|state| state.borrow().get().clone());
    let connector = create_icpswap_connector(&state_data.config.exchange);
    let params = create_trade_params(&state_data.config);

    // Determine which token to deposit
    let token = if token_type.to_lowercase() == "base" {
        &params.pair.base_token
    } else if token_type.to_lowercase() == "quote" {
        &params.pair.quote_token
    } else {
        return StrategyResult::Error("Invalid token type. Must be 'base' or 'quote'".to_string());
    };

    // Deposit token to ICPSwap
    match connector.deposit_token(&params, token, amount).await {
        Ok(deposited_amount) => {
            // Update unused balance
            let _ = check_icpswap_balance().await;
            StrategyResult::Success
        },
        Err(e) => StrategyResult::Error(format!("Failed to deposit token: {:?}", e)),
    }
}

// Withdraw from ICPSwap
#[update]
async fn withdraw_from_exchange(token_type: String, amount: u128) -> StrategyResult {
    // Verify the caller is the owner
    if let Err(e) = verify_owner() {
        return StrategyResult::Error(e);
    }

    let state_data = STATE.with(|state| state.borrow().get().clone());
    let connector = create_icpswap_connector(&state_data.config.exchange);
    let params = create_trade_params(&state_data.config);

    // Determine which token to withdraw
    let token = if token_type.to_lowercase() == "base" {
        &params.pair.base_token
    } else if token_type.to_lowercase() == "quote" {
        &params.pair.quote_token
    } else {
        return StrategyResult::Error("Invalid token type. Must be 'base' or 'quote'".to_string());
    };

    // Check unused balance is sufficient
    let (base_balance, quote_balance) = match check_icpswap_balance().await {
        Ok((base, quote)) => (base, quote),
        Err(e) => return StrategyResult::Error(format!("Failed to check exchange balance: {}", e)),
    };

    let available_balance = if token_type.to_lowercase() == "base" {
        base_balance
    } else {
        quote_balance
    };

    if amount > available_balance {
        return StrategyResult::Error(format!("Insufficient unused balance. Available: {}, Requested: {}", available_balance, amount));
    }

    // Withdraw from ICPSwap
    match connector.withdraw_token(&params, token, amount).await {
        Ok(withdrawn_amount) => {
            // Update unused balance
            let _ = check_icpswap_balance().await;
            StrategyResult::Success
        },
        Err(e) => StrategyResult::Error(format!("Failed to withdraw token: {:?}", e)),
    }
}

// Get balance information
#[derive(CandidType, Deserialize, Clone, Debug)]
struct BalanceInfo {
    base_token_unused: u128,
    quote_token_unused: u128,
    canister_balance: u128,
    last_update: Option<u64>,
}

// Get balance information
#[query]
fn get_balance_info() -> BalanceInfo {
    let state_data = STATE.with(|state| state.borrow().get().clone());
    let canister_balance_value = canister_balance();

    BalanceInfo {
        base_token_unused: state_data.base_token_unused_balance,
        quote_token_unused: state_data.quote_token_unused_balance,
        canister_balance: canister_balance_value as u128,
        last_update: state_data.last_balance_check,
    }
}

// Refresh balance information (get latest balance from ICPSwap)
#[update]
async fn refresh_balance() -> StrategyResult {
    match check_icpswap_balance().await {
        Ok(_) => StrategyResult::Success,
        Err(e) => StrategyResult::Error(format!("Failed to refresh balance: {}", e)),
    }
}

// Get trading pair information
#[derive(CandidType, Deserialize, Clone, Debug)]
struct TradingPairInfo {
    base_token_symbol: String,
    base_token_decimals: u8,
    base_token_canister: Principal,
    quote_token_symbol: String,
    quote_token_decimals: u8,
    quote_token_canister: Principal,
}

// Get trading pair information
#[query]
fn get_trading_pair_info() -> TradingPairInfo {
    let state_data = STATE.with(|state| state.borrow().get().clone());

    TradingPairInfo {
        base_token_symbol: state_data.config.trading_pair.base_token.symbol.clone(),
        base_token_decimals: state_data.config.trading_pair.base_token.decimals,
        base_token_canister: state_data.config.trading_pair.base_token.canister_id,
        quote_token_symbol: state_data.config.trading_pair.quote_token.symbol.clone(),
        quote_token_decimals: state_data.config.trading_pair.quote_token.decimals,
        quote_token_canister: state_data.config.trading_pair.quote_token.canister_id,
    }
}

// Get strategy configuration information
#[derive(CandidType, Deserialize, Clone, Debug)]
struct StrategyConfigInfo {
    transaction_size: u128,
    order_split_type: OrderSplitType,
    check_interval_secs: u64,
    slippage_tolerance: f64,
}

// Get strategy configuration information
#[query]
fn get_strategy_config() -> StrategyConfigInfo {
    let state_data = STATE.with(|state| state.borrow().get().clone());

    StrategyConfigInfo {
        transaction_size: state_data.config.transaction_size,
        order_split_type: state_data.config.order_split_type,
        check_interval_secs: state_data.config.check_interval_secs,
        slippage_tolerance: state_data.config.slippage_tolerance,
    }
} 