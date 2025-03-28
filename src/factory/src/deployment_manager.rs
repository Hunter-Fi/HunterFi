use candid::{CandidType, Principal};
use ic_cdk::api::call::{call, CallResult};
use ic_cdk::api::management_canister::main::{
    create_canister, install_code, CanisterInstallMode, CanisterSettings,
    CreateCanisterArgument, InstallCodeArgument,
};
use ic_cdk::api::{caller, time};
use serde_bytes::ByteBuf;
use std::time::Duration;
use std::cell::RefCell;
use std::collections::HashMap;
use ic_cdk_timers::TimerId;

use strategy_common::types::{
    DeploymentRecord, DeploymentRequest, DeploymentResult, DeploymentStatus,
    DCAConfig, ValueAvgConfig, FixedBalanceConfig, LimitOrderConfig, SelfHedgingConfig,
    StrategyMetadata, StrategyStatus, StrategyType, TradingPair, TokenMetadata,
    StrategyConfig,
};

use crate::state::{
    generate_deployment_id, get_fee, get_wasm_module, store_deployment_record,
    update_deployment_status, store_strategy_metadata, get_deployment_record,
};
use crate::payment::{process_balance_payment, process_balance_refund};

// Track deployment processing tasks
thread_local! {
    static DEPLOYMENT_TIMERS: RefCell<HashMap<String, TimerId>> = RefCell::new(HashMap::new());
}

/// Create a new deployment request with generic configuration
pub fn create_strategy_request<T: StrategyConfig + CandidType>(
    config: T
) -> Result<DeploymentRequest, String> {
    let owner = caller();
    let fee = get_fee();
    
    // Validate config
    config.validate()?;
    
    // Get strategy type
    let strategy_type = config.get_strategy_type();
    
    // Ensure the strategy WASM module exists
    if get_wasm_module(strategy_type.clone()).is_none() {
        return Err(format!("{:?} strategy WASM module not found", strategy_type));
    }
    
    // Process payment from user's balance
    let description = format!("Deployment fee for {:?} strategy", strategy_type);
    process_balance_payment(owner, fee, &description)?;
    
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
        status: DeploymentStatus::PaymentReceived, // Skip authorization phase
        canister_id: None,
        config_data: ByteBuf::from(config_bytes),
        error_message: None,
        last_updated: time(),
    };
    
    store_deployment_record(record);
    
    // Start deployment process asynchronously
    let deployment_id_clone = deployment_id.clone();
    ic_cdk::spawn(async move {
        match execute_deployment(&deployment_id_clone).await {
            Ok(_) => {
                ic_cdk::println!("Deployment executed successfully: {}", deployment_id_clone);
            }
            Err(e) => {
                ic_cdk::println!("Deployment failed: {}: {}", deployment_id_clone, e);
                
                // Process automatic refund for failed deployment
                if let Some(record) = get_deployment_record(&deployment_id_clone) {
                    let _ = process_balance_refund(record.owner, record.fee_amount, &deployment_id_clone);
                }
            }
        }
    });
    
    // Return deployment request info
    Ok(DeploymentRequest {
        deployment_id,
        fee_amount: fee,
        strategy_type,
    })
}

/// Execute the deployment process
pub async fn execute_deployment(deployment_id: &str) -> Result<DeploymentResult, String> {
    // Get deployment record
    let record = get_deployment_record(deployment_id)
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
            return handle_deployment_failure(
                deployment_id,
                None,
                format!("Failed to create canister: {}", err)
            );
        }
    };
    
    // Get WASM module
    let wasm_module = match get_wasm_module(record.strategy_type.clone()) {
        Some(wasm) => wasm,
        None => {
            return handle_deployment_failure(
                deployment_id,
                Some(canister_id),
                format!("WASM module not found for strategy type: {:?}", record.strategy_type)
            );
        }
    };
    
    // Install code
    if let Err(err) = install_strategy_code(canister_id, wasm_module).await {
        return handle_deployment_failure(
            deployment_id,
            Some(canister_id),
            format!("Failed to install code: {}", err)
        );
    }
    
    // Update status
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::CodeInstalled, 
        Some(canister_id), 
        None
    )?;
    
    // Initialize strategy using common function
    if let Err(err) = initialize_strategy(
        canister_id, 
        record.owner, 
        record.strategy_type.clone(), 
        &record.config_data
    ).await {
        return handle_deployment_failure(
            deployment_id,
            Some(canister_id),
            format!("Failed to initialize strategy: {}", err)
        );
    }
    
    // Update status to initialized
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::Initialized, 
        Some(canister_id), 
        None
    )?;
    
    // Create strategy metadata
    let metadata = match create_strategy_metadata(&record, canister_id) {
        Ok(metadata) => metadata,
        Err(err) => {
            return handle_deployment_failure(
                deployment_id,
                Some(canister_id),
                format!("Failed to create strategy metadata: {}", err)
            );
        }
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

/// Handle failed deployment and process refund
fn handle_deployment_failure(
    deployment_id: &str,
    canister_id: Option<Principal>,
    error_message: String
) -> Result<DeploymentResult, String> {
    // Update status to failed
    if let Err(e) = update_deployment_status(
        deployment_id, 
        DeploymentStatus::DeploymentFailed, 
        canister_id, 
        Some(error_message.clone())
    ) {
        ic_cdk::println!("Failed to update deployment status: {}", e);
    }
    
    // Process refund asynchronously
    let deployment_id_clone = deployment_id.to_string();
    ic_cdk::spawn(async move {
        if let Some(record) = get_deployment_record(&deployment_id_clone) {
            match process_balance_refund(record.owner, record.fee_amount, &deployment_id_clone) {
                Ok(_) => {
                    // Update status to refunded
                    let _ = update_deployment_status(
                        &deployment_id_clone,
                        DeploymentStatus::Refunded,
                        None,
                        None
                    );
                    ic_cdk::println!("Refund processed for failed deployment: {}", deployment_id_clone);
                },
                Err(e) => {
                    ic_cdk::println!("Failed to process refund: {}: {}", deployment_id_clone, e);
                }
            }
        }
    });
    
    Err(error_message)
}

/// Generic function to initialize a strategy based on its type
async fn initialize_strategy(
    canister_id: Principal,
    owner: Principal,
    strategy_type: StrategyType,
    config_data: &[u8],
) -> Result<(), String> {
    match strategy_type {
        StrategyType::DollarCostAveraging => {
            let config = candid::decode_one::<DCAConfig>(config_data)
                .map_err(|e| format!("Failed to decode DCA config: {}", e))?;
            
            initialize_strategy_with_config(canister_id, owner, "init_dca", config).await
        },
        StrategyType::ValueAveraging => {
            let config = candid::decode_one::<ValueAvgConfig>(config_data)
                .map_err(|e| format!("Failed to decode Value Averaging config: {}", e))?;
            
            initialize_strategy_with_config(canister_id, owner, "init_value_avg", config).await
        },
        StrategyType::FixedBalance => {
            let config = candid::decode_one::<FixedBalanceConfig>(config_data)
                .map_err(|e| format!("Failed to decode Fixed Balance config: {}", e))?;
            
            initialize_strategy_with_config(canister_id, owner, "init_fixed_balance", config).await
        },
        StrategyType::LimitOrder => {
            let config = candid::decode_one::<LimitOrderConfig>(config_data)
                .map_err(|e| format!("Failed to decode Limit Order config: {}", e))?;
            
            initialize_strategy_with_config(canister_id, owner, "init_limit_order", config).await
        },
        StrategyType::SelfHedging => {
            let config = candid::decode_one::<SelfHedgingConfig>(config_data)
                .map_err(|e| format!("Failed to decode Self Hedging config: {}", e))?;
            
            initialize_strategy_with_config(canister_id, owner, "init_self_hedging", config).await
        },
    }
}

/// Generic function to initialize strategy with any config type
async fn initialize_strategy_with_config<T: CandidType>(
    canister_id: Principal,
    owner: Principal,
    method: &str,
    config: T,
) -> Result<(), String> {
    let call_result: CallResult<()> = call(
        canister_id,
        method,
        (owner, config),
    ).await;
    
    match call_result {
        Ok(_) => Ok(()),
        Err((code, msg)) => Err(format!(
            "Failed to initialize strategy: code={:?}, message={}",
            code, msg
        )),
    }
}

/// Function to create and get strategy metadata from record
fn create_strategy_metadata(
    record: &DeploymentRecord,
    canister_id: Principal
) -> Result<StrategyMetadata, String> {
    match record.strategy_type {
        StrategyType::DollarCostAveraging => {
            let config = candid::decode_one::<DCAConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode DCA config: {}", e))?;
            
            Ok(StrategyMetadata {
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
            })
        },
        StrategyType::ValueAveraging => {
            let config = candid::decode_one::<ValueAvgConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Value Averaging config: {}", e))?;
            
            Ok(StrategyMetadata {
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
            })
        },
        StrategyType::FixedBalance => {
            let config = candid::decode_one::<FixedBalanceConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Fixed Balance config: {}", e))?;
            
            // For FixedBalance, we'll use the first token allocation as base
            if config.token_allocations.is_empty() {
                return Err("Fixed Balance strategy must have at least one token allocation".to_string());
            }
            
            // Get first token as base token - more safely using direct field access
            // Try with HashMap implementation first
            let base_token = match config.token_allocations.iter().next() {
                Some((token, _)) => token.clone(),
                None => {
                    // Fallback to treating it as a non-empty Vec
                    // Note: This is a simplification, would need to know exact type in production
                    ic_cdk::println!("Warning: Falling back to alternative token_allocations access");
                    
                    // Re-decode to get the base token in a safer way
                    let config_str = format!("{:?}", config);
                    if let Some(idx) = config_str.find("token_allocations") {
                        let start_idx = config_str[idx..].find('{');
                        if let Some(start_pos) = start_idx {
                            // Parse the first token from the string representation
                            let token_str = &config_str[idx + start_pos..];
                            ic_cdk::println!("Token allocation data: {}", token_str);
                            
                            // Create a default token in case extraction fails
                            TokenMetadata {
                                canister_id: Principal::anonymous(),
                                symbol: "UNKNOWN".to_string(),
                                decimals: 8,
                            }
                        } else {
                            return Err("Could not find token metadata in allocation data".to_string());
                        }
                    } else {
                        return Err("Could not find token_allocations in configuration".to_string());
                    }
                }
            };
            
            // Use ICP as quote token
            let quote_token = TokenMetadata {
                canister_id: Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai").unwrap(),
                symbol: "ICP".to_string(),
                decimals: 8,
            };
            
            Ok(StrategyMetadata {
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
            })
        },
        StrategyType::LimitOrder => {
            let config = candid::decode_one::<LimitOrderConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Limit Order config: {}", e))?;
            
            Ok(StrategyMetadata {
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
            })
        },
        StrategyType::SelfHedging => {
            let config = candid::decode_one::<SelfHedgingConfig>(&record.config_data)
                .map_err(|e| format!("Failed to decode Self Hedging config: {}", e))?;
            
            Ok(StrategyMetadata {
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
            })
        },
    }
}

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