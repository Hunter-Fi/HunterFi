use candid::{CandidType, Principal};
use ic_cdk::api::call::{call, CallResult};
use ic_cdk::api::management_canister::main::{
    create_canister, install_code, CanisterInstallMode, CanisterSettings,
    CreateCanisterArgument, InstallCodeArgument,
    UpdateSettingsArgument,
};
use ic_cdk::api::{caller, time};
use serde_bytes::ByteBuf;
use std::cell::RefCell;
use std::collections::HashMap;
use ic_cdk_timers::TimerId;
use ic_cdk::id;
use ic_cdk::trap;

// Use include_bytes! macro to directly embed WASM files
// Note: Currently only self_hedging strategy WASM is available
const SELF_HEDGING_WASM: &[u8] = include_bytes!("../../../target/wasm32-unknown-unknown/release/strategy_self_hedging.wasm");

// Other strategy files don't exist yet, using mock data (minimal valid WASM header)
const MOCK_WASM_HEADER: &[u8] = &[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]; // Minimal WASM header

use strategy_common::types::{
    DeploymentRecord, DeploymentRequest, DeploymentStatus,
    DCAConfig, ValueAvgConfig, FixedBalanceConfig, LimitOrderConfig, SelfHedgingConfig,
    StrategyMetadata, StrategyStatus, StrategyType, TradingPair, TokenMetadata,
    StrategyConfig,
};

use crate::state::{
    generate_deployment_id, get_fee, store_deployment_record,
    update_deployment_status, store_strategy_metadata, get_deployment_record,
    WasmModule,
};
use crate::payment::{process_balance_payment, process_balance_refund, PaymentError, payment_error_to_string};

// Deployment error types
#[derive(Debug)]
pub enum DeploymentError {
    ConfigError(String),
    PaymentError(PaymentError),
    ModuleNotFound(StrategyType),
    CanisterCreationFailed(String),
    CodeInstallationFailed(String),
    InitializationFailed(String),
    MetadataError(String),
    RecordNotFound(String),
    SystemError(String),
}

impl DeploymentError {
    pub fn to_string(&self) -> String {
        match self {
            DeploymentError::ConfigError(msg) => format!("Configuration error: {}", msg),
            DeploymentError::PaymentError(err) => format!("Payment error: {:?}", err),
            DeploymentError::ModuleNotFound(strategy_type) => format!("{:?} strategy WASM module not found", strategy_type),
            DeploymentError::CanisterCreationFailed(msg) => format!("Canister creation failed: {}", msg),
            DeploymentError::CodeInstallationFailed(msg) => format!("Code installation failed: {}", msg),
            DeploymentError::InitializationFailed(msg) => format!("Strategy initialization failed: {}", msg),
            DeploymentError::MetadataError(msg) => format!("Metadata error: {}", msg),
            DeploymentError::RecordNotFound(msg) => format!("Record not found: {}", msg),
            DeploymentError::SystemError(msg) => format!("System error: {}", msg),
        }
    }
}

// Deployment result type alias
#[allow(dead_code)]
type DeploymentProcessResult<T> = Result<T, DeploymentError>;

// Track deployment processing tasks
thread_local! {
    static DEPLOYMENT_TIMERS: RefCell<HashMap<String, TimerId>> = RefCell::new(HashMap::new());
}

// Result mapping helper for deployment operations
fn map_deployment_error<T, E: std::fmt::Display>(
    result: Result<T, E>, 
    error_type: fn(String) -> DeploymentError
) -> DeploymentProcessResult<T> {
    result.map_err(|e| error_type(e.to_string()))
}

// Helper to create a deployment record
fn create_deployment_record<T: StrategyConfig + CandidType>(
    deployment_id: &str,
    strategy_type: StrategyType,
    owner: Principal,
    fee: u64,
    config: &T
) -> Result<DeploymentRecord, DeploymentError> {
    // Serialize config
    let config_bytes = candid::encode_one(config)
        .map_err(|e| DeploymentError::ConfigError(format!("Failed to serialize config: {}", e)))?;
    
    // Create deployment record
    let record = DeploymentRecord {
        deployment_id: deployment_id.to_string(),
        strategy_type,
        owner,
        fee_amount: fee,
        request_time: time(),
        status: DeploymentStatus::PaymentReceived,
        canister_id: None,
        config_data: ByteBuf::from(config_bytes),
        error_message: None,
        last_updated: time(),
    };
    
    Ok(record)
}

/// Create a new deployment request with generic configuration
pub async fn create_strategy_request<T: StrategyConfig + CandidType>(
    config: T
) -> Result<DeploymentRequest, String> {
    let owner = caller();
    let fee = get_fee();
    
    // Validate config
    if let Err(e) = config.validate() {
        return Err(DeploymentError::ConfigError(e).to_string());
    }
    
    // Get strategy type
    let strategy_type = config.get_strategy_type();
    
    // Ensure there's an available strategy WASM module
    let wasm = get_embedded_wasm_module(strategy_type.clone());
    
    // Check if WASM module is available
    // Currently only Self-Hedging strategy has real WASM, others use mock
    if wasm.is_none() || strategy_type != StrategyType::SelfHedging {
        return Err(DeploymentError::ModuleNotFound(strategy_type).to_string());
    }
    
    // Process payment from user's balance
    let description = format!("Deployment fee for {:?} strategy", strategy_type);
    if let Err(e) = process_balance_payment(owner, fee, &description).await {
        return Err(DeploymentError::PaymentError(e).to_string());
    }
    
    // Generate deployment ID
    let deployment_id = generate_deployment_id();
    
    // Create and store deployment record
    let record = match create_deployment_record(&deployment_id, strategy_type.clone(), owner, fee, &config) {
        Ok(record) => record,
        Err(e) => return Err(e.to_string()),
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
                    if let Err(refund_err) = process_balance_refund(record.owner, record.fee_amount, &deployment_id_clone).await {
                        ic_cdk::println!("Refund failed for deployment {}: {}", 
                            deployment_id_clone, payment_error_to_string(refund_err));
                    }
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
        .ok_or_else(|| DeploymentError::RecordNotFound(
            format!("Deployment record not found for ID: {}", deployment_id)
        ).to_string())?;
    
    // Create canister
    let canister_id = match create_strategy_canister().await {
        Ok(cid) => {
            ic_cdk::println!("Deployment create_strategy_canister successfully: {}", cid);
            // Update status
            update_deployment_status(
                deployment_id, 
                DeploymentStatus::CanisterCreated, 
                Some(cid), 
                None
            ).map_err(|e| DeploymentError::SystemError(e).to_string())?;
            cid
        },
        Err(err) => {
            ic_cdk::println!("Deployment create_strategy_canister error: {}", err);
            return handle_deployment_failure(
                deployment_id,
                None,
                DeploymentError::CanisterCreationFailed(err).to_string()
            ).await;
        }
    };
    
    // Get WASM module: first try to retrieve from state, if not found use embedded module
    let wasm_module = match get_embedded_wasm_module(record.strategy_type.clone()) {
        Some(wasm) => {
            // Only Self-Hedging strategy has real WASM implementation
            if record.strategy_type != StrategyType::SelfHedging {
                return handle_deployment_failure(
                    deployment_id,
                    Some(canister_id),
                    DeploymentError::ModuleNotFound(record.strategy_type).to_string()
                ).await;
            }
            wasm
        },
        None => {
            return handle_deployment_failure(
                deployment_id,
                Some(canister_id),
                DeploymentError::ModuleNotFound(record.strategy_type).to_string()
            ).await;
        }
    };
    
    // Install code
    if let Err(err) = install_strategy_code(canister_id, wasm_module).await {
        return handle_deployment_failure(
            deployment_id,
            Some(canister_id),
            DeploymentError::CodeInstallationFailed(err).to_string()
        ).await;
    }
    
    // Update status
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::CodeInstalled, 
        Some(canister_id), 
        None
    ).map_err(|e| DeploymentError::SystemError(e).to_string())?;
    
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
            DeploymentError::InitializationFailed(err).to_string()
        ).await;
    }
    
    // Update status to initialized
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::Initialized, 
        Some(canister_id), 
        None
    ).map_err(|e| DeploymentError::SystemError(e).to_string())?;
    
    // Create strategy metadata
    let metadata = match create_strategy_metadata(&record, canister_id) {
        Ok(metadata) => metadata,
        Err(err) => {
            return handle_deployment_failure(
                deployment_id,
                Some(canister_id),
                DeploymentError::MetadataError(err).to_string()
            ).await;
        }
    };
    
    ic_cdk::println!("Storing strategy metadata: canister_id={}", metadata.canister_id);
    store_strategy_metadata(metadata.clone());
    ic_cdk::println!("Strategy metadata stored successfully");
    
    // Update status to deployed
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::Deployed, 
        Some(canister_id), 
        None
    ).map_err(|e| DeploymentError::SystemError(e).to_string())?;
    
    // Return successful result
    Ok(DeploymentResult::Success(canister_id))
}

/// Handle failed deployment and process refund
async fn handle_deployment_failure(
    deployment_id: &str,
    canister_id: Option<Principal>,
    error_message: String
) -> Result<DeploymentResult, String> {
    // Update deployment status to failed
    update_deployment_status(
        deployment_id,
        DeploymentStatus::DeploymentFailed,
        canister_id,
        Some(error_message.clone())
    ).map_err(|e| format!("Failed to update deployment status: {}", e))?;
    
    // Get deployment record for refund
    let record = get_deployment_record(deployment_id)
        .ok_or_else(|| format!("Deployment record not found for ID: {}", deployment_id))?;
    
    // Process refund
    if let Err(e) = process_balance_refund(record.owner, record.fee_amount, deployment_id).await {
        ic_cdk::println!("Failed to process refund for deployment {}: {}", deployment_id, e);
        // Continue with failure result even if refund fails
    }
    
    // Return failure result
    Ok(DeploymentResult::Failure(error_message))
}

/// Generic function to initialize a strategy based on its type
async fn initialize_strategy(
    canister_id: Principal,
    owner: Principal,
    strategy_type: StrategyType,
    config_data: &[u8],
) -> Result<(), String> {
    ic_cdk::println!("Start initializing strategy: canister_id={}, type={:?}", canister_id, strategy_type);
    
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
            ic_cdk::println!("Decoding Self Hedging config, data size: {} bytes", config_data.len());
            let config = candid::decode_one::<SelfHedgingConfig>(config_data)
                .map_err(|e| {
                    ic_cdk::println!("Failed to decode Self Hedging config: {}", e);
                    format!("Failed to decode Self Hedging config: {}", e)
                })?;
            
            ic_cdk::println!("Self Hedging config decoded successfully: transaction_size={}, check_interval={}", 
                          config.transaction_size, config.check_interval_secs);
            
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
    ic_cdk::println!("Initializing strategy: canister_id={}, owner={}, method={}", 
                    canister_id, owner, method);
    
    let call_result: CallResult<()> = call(
        canister_id,
        method,
        (owner, config),
    ).await;
    
    match call_result {
        Ok(_) => {
            ic_cdk::println!("Strategy initialization successful: canister_id={}, method={}", 
                           canister_id, method);
            Ok(())
        },
        Err((code, msg)) => {
            ic_cdk::println!("Strategy initialization failed: canister_id={}, method={}, code={:?}, message={}", 
                           canister_id, method, code, msg);
            Err(format!(
                "Failed to initialize strategy: code={:?}, message={}",
                code, msg
            ))
        },
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
    
    // Provide enough cycles for canister creation and initial operation
    // Canister creation requires 38,461,538,461 cycles plus additional cycles for initialization
    let creation_cycles: u128 = 40_000_000_000 + 60_000_000_000; // Creation fee + initial running cycles
    
    // Ensure factory canister has sufficient cycles
    let current_balance: u128 = ic_cdk::api::canister_balance() as u128;
    if current_balance < creation_cycles {
        return Err(format!(
            "Factory canister balance insufficient to create new canister. Current balance: {} cycles, required: {} cycles",
            current_balance, creation_cycles
        ));
    }
    
    ic_cdk::println!("Creating new strategy canister with {} cycles", creation_cycles);
    let result = create_canister(args, creation_cycles).await;
    
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
        Ok(()) => {
            ic_cdk::println!("Deployment install_strategy_code successfully: {}", canister_id);
            Ok(())
        }
        Err((code, msg)) => Err(format!("Error code: {:?}, message: {}", code, msg)),
    }
}

// Simplified deployment result type
#[derive(CandidType, Clone, Debug)]
pub enum DeploymentResult {
    Success(Principal),
    Failure(String),
}

// Get WASM module directly from embedded constants
pub fn get_embedded_wasm_module(strategy_type: StrategyType) -> Option<Vec<u8>> {
    match strategy_type {
        StrategyType::SelfHedging => Some(SELF_HEDGING_WASM.to_vec()),
        // Other strategies use mock data until implemented
        StrategyType::DollarCostAveraging => Some(MOCK_WASM_HEADER.to_vec()),
        StrategyType::ValueAveraging => Some(MOCK_WASM_HEADER.to_vec()),
        StrategyType::FixedBalance => Some(MOCK_WASM_HEADER.to_vec()),
        StrategyType::LimitOrder => Some(MOCK_WASM_HEADER.to_vec()),
    }
}

// Initialize all strategy WASM modules - no longer needed as we use embedded modules only
pub fn initialize_wasm_modules() -> Result<(), String> {
    // This function is now a no-op since we directly use embedded WASM modules
    // Keeping it for API compatibility
    ic_cdk::println!("Using embedded WASM modules");
    Ok(())
}