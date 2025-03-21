use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::call::{call, CallResult};
use ic_cdk::api::management_canister::main::{
    canister_status, create_canister, install_code, CanisterInstallMode, CanisterSettings,
    CreateCanisterArgument, InstallCodeArgument,
};
use ic_cdk::api::{caller, canister_balance, time};
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};
use ic_ledger_types::{AccountIdentifier, Memo, Tokens, TransferArgs, DEFAULT_SUBACCOUNT};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, Storable,
};
use serde::Serialize;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use strategy_common::types::{
    DCAConfig, DeploymentResult, FixedBalanceConfig, LimitOrderConfig, SelfHedgingConfig,
    StrategyMetadata, StrategyStatus, StrategyType, ValueAvgConfig, TradingPair, TokenMetadata,
};

// ICP Ledger canister ID for fee collection
const ICP_LEDGER_CANISTER_ID: &str = "ryjl3-tyaaa-aaaaa-aaaba-cai";

// Default deployment fee (1 ICP)
const DEFAULT_DEPLOYMENT_FEE: u64 = 100_000_000; // 1 ICP in e8s

// Memory manager and stable storage
type Memory = VirtualMemory<DefaultMemoryImpl>;
thread_local! {
    // Memory manager for stable storage
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    // Map to store WASM modules by strategy type
    static WASM_MODULES: RefCell<StableBTreeMap<StrategyTypeBytes, WasmModuleBytes, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0)))
        )
    );

    // Map to store strategy metadata by canister ID
    static STRATEGIES: RefCell<StableBTreeMap<PrincipalBytes, StrategyMetadataBytes, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
        )
    );

    // Map to store owner to strategies mapping
    static OWNER_STRATEGIES: RefCell<HashMap<Principal, Vec<Principal>>> = RefCell::new(HashMap::new());

    // Deployment fee in e8s (100_000_000 = 1 ICP)
    static DEPLOYMENT_FEE: Cell<u64> = Cell::new(DEFAULT_DEPLOYMENT_FEE);
    
    // Set of admin principals
    static ADMINS: RefCell<HashSet<Principal>> = RefCell::new(HashSet::new());
}

// Helper types for stable storage
#[derive(Clone, Debug)]
struct StrategyTypeBytes(Vec<u8>);

impl Storable for StrategyTypeBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug)]
struct WasmModuleBytes(Vec<u8>);

impl Storable for WasmModuleBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug)]
struct PrincipalBytes(Vec<u8>);

impl Storable for PrincipalBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug)]
struct StrategyMetadataBytes(Vec<u8>);

impl Storable for StrategyMetadataBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

// WASM module record for strategy installations
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct WasmModule {
    pub strategy_type: StrategyType,
    pub wasm_module: Vec<u8>,
}

// Admin permission check
fn is_admin() -> bool {
    let caller_principal = caller();
    ADMINS.with(|admins| admins.borrow().contains(&caller_principal))
}

fn require_admin() -> Result<(), String> {
    if !is_admin() {
        return Err("Caller is not authorized to perform this action".to_string());
    }
    Ok(())
}

// Canister initialization
#[init]
fn init() {
    // Initialize with default deployment fee
    DEPLOYMENT_FEE.with(|fee| fee.set(DEFAULT_DEPLOYMENT_FEE));
    
    // Set initial admin (caller of init)
    let initial_admin = caller();
    ADMINS.with(|admins| {
        admins.borrow_mut().insert(initial_admin);
    });
}

// Admin management
#[update]
fn add_admin(principal: Principal) -> Result<(), String> {
    require_admin()?;
    
    ADMINS.with(|admins| {
        admins.borrow_mut().insert(principal);
    });
    
    Ok(())
}

#[update]
fn remove_admin(principal: Principal) -> Result<(), String> {
    require_admin()?;
    
    // Prevent removing the last admin
    let is_last_admin = ADMINS.with(|admins| {
        let admins_ref = admins.borrow();
        admins_ref.len() == 1 && admins_ref.contains(&principal)
    });
    
    if is_last_admin {
        return Err("Cannot remove the last admin".to_string());
    }
    
    ADMINS.with(|admins| {
        admins.borrow_mut().remove(&principal);
    });
    
    Ok(())
}

#[query]
fn get_admins() -> Vec<Principal> {
    ADMINS.with(|admins| admins.borrow().iter().cloned().collect())
}

#[query]
fn is_caller_admin() -> bool {
    is_admin()
}

// Strategy WASM module management
#[update]
async fn install_strategy_wasm(wasm_module: WasmModule) -> Result<(), String> {
    require_admin()?;
    
    let strategy_type_bytes = candid::encode_one(&wasm_module.strategy_type)
        .map_err(|e| format!("Failed to encode strategy type: {}", e))?;
    
    WASM_MODULES.with(|modules| {
        modules.borrow_mut().insert(
            StrategyTypeBytes(strategy_type_bytes),
            WasmModuleBytes(wasm_module.wasm_module),
        );
    });
    
    Ok(())
}

#[query]
fn get_strategy_wasm(strategy_type: StrategyType) -> Option<Vec<u8>> {
    let strategy_type_bytes = candid::encode_one(&strategy_type).unwrap();
    
    WASM_MODULES.with(|modules| {
        modules
            .borrow()
            .get(&StrategyTypeBytes(strategy_type_bytes))
            .map(|module_bytes| module_bytes.0)
    })
}

// Helper function to collect deployment fee
async fn collect_deployment_fee() -> Result<(), String> {
    let fee = DEPLOYMENT_FEE.with(|f| f.get());
    // todo Refactor transfer logic, using user authorization for transactions
    if fee > 0 {
        let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID).unwrap();
        let factory_account = AccountIdentifier::new(&ic_cdk::id(), &DEFAULT_SUBACCOUNT);
        
        let transfer_args = TransferArgs {
            memo: Memo(0),
            amount: Tokens::from_e8s(fee),
            fee: Tokens::from_e8s(10_000), // 0.0001 ICP
            from_subaccount: None,
            to: factory_account,
            created_at_time: None,
        };
        
        let transfer_result = call(ledger_id, "transfer", (transfer_args,)).await;
        if let Err((code, msg)) = transfer_result {
            return Err(format!(
                "Failed to collect deployment fee: code={:?}, message={}",
                code, msg
            ));
        }
    }
    
    Ok(())
}

// Strategy deployment
#[update]
async fn deploy_dca_strategy(config: DCAConfig) -> DeploymentResult {
    let caller = caller();
    
    // Charge deployment fee
    if let Err(e) = collect_deployment_fee().await {
        return DeploymentResult::Error(e);
    }
    
    // Check if strategy WASM module exists
    let wasm_module = match get_strategy_wasm(StrategyType::DollarCostAveraging) {
        Some(wasm) => wasm,
        None => return DeploymentResult::Error("DCA strategy WASM module not found".to_string()),
    };
    
    // Create canister
    let canister_id = match create_strategy_canister().await {
        Ok(canister_id) => canister_id,
        Err(err) => return DeploymentResult::Error(format!("Failed to create canister: {}", err)),
    };
    
    // Install code
    if let Err(err) = install_strategy_code(canister_id, wasm_module).await {
        return DeploymentResult::Error(format!("Failed to install code: {}", err));
    }
    
    // Initialize canister with config
    let init_result = call(
        canister_id,
        "init_dca",
        (caller, config.clone()),
    )
    .await;
    
    if let Err((code, msg)) = init_result {
        return DeploymentResult::Error(format!(
            "Failed to initialize DCA strategy: code={:?}, message={}",
            code, msg
        ));
    }
    
    // Store strategy metadata
    let metadata = StrategyMetadata {
        canister_id,
        strategy_type: StrategyType::DollarCostAveraging,
        owner: caller,
        created_at: time(),
        status: StrategyStatus::Created,
        exchange: config.exchange,
        trading_pair: TradingPair {
            base_token: config.base_token.clone(),
            quote_token: config.quote_token.clone(),
        },
    };
    
    store_strategy_metadata(metadata.clone());
    
    DeploymentResult::Success(canister_id)
}

#[update]
async fn deploy_value_avg_strategy(config: ValueAvgConfig) -> DeploymentResult {
    let caller = caller();
    
    // Charge deployment fee
    if let Err(e) = collect_deployment_fee().await {
        return DeploymentResult::Error(e);
    }
    
    // Check if strategy WASM module exists
    let wasm_module = match get_strategy_wasm(StrategyType::ValueAveraging) {
        Some(wasm) => wasm,
        None => return DeploymentResult::Error("Value Averaging strategy WASM module not found".to_string()),
    };
    
    // Create canister
    let canister_id = match create_strategy_canister().await {
        Ok(canister_id) => canister_id,
        Err(err) => return DeploymentResult::Error(format!("Failed to create canister: {}", err)),
    };
    
    // Install code
    if let Err(err) = install_strategy_code(canister_id, wasm_module).await {
        return DeploymentResult::Error(format!("Failed to install code: {}", err));
    }
    
    // Initialize canister with config
    let init_result = call(
        canister_id,
        "init_value_avg",
        (caller, config.clone()),
    )
    .await;
    
    if let Err((code, msg)) = init_result {
        return DeploymentResult::Error(format!(
            "Failed to initialize Value Averaging strategy: code={:?}, message={}",
            code, msg
        ));
    }
    
    // Store strategy metadata
    let metadata = StrategyMetadata {
        canister_id,
        strategy_type: StrategyType::ValueAveraging,
        owner: caller,
        created_at: time(),
        status: StrategyStatus::Created,
        exchange: config.exchange,
        trading_pair: TradingPair {
            base_token: config.base_token.clone(),
            quote_token: config.quote_token.clone(),
        },
    };
    
    store_strategy_metadata(metadata.clone());
    
    DeploymentResult::Success(canister_id)
}

#[update]
async fn deploy_fixed_balance_strategy(config: FixedBalanceConfig) -> DeploymentResult {
    let caller = caller();
    
    // Charge deployment fee
    if let Err(e) = collect_deployment_fee().await {
        return DeploymentResult::Error(e);
    }
    
    // Check if strategy WASM module exists
    let wasm_module = match get_strategy_wasm(StrategyType::FixedBalance) {
        Some(wasm) => wasm,
        None => return DeploymentResult::Error("Fixed Balance strategy WASM module not found".to_string()),
    };
    
    // Create canister
    let canister_id = match create_strategy_canister().await {
        Ok(canister_id) => canister_id,
        Err(err) => return DeploymentResult::Error(format!("Failed to create canister: {}", err)),
    };
    
    // Install code
    if let Err(err) = install_strategy_code(canister_id, wasm_module).await {
        return DeploymentResult::Error(format!("Failed to install code: {}", err));
    }
    
    // Initialize canister with config
    let init_result = call(
        canister_id,
        "init_fixed_balance",
        (caller, config.clone()),
    )
    .await;
    
    if let Err((code, msg)) = init_result {
        return DeploymentResult::Error(format!(
            "Failed to initialize Fixed Balance strategy: code={:?}, message={}",
            code, msg
        ));
    }
    
    // Store strategy metadata
    let metadata = StrategyMetadata {
        canister_id,
        strategy_type: StrategyType::FixedBalance,
        owner: caller,
        created_at: time(),
        status: StrategyStatus::Created,
        exchange: config.exchange,
        trading_pair: TradingPair {
            base_token: config.token_allocations.keys().next()
                .expect("Fixed Balance strategy must have at least one token allocation")
                .clone(),
            quote_token: TokenMetadata {
                canister_id: Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai").unwrap(),
                symbol: "ICP".to_string(),
                decimals: 8,
            },
        },
    };
    
    store_strategy_metadata(metadata.clone());
    
    DeploymentResult::Success(canister_id)
}

#[update]
async fn deploy_limit_order_strategy(config: LimitOrderConfig) -> DeploymentResult {
    let caller = caller();
    
    // Charge deployment fee
    if let Err(e) = collect_deployment_fee().await {
        return DeploymentResult::Error(e);
    }
    
    // Check if strategy WASM module exists
    let wasm_module = match get_strategy_wasm(StrategyType::LimitOrder) {
        Some(wasm) => wasm,
        None => return DeploymentResult::Error("Limit Order strategy WASM module not found".to_string()),
    };
    
    // Create canister
    let canister_id = match create_strategy_canister().await {
        Ok(canister_id) => canister_id,
        Err(err) => return DeploymentResult::Error(format!("Failed to create canister: {}", err)),
    };
    
    // Install code
    if let Err(err) = install_strategy_code(canister_id, wasm_module).await {
        return DeploymentResult::Error(format!("Failed to install code: {}", err));
    }
    
    // Initialize canister with config
    let init_result = call(
        canister_id,
        "init_limit_order",
        (caller, LimitOrderConfig {
            exchange: config.exchange.clone(),
            base_token: config.base_token.clone(),
            quote_token: config.quote_token.clone(),
            order_type: config.order_type,
            price: config.price,
            amount: config.amount,
            expiration: config.expiration,
        }),
    )
    .await;
    
    if let Err((code, msg)) = init_result {
        return DeploymentResult::Error(format!(
            "Failed to initialize Limit Order strategy: code={:?}, message={}",
            code, msg
        ));
    }
    
    // Store strategy metadata
    let metadata = StrategyMetadata {
        canister_id,
        strategy_type: StrategyType::LimitOrder,
        owner: caller,
        created_at: time(),
        status: StrategyStatus::Created,
        exchange: config.exchange,
        trading_pair: TradingPair {
            base_token: config.base_token.clone(),
            quote_token: config.quote_token.clone(),
        },
    };
    
    store_strategy_metadata(metadata.clone());
    
    DeploymentResult::Success(canister_id)
}

#[update]
async fn deploy_self_hedging_strategy(config: SelfHedgingConfig) -> DeploymentResult {
    let caller = caller();
    
    // Charge deployment fee
    if let Err(e) = collect_deployment_fee().await {
        return DeploymentResult::Error(e);
    }
    
    // Check if strategy WASM module exists
    let wasm_module = match get_strategy_wasm(StrategyType::SelfHedging) {
        Some(wasm) => wasm,
        None => return DeploymentResult::Error("Self Hedging strategy WASM module not found".to_string()),
    };
    
    // Create canister
    let canister_id = match create_strategy_canister().await {
        Ok(canister_id) => canister_id,
        Err(err) => return DeploymentResult::Error(format!("Failed to create canister: {}", err)),
    };
    
    // Install code
    if let Err(err) = install_strategy_code(canister_id, wasm_module).await {
        return DeploymentResult::Error(format!("Failed to install code: {}", err));
    }
    
    // Initialize canister with config
    let init_result: CallResult<()> = call(
        canister_id,
        "init_self_hedging",
        (caller, config.clone()),
    )
    .await;
    
    if let Err((code, msg)) = init_result {
        return DeploymentResult::Error(format!(
            "Failed to initialize Self Hedging strategy: code={:?}, message={}",
            code, msg
        ));
    }
    
    // Store strategy metadata
    let metadata = StrategyMetadata {
        canister_id,
        strategy_type: StrategyType::SelfHedging,
        owner: caller,
        created_at: time(),
        status: StrategyStatus::Created,
        exchange: config.exchange,
        trading_pair: TradingPair {
            base_token: config.primary_token.clone(),
            quote_token: config.hedge_token.clone(),
        },
    };
    
    store_strategy_metadata(metadata.clone());
    
    DeploymentResult::Success(canister_id)
}

// Strategy registry queries
#[query]
fn get_strategies_by_owner(owner: Principal) -> Vec<StrategyMetadata> {
    let mut strategies = Vec::new();
    
    OWNER_STRATEGIES.with(|owner_strategies| {
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
    
    STRATEGIES.with(|s| {
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
    STRATEGIES.with(|s| s.borrow().len() as u64)
}

// Governance functions
#[update]
fn set_deployment_fee(fee_e8s: u64) -> Result<(), String> {
    require_admin()?;
    
    DEPLOYMENT_FEE.with(|f| f.set(fee_e8s));
    Ok(())
}

#[query]
fn get_deployment_fee() -> u64 {
    DEPLOYMENT_FEE.with(|f| f.get())
}

#[update]
async fn withdraw_funds(recipient: Principal, amount_e8s: u64) -> Result<(), String> {
    require_admin()?;
    
    // Convert principal to account identifier
    let recipient_account = AccountIdentifier::new(&recipient, &DEFAULT_SUBACCOUNT);
    
    // Prepare transfer arguments
    let transfer_args = TransferArgs {
        memo: Memo(0),
        amount: Tokens::from_e8s(amount_e8s),
        fee: Tokens::from_e8s(10_000), // 0.0001 ICP
        from_subaccount: None,
        to: recipient_account,
        created_at_time: None,
    };
    
    // Execute transfer with error handling
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID).unwrap();
    let transfer_result = call(ledger_id, "transfer", (transfer_args,)).await;
    
    match transfer_result {
        Ok(_) => Ok(()),
        Err((code, msg)) => Err(format!("Transfer failed: code={:?}, message={}", code, msg)),
    }
}

// Cycles management
#[query]
fn get_cycles_balance() -> u64 {
    canister_balance()
}

// Helper functions
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
    
    let result = create_canister(args, 1_000_000_000_000u128).await;
    
    match result {
        Ok((record,)) => Ok(record.canister_id),
        Err((code, msg)) => Err(format!("Error code: {:?}, message: {}", code, msg)),
    }
}

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

fn store_strategy_metadata(metadata: StrategyMetadata) {
    // Serialize metadata
    let metadata_bytes = candid::encode_one(&metadata)
        .expect("Failed to encode strategy metadata");
    let principal_bytes = metadata.canister_id.as_slice().to_vec();
    
    // Store in stable storage
    STRATEGIES.with(|s| {
        s.borrow_mut().insert(
            PrincipalBytes(principal_bytes),
            StrategyMetadataBytes(metadata_bytes),
        );
    });
    
    // Update owner to strategies mapping
    OWNER_STRATEGIES.with(|owner_strategies| {
        let mut map = owner_strategies.borrow_mut();
        if let Some(strategies) = map.get_mut(&metadata.owner) {
            strategies.push(metadata.canister_id);
        } else {
            map.insert(metadata.owner, vec![metadata.canister_id]);
        }
    });
}

fn get_strategy_metadata(canister_id: Principal) -> Option<StrategyMetadata> {
    let principal_bytes = canister_id.as_slice().to_vec();
    
    STRATEGIES.with(|s| {
        s.borrow()
            .get(&PrincipalBytes(principal_bytes))
            .and_then(|metadata_bytes| {
                candid::decode_one::<StrategyMetadata>(&metadata_bytes.0).ok()
            })
    })
}

// Pre-upgrade hook to preserve data
#[pre_upgrade]
fn pre_upgrade() {
    // Owner strategies map needs special handling as it's not in stable storage
    let owner_strategies_map = OWNER_STRATEGIES.with(|m| m.borrow().clone());
    let admins_set = ADMINS.with(|a| a.borrow().clone());
    
    let serialized = candid::encode_one(&(owner_strategies_map, admins_set))
        .expect("Failed to encode upgrade data");
    
    ic_cdk::storage::stable_save((serialized,)).unwrap();
}

// Post-upgrade hook to restore data
#[post_upgrade]
fn post_upgrade() {
    if let Ok((serialized,)) = ic_cdk::storage::stable_restore::<(Vec<u8>,)>() {
        if let Ok((map, admins)) = candid::decode_one::<(HashMap<Principal, Vec<Principal>>, HashSet<Principal>)>(&serialized) {
            OWNER_STRATEGIES.with(|m| *m.borrow_mut() = map);
            ADMINS.with(|a| *a.borrow_mut() = admins);
        }
    }
} 