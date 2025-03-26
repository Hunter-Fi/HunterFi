use candid::{CandidType, Principal};
use ic_cdk::api::call::{call, CallResult};
use ic_cdk::api::management_canister::main::{create_canister, install_code, CanisterInstallMode, CanisterSettings, CreateCanisterArgument, InstallCodeArgument};
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
    DeploymentStep, DeploymentEvent, EnhancedDeploymentRecord, StrategyConfig,
};

use crate::state::{
    generate_deployment_id, get_fee, get_wasm_module, store_basic_deployment_record,
    update_deployment_status, store_strategy_metadata, update_refund_status, RefundStatus,
    ExtendedDeploymentRecord, get_deployment_record,
};
use crate::payment::{check_allowance, collect_fee, process_refund};

// Track deployment processing tasks
thread_local! {
    static DEPLOYMENT_TIMERS: RefCell<HashMap<String, TimerId>> = RefCell::new(HashMap::new());
}

/// Create a new deployment request with generic configuration type that implements StrategyConfig
pub async fn create_strategy_request<T: StrategyConfig + CandidType>(
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

/// Authorize payment for the deployment request
pub async fn authorize_deployment(deployment_id: &str) -> Result<(), String> {
    // Get deployment record
    let record = get_deployment_record(deployment_id)
        .ok_or_else(|| format!("Deployment record not found for ID: {}", deployment_id))?;
    
    // Ensure caller is owner
    let caller_principal = caller();
    if record.owner != caller_principal {
        return Err("Only the deployment owner can confirm authorization".to_string());
    }
    
    // Check current status
    if record.status != DeploymentStatus::PendingPayment {
        return Err(format!(
            "Invalid deployment status: {:?}, expected: {:?}", 
            record.status, 
            DeploymentStatus::PendingPayment
        ));
    }
    
    // Verify allowance is sufficient
    let has_sufficient_allowance = check_allowance(caller_principal, record.fee_amount).await?;
    
    if !has_sufficient_allowance {
        return Err(format!(
            "Insufficient allowance for deployment fee: {} e8s. Please approve spending on the ICP token canister.",
            record.fee_amount
        ));
    }
    
    // Update status to authorized
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::AuthorizationConfirmed, 
        None, 
        None
    )?;

    // Collect the fee
    if let Err(e) = collect_fee(deployment_id).await {
        return Err(e);
    }

    // Schedule automatic execution after authorization
    let deployment_id_clone = deployment_id.to_string();
    let timer_id = ic_cdk_timers::set_timer(Duration::from_secs(30), move || {
        let deployment_id = deployment_id_clone.clone();
        ic_cdk::spawn(async move {
            match execute_deployment(&deployment_id).await {
                Ok(_) => {
                    ic_cdk::println!("Automatic deployment executed successfully: {}", deployment_id);
                }
                Err(e) => {
                    ic_cdk::println!("Automatic deployment failed: {}: {}", deployment_id, e);
                }
            }
        });
    });
    
    // Store timer ID for possible cancellation
    DEPLOYMENT_TIMERS.with(|timers| {
        timers.borrow_mut().insert(deployment_id.to_string(), timer_id);
    });
    
    Ok(())
}

/// Cancel a pending deployment that has not been executed yet
pub fn cancel_deployment(deployment_id: &str) -> Result<(), String> {
    // Get deployment record
    let record = get_deployment_record(deployment_id)
        .ok_or_else(|| format!("Deployment record not found for ID: {}", deployment_id))?;
    
    // Ensure caller is owner
    let caller_principal = caller();
    if record.owner != caller_principal {
        return Err("Only the deployment owner can cancel".to_string());
    }
    
    // Check current status - only cancellable in certain states
    match record.status {
        DeploymentStatus::PendingPayment | 
        DeploymentStatus::AuthorizationConfirmed => {
            // Cancel any pending timer
            DEPLOYMENT_TIMERS.with(|timers| {
                if let Some(timer_id) = timers.borrow_mut().remove(deployment_id) {
                    ic_cdk_timers::clear_timer(timer_id);
                }
            });
            
            // Update status to failed
            update_deployment_status(
                deployment_id, 
                DeploymentStatus::DeploymentCancelled,
                None, 
                Some("Cancelled by owner".to_string())
            )?;
            
            Ok(())
        },
        _ => {
            Err(format!("Deployment in state {:?} cannot be cancelled", record.status))
        }
    }
}

/// Execute the deployment process
pub async fn execute_deployment(deployment_id: &str) -> Result<DeploymentResult, String> {
    // Clean up any pending timer
    DEPLOYMENT_TIMERS.with(|timers| {
        if let Some(timer_id) = timers.borrow_mut().remove(deployment_id) {
            ic_cdk_timers::clear_timer(timer_id);
        }
    });
    
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
            ).await;
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
            ).await;
        }
    };
    
    // Install code
    if let Err(err) = install_strategy_code(canister_id, wasm_module).await {
        return handle_deployment_failure(
            deployment_id,
            Some(canister_id),
            format!("Failed to install code: {}", err)
        ).await;
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
        ).await;
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
            ).await;
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

// Helper functions for deployment process

/// Handle failed deployment with appropriate cleanup
async fn handle_deployment_failure(
    deployment_id: &str,
    canister_id: Option<Principal>,
    error_message: String
) -> Result<DeploymentResult, String> {
    // Update status to failed
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::DeploymentFailed, 
        canister_id, 
        Some(error_message.clone())
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
    record: &ExtendedDeploymentRecord,
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