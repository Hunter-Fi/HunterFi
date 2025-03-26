use candid::{Principal, CandidType};
use ic_cdk::api::call::{call, CallResult};
use ic_cdk::api::management_canister::main::{create_canister, install_code, CanisterInstallMode, CanisterSettings,
    CreateCanisterArgument, InstallCodeArgument,
};
use ic_cdk::api::{caller, time};
use serde_bytes::ByteBuf;
use strategy_common::types::{
    DeploymentRecord, DeploymentRequest, DeploymentResult, DeploymentStatus, DCAConfig, 
    ValueAvgConfig, FixedBalanceConfig, LimitOrderConfig, SelfHedgingConfig, StrategyMetadata,
    StrategyStatus, StrategyType, TradingPair, TokenMetadata,
};

use crate::state::{
    generate_deployment_id, get_fee, get_wasm_module, store_basic_deployment_record, 
    update_deployment_status, store_strategy_metadata, update_refund_status, RefundStatus,
};
use crate::payment::{check_allowance, collect_fee, process_refund};

/// Create a new deployment request for DCA strategy
pub async fn create_dca_request(config: DCAConfig) -> Result<DeploymentRequest, String> {
    let owner = caller();
    let fee = get_fee();
    
    // Validate config
    if config.amount_per_execution == 0 {
        return Err("Amount per execution must be greater than 0".to_string());
    }
    
    if config.interval_secs == 0 {
        return Err("Interval must be greater than 0".to_string());
    }
    
    // Ensure the DCA WASM module exists
    let strategy_type = StrategyType::DollarCostAveraging;
    if get_wasm_module(strategy_type.clone()).is_none() {
        return Err("DCA strategy WASM module not found".to_string());
    }
    
    // Generate deployment ID
    let deployment_id = generate_deployment_id();
    
    // Serialize config
    let config_bytes = candid::encode_one(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    
    // Create and store deployment record
    let record = DeploymentRecord {
        deployment_id: deployment_id.clone(),
        strategy_type: strategy_type.clone(),
        owner,
        fee_amount: fee,
        request_time: time(),
        status: DeploymentStatus::PendingPayment,
        canister_id: None,
        config_data: ByteBuf::from(config_bytes),
        error_message: None,
        last_updated: time(),
    };
    
    store_basic_deployment_record(record);
    
    // Return deployment request info
    Ok(DeploymentRequest {
        deployment_id,
        fee_amount: fee,
        strategy_type,
    })
}

/// Create a new deployment request for Value Averaging strategy
pub async fn create_value_avg_request(config: ValueAvgConfig) -> Result<DeploymentRequest, String> {
    let owner = caller();
    let fee = get_fee();
    
    // Validate config
    if config.target_value_increase == 0 {
        return Err("Target value increase must be greater than 0".to_string());
    }
    
    if config.interval_secs == 0 {
        return Err("Interval must be greater than 0".to_string());
    }
    
    // Ensure the WASM module exists
    let strategy_type = StrategyType::ValueAveraging;
    if get_wasm_module(strategy_type.clone()).is_none() {
        return Err("Value Averaging strategy WASM module not found".to_string());
    }
    
    // Generate deployment ID
    let deployment_id = generate_deployment_id();
    
    // Serialize config
    let config_bytes = candid::encode_one(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    
    // Create and store deployment record
    let record = DeploymentRecord {
        deployment_id: deployment_id.clone(),
        strategy_type: strategy_type.clone(),
        owner,
        fee_amount: fee,
        request_time: time(),
        status: DeploymentStatus::PendingPayment,
        canister_id: None,
        config_data: ByteBuf::from(config_bytes),
        error_message: None,
        last_updated: time(),
    };
    
    store_basic_deployment_record(record);
    
    // Return deployment request info
    Ok(DeploymentRequest {
        deployment_id,
        fee_amount: fee,
        strategy_type,
    })
}

/// Create a new deployment request for Fixed Balance strategy
pub async fn create_fixed_balance_request(config: FixedBalanceConfig) -> Result<DeploymentRequest, String> {
    let owner = caller();
    let fee = get_fee();
    
    // Validate config
    if config.token_allocations.is_empty() {
        return Err("Token allocations cannot be empty".to_string());
    }
    
    if config.interval_secs == 0 {
        return Err("Interval must be greater than 0".to_string());
    }
    
    // Ensure the WASM module exists
    let strategy_type = StrategyType::FixedBalance;
    if get_wasm_module(strategy_type.clone()).is_none() {
        return Err("Fixed Balance strategy WASM module not found".to_string());
    }
    
    // Generate deployment ID
    let deployment_id = generate_deployment_id();
    
    // Serialize config
    let config_bytes = candid::encode_one(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    
    // Create and store deployment record
    let record = DeploymentRecord {
        deployment_id: deployment_id.clone(),
        strategy_type: strategy_type.clone(),
        owner,
        fee_amount: fee,
        request_time: time(),
        status: DeploymentStatus::PendingPayment,
        canister_id: None,
        config_data: ByteBuf::from(config_bytes),
        error_message: None,
        last_updated: time(),
    };
    
    store_basic_deployment_record(record);
    
    // Return deployment request info
    Ok(DeploymentRequest {
        deployment_id,
        fee_amount: fee,
        strategy_type,
    })
}

/// Create a new deployment request for Limit Order strategy
pub async fn create_limit_order_request(config: LimitOrderConfig) -> Result<DeploymentRequest, String> {
    let owner = caller();
    let fee = get_fee();
    
    // Validate config
    if config.amount == 0 {
        return Err("Amount must be greater than 0".to_string());
    }
    
    if config.price == 0 {
        return Err("Price must be greater than 0".to_string());
    }
    
    // Ensure the WASM module exists
    let strategy_type = StrategyType::LimitOrder;
    if get_wasm_module(strategy_type.clone()).is_none() {
        return Err("Limit Order strategy WASM module not found".to_string());
    }
    
    // Generate deployment ID
    let deployment_id = generate_deployment_id();
    
    // Serialize config
    let config_bytes = candid::encode_one(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    
    // Create and store deployment record
    let record = DeploymentRecord {
        deployment_id: deployment_id.clone(),
        strategy_type: strategy_type.clone(),
        owner,
        fee_amount: fee,
        request_time: time(),
        status: DeploymentStatus::PendingPayment,
        canister_id: None,
        config_data: ByteBuf::from(config_bytes),
        error_message: None,
        last_updated: time(),
    };
    
    store_basic_deployment_record(record);
    
    // Return deployment request info
    Ok(DeploymentRequest {
        deployment_id,
        fee_amount: fee,
        strategy_type,
    })
}

/// Create a new deployment request for Self Hedging strategy
pub async fn create_self_hedging_request(config: SelfHedgingConfig) -> Result<DeploymentRequest, String> {
    let owner = caller();
    let fee = get_fee();
    
    // Validate config
    if config.transaction_size == 0 {
        return Err("Transaction size must be greater than 0".to_string());
    }
    
    if config.check_interval_secs == 0 {
        return Err("Check interval must be greater than 0".to_string());
    }
    
    // Ensure the WASM module exists
    let strategy_type = StrategyType::SelfHedging;
    if get_wasm_module(strategy_type.clone()).is_none() {
        return Err("Self Hedging strategy WASM module not found".to_string());
    }
    
    // Generate deployment ID
    let deployment_id = generate_deployment_id();
    
    // Serialize config
    let config_bytes = candid::encode_one(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    
    // Create and store deployment record
    let record = DeploymentRecord {
        deployment_id: deployment_id.clone(),
        strategy_type: strategy_type.clone(),
        owner,
        fee_amount: fee,
        request_time: time(),
        status: DeploymentStatus::PendingPayment,
        canister_id: None,
        config_data: ByteBuf::from(config_bytes),
        error_message: None,
        last_updated: time(),
    };
    
    store_basic_deployment_record(record);
    
    // Return deployment request info
    Ok(DeploymentRequest {
        deployment_id,
        fee_amount: fee,
        strategy_type,
    })
}

/// Confirm deployment authorization
pub async fn confirm_deployment_authorization(deployment_id: &str) -> Result<(), String> {
    // Get deployment record
    let record = crate::state::get_deployment_record(deployment_id)
        .ok_or_else(|| format!("Deployment record not found for ID: {}", deployment_id))?;
    
    // Verify caller is the owner
    let caller_principal = caller();
    if record.owner != caller_principal {
        return Err("Only the owner can confirm this deployment".to_string());
    }
    
    // Verify deployment status
    if record.status != DeploymentStatus::PendingPayment {
        return Err(format!(
            "Deployment is in invalid state: {:?}. Expected: {:?}", 
            record.status, 
            DeploymentStatus::PendingPayment
        ));
    }
    
    // Check authorization (allowance)
    let has_allowance = check_allowance(caller_principal, record.fee_amount).await?;
    
    if !has_allowance {
        return Err(format!(
            "Insufficient allowance. Please approve at least {} e8s from your account to the factory canister.",
            record.fee_amount
        ));
    }
    
    // Update status to authorization confirmed
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::AuthorizationConfirmed, 
        None, 
        None
    )?;
    
    // Start deployment process in the background
    let deployment_id = deployment_id.to_string();
    ic_cdk::spawn(async move {
        let _ = execute_deployment(&deployment_id).await;
    });
    
    Ok(())
}

/// Execute the deployment process
pub async fn execute_deployment(deployment_id: &str) -> Result<DeploymentResult, String> {
    // Collect the fee
    if let Err(e) = collect_fee(deployment_id).await {
        return Err(e);
    }
    
    // Get deployment record
    let record = crate::state::get_deployment_record(deployment_id)
        .ok_or_else(|| format!("Deployment record not found for ID: {}", deployment_id))?;
    
    // Create canister
    let canister_id = match create_strategy_canister().await {
        Ok(cid) => {
            // Update status
            update_deployment_status(
                deployment_id, 
                DeploymentStatus::CanisterCreated, 
                Some(cid), 
                None
            )?;
            cid
        },
        Err(err) => {
            // Update status to failed
            update_deployment_status(
                deployment_id, 
                DeploymentStatus::DeploymentFailed, 
                None, 
                Some(format!("Failed to create canister: {}", err))
            )?;
            
            // Mark for refund but don't process immediately
            update_refund_status(
                deployment_id,
                RefundStatus::NotStarted
            )?;
            
            // Schedule refund processing asynchronously
            let deployment_id = deployment_id.to_string();
            ic_cdk::spawn(async move {
                let _ = process_refund(&deployment_id).await;
            });
            
            return Err(format!("Failed to create canister: {}", err));
        }
    };
    
    // Get WASM module
    let wasm_module = match get_wasm_module(record.strategy_type.clone()) {
        Some(wasm) => wasm,
        None => {
            // Update status to failed
            update_deployment_status(
                deployment_id, 
                DeploymentStatus::DeploymentFailed, 
                Some(canister_id), 
                Some(format!("WASM module not found for strategy type: {:?}", record.strategy_type))
            )?;
            
            // Mark for refund but don't process immediately
            update_refund_status(
                deployment_id,
                RefundStatus::NotStarted
            )?;
            
            // Schedule refund processing asynchronously
            let deployment_id = deployment_id.to_string();
            ic_cdk::spawn(async move {
                let _ = process_refund(&deployment_id).await;
            });
            
            return Err(format!("WASM module not found for strategy type: {:?}", record.strategy_type));
        }
    };
    
    // Install code
    if let Err(err) = install_strategy_code(canister_id, wasm_module).await {
        // Update status to failed
        update_deployment_status(
            deployment_id, 
            DeploymentStatus::DeploymentFailed, 
            Some(canister_id), 
            Some(format!("Failed to install code: {}", err))
        )?;
        
        // Mark for refund but don't process immediately
        update_refund_status(
            deployment_id,
            RefundStatus::NotStarted
        )?;
        
        // Schedule refund processing asynchronously
        let deployment_id = deployment_id.to_string();
        ic_cdk::spawn(async move {
            let _ = process_refund(&deployment_id).await;
        });
        
        return Err(format!("Failed to install code: {}", err));
    }
    
    // Update status
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::CodeInstalled, 
        Some(canister_id), 
        None
    )?;
    
    // Initialize based on strategy type
    let init_result = match record.strategy_type {
        StrategyType::DollarCostAveraging => {
            let config = candid::decode_one::<DCAConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode DCA config: {}", e))?;
            
            initialize_dca_strategy(canister_id, record.owner, config).await
        },
        StrategyType::ValueAveraging => {
            let config = candid::decode_one::<ValueAvgConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Value Averaging config: {}", e))?;
            
            initialize_value_avg_strategy(canister_id, record.owner, config).await
        },
        StrategyType::FixedBalance => {
            let config = candid::decode_one::<FixedBalanceConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Fixed Balance config: {}", e))?;
            
            initialize_fixed_balance_strategy(canister_id, record.owner, config).await
        },
        StrategyType::LimitOrder => {
            let config = candid::decode_one::<LimitOrderConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Limit Order config: {}", e))?;
            
            initialize_limit_order_strategy(canister_id, record.owner, config).await
        },
        StrategyType::SelfHedging => {
            let config = candid::decode_one::<SelfHedgingConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Self Hedging config: {}", e))?;
            
            initialize_self_hedging_strategy(canister_id, record.owner, config).await
        },
    };
    
    if let Err(err) = init_result {
        // Update status to failed
        update_deployment_status(
            deployment_id, 
            DeploymentStatus::DeploymentFailed, 
            Some(canister_id), 
            Some(format!("Failed to initialize strategy: {}", err))
        )?;
        
        // Mark for refund but don't process immediately
        update_refund_status(
            deployment_id,
            RefundStatus::NotStarted
        )?;
        
        // Schedule refund processing asynchronously
        let deployment_id = deployment_id.to_string();
        ic_cdk::spawn(async move {
            let _ = process_refund(&deployment_id).await;
        });
        
        return Err(format!("Failed to initialize strategy: {}", err));
    }
    
    // Update status to initialized
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::Initialized, 
        Some(canister_id), 
        None
    )?;
    
    // Create and store strategy metadata
    let metadata = match record.strategy_type {
        StrategyType::DollarCostAveraging => {
            let config = candid::decode_one::<DCAConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode DCA config: {}", e))?;
            
            StrategyMetadata {
                canister_id,
                strategy_type: StrategyType::DollarCostAveraging,
                owner: record.owner,
                created_at: time(),
                status: StrategyStatus::Created,
                exchange: config.exchange,
                trading_pair: TradingPair {
                    base_token: config.base_token,
                    quote_token: config.quote_token,
                },
            }
        },
        StrategyType::ValueAveraging => {
            let config = candid::decode_one::<ValueAvgConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Value Averaging config: {}", e))?;
            
            StrategyMetadata {
                canister_id,
                strategy_type: StrategyType::ValueAveraging,
                owner: record.owner,
                created_at: time(),
                status: StrategyStatus::Created,
                exchange: config.exchange,
                trading_pair: TradingPair {
                    base_token: config.base_token,
                    quote_token: config.quote_token,
                },
            }
        },
        StrategyType::FixedBalance => {
            let config = candid::decode_one::<FixedBalanceConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Fixed Balance config: {}", e))?;
            
            // Get first token for base token
            let base_token = config.token_allocations.keys().next()
                .expect("Fixed Balance strategy must have at least one token allocation")
                .clone();
            
            // Use ICP as quote token
            let quote_token = TokenMetadata {
                canister_id: Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai").unwrap(),
                symbol: "ICP".to_string(),
                decimals: 8,
            };
            
            StrategyMetadata {
                canister_id,
                strategy_type: StrategyType::FixedBalance,
                owner: record.owner,
                created_at: time(),
                status: StrategyStatus::Created,
                exchange: config.exchange,
                trading_pair: TradingPair {
                    base_token,
                    quote_token,
                },
            }
        },
        StrategyType::LimitOrder => {
            let config = candid::decode_one::<LimitOrderConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Limit Order config: {}", e))?;
            
            StrategyMetadata {
                canister_id,
                strategy_type: StrategyType::LimitOrder,
                owner: record.owner,
                created_at: time(),
                status: StrategyStatus::Created,
                exchange: config.exchange,
                trading_pair: TradingPair {
                    base_token: config.base_token,
                    quote_token: config.quote_token,
                },
            }
        },
        StrategyType::SelfHedging => {
            let config = candid::decode_one::<SelfHedgingConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Self Hedging config: {}", e))?;
            
            StrategyMetadata {
                canister_id,
                strategy_type: StrategyType::SelfHedging,
                owner: record.owner,
                created_at: time(),
                status: StrategyStatus::Created,
                exchange: config.exchange,
                trading_pair: TradingPair {
                    base_token: config.trading_token.clone(),
                    quote_token: config.trading_token.clone(),
                },
            }
        },
    };
    
    // Store metadata
    store_strategy_metadata(metadata);
    
    // Update status to deployed
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::Deployed, 
        Some(canister_id), 
        None
    )?;
    
    Ok(DeploymentResult::Success(canister_id))
}

// Helper functions

/// Create a new strategy canister
async fn create_strategy_canister() -> Result<Principal, String> {
    let settings = CanisterSettings {
        controllers: Some(vec![ic_cdk::id()]),
        compute_allocation: None,
        memory_allocation: None,
        freezing_threshold: None,
        reserved_cycles_limit: None,
        log_visibility: None,
        wasm_memory_limit: None,
    };
    
    let args = CreateCanisterArgument {
        settings: Some(settings),
    };
    
    let result = create_canister(args, 0u128).await;
    
    match result {
        Ok((record,)) => Ok(record.canister_id),
        Err((code, msg)) => Err(format!("Error code: {:?}, message: {}", code, msg)),
    }
}

/// Install strategy code on a canister
async fn install_strategy_code(
    canister_id: Principal,
    wasm_module: Vec<u8>,
) -> Result<(), String> {
    let args = InstallCodeArgument {
        mode: CanisterInstallMode::Install,
        canister_id,
        wasm_module,
        arg: vec![],
    };
    
    let result = install_code(args).await;
    
    match result {
        Ok(()) => Ok(()),
        Err((code, msg)) => Err(format!("Error code: {:?}, message: {}", code, msg)),
    }
}

/// Initialize DCA strategy
async fn initialize_dca_strategy(
    canister_id: Principal,
    owner: Principal,
    config: DCAConfig,
) -> Result<(), String> {
    let call_result: CallResult<()> = call(
        canister_id,
        "init_dca",
        (owner, config),
    ).await;
    
    match call_result {
        Ok(_) => Ok(()),
        Err((code, msg)) => Err(format!(
            "Failed to initialize DCA strategy: code={:?}, message={}",
            code, msg
        )),
    }
}

/// Initialize Value Averaging strategy
async fn initialize_value_avg_strategy(
    canister_id: Principal,
    owner: Principal,
    config: ValueAvgConfig,
) -> Result<(), String> {
    let call_result: CallResult<()> = call(
        canister_id,
        "init_value_avg",
        (owner, config),
    ).await;
    
    match call_result {
        Ok(_) => Ok(()),
        Err((code, msg)) => Err(format!(
            "Failed to initialize Value Averaging strategy: code={:?}, message={}",
            code, msg
        )),
    }
}

/// Initialize Fixed Balance strategy
async fn initialize_fixed_balance_strategy(
    canister_id: Principal,
    owner: Principal,
    config: FixedBalanceConfig,
) -> Result<(), String> {
    let call_result: CallResult<()> = call(
        canister_id,
        "init_fixed_balance",
        (owner, config),
    ).await;
    
    match call_result {
        Ok(_) => Ok(()),
        Err((code, msg)) => Err(format!(
            "Failed to initialize Fixed Balance strategy: code={:?}, message={}",
            code, msg
        )),
    }
}

/// Initialize Limit Order strategy
async fn initialize_limit_order_strategy(
    canister_id: Principal,
    owner: Principal,
    config: LimitOrderConfig,
) -> Result<(), String> {
    let call_result: CallResult<()> = call(
        canister_id,
        "init_limit_order",
        (owner, config),
    ).await;
    
    match call_result {
        Ok(_) => Ok(()),
        Err((code, msg)) => Err(format!(
            "Failed to initialize Limit Order strategy: code={:?}, message={}",
            code, msg
        )),
    }
}

/// Initialize Self Hedging strategy
async fn initialize_self_hedging_strategy(
    canister_id: Principal,
    owner: Principal,
    config: SelfHedgingConfig,
) -> Result<(), String> {
    let call_result: CallResult<()> = call(
        canister_id,
        "init_self_hedging",
        (owner, config),
    ).await;
    
    match call_result {
        Ok(_) => Ok(()),
        Err((code, msg)) => Err(format!(
            "Failed to initialize Self Hedging strategy: code={:?}, message={}",
            code, msg
        )),
    }
} 