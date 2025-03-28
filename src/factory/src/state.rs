use candid::Principal;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, Storable,
};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use strategy_common::types::{
    DeploymentRecord, DeploymentStatus, StrategyMetadata, StrategyType,
};
use serde::{Deserialize, Serialize};
use candid::CandidType;
use hex;

// ICP Ledger canister ID
pub const ICP_LEDGER_CANISTER_ID: &str = "ryjl3-tyaaa-aaaaa-aaaba-cai";

// Default deployment fee (1 ICP)
pub const DEFAULT_DEPLOYMENT_FEE: u64 = 100_000_000; // 1 ICP in e8s

// Constants for record archiving
pub const COMPLETED_RECORD_RETENTION_DAYS: u64 = 90; // 90 days
pub const RETENTION_PERIOD_NS: u64 = COMPLETED_RECORD_RETENTION_DAYS * 24 * 60 * 60 * 1_000_000_000;
pub const MAX_COMPLETED_RECORDS: usize = 10000;
pub const ARCHIVING_THRESHOLD_PERCENT: u8 = 80;

// User account system for recharge-based payment model
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct UserAccount {
    pub owner: Principal,
    pub balance: u64,        // User balance in e8s
    pub last_deposit: u64,   // Timestamp of last deposit
    pub total_deposited: u64, // Total amount deposited
    pub total_consumed: u64,  // Total amount consumed
}

// Transaction types for tracking financial operations
#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub enum TransactionType {
    Deposit,
    DeploymentFee,
    Refund,
    AdminAdjustment,
}

// Transaction record for financial history
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct TransactionRecord {
    pub transaction_id: String,
    pub user: Principal,
    pub amount: u64,
    pub transaction_type: TransactionType,
    pub timestamp: u64,
    pub description: String,
}

// Memory manager and stable storage
type Memory = VirtualMemory<DefaultMemoryImpl>;

// Helper types for stable storage with Ord implementation
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StrategyTypeBytes(pub Vec<u8>);

impl Storable for StrategyTypeBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WasmModuleBytes(pub Vec<u8>);

impl Storable for WasmModuleBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrincipalBytes(pub Vec<u8>);

impl Storable for PrincipalBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StrategyMetadataBytes(pub Vec<u8>);

impl Storable for StrategyMetadataBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeploymentIdBytes(pub Vec<u8>);

impl Storable for DeploymentIdBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeploymentRecordBytes(pub Vec<u8>);

impl Storable for DeploymentRecordBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TransactionRecordBytes(pub Vec<u8>);

impl Storable for TransactionRecordBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserAccountBytes(pub Vec<u8>);

impl Storable for UserAccountBytes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
    }
}

// Global state
thread_local! {
    // Memory manager for stable storage
    pub static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    // Map to store WASM modules by strategy type
    pub static WASM_MODULES: RefCell<StableBTreeMap<StrategyTypeBytes, WasmModuleBytes, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0)))
        )
    );

    // Map to store strategy metadata by canister ID
    pub static STRATEGIES: RefCell<StableBTreeMap<PrincipalBytes, StrategyMetadataBytes, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
        )
    );

    // Map to store deployment records by deployment ID
    pub static DEPLOYMENT_RECORDS: RefCell<StableBTreeMap<DeploymentIdBytes, DeploymentRecordBytes, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(2)))
        )
    );
    
    // Map to store user accounts by principal
    pub static USER_ACCOUNTS: RefCell<StableBTreeMap<PrincipalBytes, UserAccountBytes, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(3)))
        )
    );
    
    // Store transaction records
    pub static TRANSACTIONS: RefCell<StableBTreeMap<DeploymentIdBytes, TransactionRecordBytes, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(4)))
        )
    );

    // Map to store owner to strategies mapping (in-memory)
    pub static OWNER_STRATEGIES: RefCell<HashMap<Principal, Vec<Principal>>> = RefCell::new(HashMap::new());

    // Deployment fee in e8s (100_000_000 = 1 ICP)
    pub static DEPLOYMENT_FEE: Cell<u64> = Cell::new(DEFAULT_DEPLOYMENT_FEE);
    
    // Set of admin principals
    pub static ADMINS: RefCell<HashSet<Principal>> = RefCell::new(HashSet::new());
    
    // Counter for deployment IDs
    pub static DEPLOYMENT_ID_COUNTER: Cell<u64> = Cell::new(0);
    
    // In-memory transaction cache for faster access (limited to 1000 most recent entries)
    pub static TRANSACTION_CACHE: RefCell<Vec<TransactionRecord>> = RefCell::new(Vec::new());
    
    // Maximum transaction cache size
    pub static MAX_CACHE_SIZE: Cell<usize> = Cell::new(1000);
}

// WASM module record for strategy installations
#[derive(Serialize, Deserialize, Clone, Debug, CandidType)]
pub struct WasmModule {
    pub strategy_type: StrategyType,
    pub wasm_module: Vec<u8>,
}

// Generate a new deployment ID
pub fn generate_deployment_id() -> String {
    let timestamp = ic_cdk::api::time();
    let caller = ic_cdk::api::caller().to_text();
    let counter = DEPLOYMENT_ID_COUNTER.with(|c| {
        let current = c.get();
        c.set(current + 1);
        current
    });
    
    format!("{}-{}-{}", timestamp, caller, counter)
}

// Store deployment record
pub fn store_deployment_record(record: DeploymentRecord) {
    let record_bytes = candid::encode_one(&record)
        .expect("Failed to encode deployment record");
    let id_bytes = record.deployment_id.as_bytes().to_vec();
    
    DEPLOYMENT_RECORDS.with(|records| {
        records.borrow_mut().insert(
            DeploymentIdBytes(id_bytes),
            DeploymentRecordBytes(record_bytes),
        );
    });
}

// Get deployment record
pub fn get_deployment_record(deployment_id: &str) -> Option<DeploymentRecord> {
    let id_bytes = deployment_id.as_bytes().to_vec();
    
    DEPLOYMENT_RECORDS.with(|records| {
        records.borrow()
            .get(&DeploymentIdBytes(id_bytes))
            .and_then(|record_bytes| {
                candid::decode_one::<DeploymentRecord>(&record_bytes.0).ok()
            })
    })
}

// Update deployment record status
pub fn update_deployment_status(
    deployment_id: &str, 
    status: DeploymentStatus, 
    canister_id: Option<Principal>,
    error_message: Option<String>
) -> Result<(), String> {
    if let Some(mut record) = get_deployment_record(deployment_id) {
        record.status = status;
        record.last_updated = ic_cdk::api::time();
        
        if let Some(cid) = canister_id {
            record.canister_id = Some(cid);
        }
        
        if let Some(error) = error_message {
            record.error_message = Some(error);
        }
        
        store_deployment_record(record);
        Ok(())
    } else {
        Err(format!("Deployment record not found: {}", deployment_id))
    }
}

// Store strategy metadata
pub fn store_strategy_metadata(metadata: StrategyMetadata) {
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

// Get strategy metadata
pub fn get_strategy_metadata(canister_id: Principal) -> Option<StrategyMetadata> {
    let principal_bytes = canister_id.as_slice().to_vec();
    
    STRATEGIES.with(|s| {
        s.borrow()
            .get(&PrincipalBytes(principal_bytes))
            .and_then(|metadata_bytes| {
                candid::decode_one::<StrategyMetadata>(&metadata_bytes.0).ok()
            })
    })
}

// Admin permission check
pub fn is_admin() -> bool {
    let caller_principal = ic_cdk::api::caller();
    ADMINS.with(|admins| admins.borrow().contains(&caller_principal))
}

pub fn require_admin() -> Result<(), String> {
    if !is_admin() {
        return Err("Caller is not authorized to perform this action".to_string());
    }
    Ok(())
}

// Get deployment fee
pub fn get_fee() -> u64 {
    DEPLOYMENT_FEE.with(|f| f.get())
}

// Set deployment fee
pub fn set_fee(fee: u64) -> Result<(), String> {
    require_admin()?;
    DEPLOYMENT_FEE.with(|f| f.set(fee));
    Ok(())
}

// Store WASM module
pub fn store_wasm_module(wasm_module: WasmModule) -> Result<(), String> {
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

// Get WASM module
pub fn get_wasm_module(strategy_type: StrategyType) -> Option<Vec<u8>> {
    let strategy_type_bytes = candid::encode_one(&strategy_type).unwrap();
    
    WASM_MODULES.with(|modules| {
        modules
            .borrow()
            .get(&StrategyTypeBytes(strategy_type_bytes))
            .map(|module_bytes| module_bytes.0)
    })
}

// Get all deployment records
pub fn get_all_deployment_records() -> Vec<DeploymentRecord> {
    let mut records = Vec::new();
    
    DEPLOYMENT_RECORDS.with(|recs| {
        for (_, record_bytes) in recs.borrow().iter() {
            if let Ok(record) = candid::decode_one::<DeploymentRecord>(&record_bytes.0) {
                records.push(record);
            }
        }
    });
    
    records
}

// Get deployment records by owner
pub fn get_deployment_records_by_owner(owner: Principal) -> Vec<DeploymentRecord> {
    get_all_deployment_records()
        .into_iter()
        .filter(|record| record.owner == owner)
        .collect()
}

// Get deployment records by status
pub fn get_deployment_records_by_status(status: DeploymentStatus) -> Vec<DeploymentRecord> {
    get_all_deployment_records()
        .into_iter()
        .filter(|record| record.status == status)
        .collect()
}

// Archive old completed deployment records
pub fn archive_old_deployment_records() -> Result<usize, String> {
    let current_time = ic_cdk::api::time();
    let mut archived_count = 0;
    
    // First get all completed records to analyze
    let completed_records: Vec<_> = get_all_deployment_records()
        .into_iter()
        .filter(|record| 
            record.status == DeploymentStatus::Deployed || 
            record.status == DeploymentStatus::Refunded
        )
        .collect();
    
    // Sort by completion time (last_updated for completed records)
    let mut sorted_records = completed_records.clone();
    sorted_records.sort_by(|a, b| a.last_updated.cmp(&b.last_updated));
    
    // If we have more than MAX_COMPLETED_RECORDS, archive the oldest ones
    let excess_count = if sorted_records.len() > MAX_COMPLETED_RECORDS {
        sorted_records.len() - MAX_COMPLETED_RECORDS
    } else {
        0
    };
    
    // Also archive records older than retention period
    let retention_threshold = current_time.saturating_sub(RETENTION_PERIOD_NS);
    
    // Collect records to archive
    let records_to_archive: Vec<_> = sorted_records.into_iter()
        .enumerate()
        .filter(|(index, record)| {
            // Archive if it's in the excess count OR older than retention threshold
            *index < excess_count || record.last_updated < retention_threshold
        })
        .map(|(_, record)| record.deployment_id.clone())
        .collect();
    
    // Archive each record
    for deployment_id in records_to_archive {
        // Remove from main storage
        DEPLOYMENT_RECORDS.with(|records| {
            let mut records = records.borrow_mut();
            let key = DeploymentIdBytes(deployment_id.as_bytes().to_vec());
            if records.remove(&key).is_some() {
                archived_count += 1;
            }
        });
    }
    
    ic_cdk::println!("Archived {} completed deployment records", archived_count);
    Ok(archived_count)
}

// Estimate stable memory usage percentage
pub fn get_memory_usage_percent() -> u8 {
    // This is a simplified estimate - in production you would want to get
    // actual memory usage from the memory manager
    let total_records = DEPLOYMENT_RECORDS.with(|records| records.borrow().len());
    
    // Roughly estimate memory usage: 
    // Assume each record uses about 500 bytes including keys
    let estimated_memory_usage = total_records * 500;
    
    // Assuming 4GB stable memory maximum
    let max_memory = 4 * 1024 * 1024 * 1024;
    
    // Calculate percentage
    let percentage = (estimated_memory_usage as f64 / max_memory as f64 * 100.0) as u8;
    
    percentage.min(100)
}

// Check if archiving is needed
pub fn should_archive_records() -> bool {
    // Check if memory usage is above threshold
    let usage_percent = get_memory_usage_percent();
    
    // Archive if above threshold
    usage_percent > ARCHIVING_THRESHOLD_PERCENT
}

// Upgrade data structure
#[derive(Serialize, Deserialize, CandidType)]
pub struct UpgradeData {
    pub owner_strategies: HashMap<Principal, Vec<Principal>>,
    pub admins: HashSet<Principal>,
    pub deployment_id_counter: u64,
    pub deployment_fee: u64,
}

// Pre-upgrade data
pub fn get_upgrade_data() -> UpgradeData {
    UpgradeData {
        owner_strategies: OWNER_STRATEGIES.with(|m| m.borrow().clone()),
        admins: ADMINS.with(|a| a.borrow().clone()),
        deployment_id_counter: DEPLOYMENT_ID_COUNTER.with(|c| c.get()),
        deployment_fee: DEPLOYMENT_FEE.with(|f| f.get()),
    }
}

// Restore upgrade data
pub fn restore_upgrade_data(data: UpgradeData) {
    OWNER_STRATEGIES.with(|m| *m.borrow_mut() = data.owner_strategies);
    ADMINS.with(|a| *a.borrow_mut() = data.admins);
    DEPLOYMENT_ID_COUNTER.with(|c| c.set(data.deployment_id_counter));
    DEPLOYMENT_FEE.with(|f| f.set(data.deployment_fee));
}

// Generate a unique transaction ID
pub fn generate_transaction_id() -> String {
    // Use async/await properly with raw_rand
    let timestamp = ic_cdk::api::time();
    
    // Generate a pseudo-random string for the transaction ID
    // Instead of using raw_rand (which requires async), we'll use timestamp + counter
    let random_part = DEPLOYMENT_ID_COUNTER.with(|counter| {
        let value = counter.get();
        counter.set(value.wrapping_add(1));
        value
    });
    
    format!("txn-{}-{:x}", timestamp, random_part)
}

// Store user account
pub fn store_user_account(account: UserAccount) {
    let account_bytes = candid::encode_one(&account)
        .expect("Failed to encode user account");
    let principal_bytes = account.owner.as_slice().to_vec();
    
    USER_ACCOUNTS.with(|accounts| {
        accounts.borrow_mut().insert(
            PrincipalBytes(principal_bytes),
            UserAccountBytes(account_bytes),
        );
    });
}

// Get user account
pub fn get_user_account(user: Principal) -> Option<UserAccount> {
    let principal_bytes = user.as_slice().to_vec();
    
    USER_ACCOUNTS.with(|accounts| {
        accounts.borrow()
            .get(&PrincipalBytes(principal_bytes))
            .and_then(|account_bytes| {
                candid::decode_one::<UserAccount>(&account_bytes.0).ok()
            })
    })
}

// Store transaction record - with cache size management
pub fn store_transaction_record(record: TransactionRecord) {
    let record_bytes = candid::encode_one(&record)
        .expect("Failed to encode transaction record");
    let id_bytes = record.transaction_id.as_bytes().to_vec();
    
    TRANSACTIONS.with(|transactions| {
        transactions.borrow_mut().insert(
            DeploymentIdBytes(id_bytes),
            TransactionRecordBytes(record_bytes),
        );
    });
    
    // Also add to the in-memory cache for faster access, with size management
    TRANSACTION_CACHE.with(|cache| {
        let max_size = MAX_CACHE_SIZE.with(|ms| ms.get());
        let mut cache_ref = cache.borrow_mut();
        
        // Add to cache
        cache_ref.push(record);
        
        // Trim cache if too large
        if cache_ref.len() > max_size {
            // Only keep the most recent 80% of the maximum size
            let trim_size = (max_size as f64 * 0.8) as usize;
            let start_index = cache_ref.len() - trim_size;
            *cache_ref = cache_ref.split_off(start_index);
        }
    });
}

// Get all transaction records
pub fn get_all_transaction_records() -> Vec<TransactionRecord> {
    let mut records = Vec::new();
    
    TRANSACTIONS.with(|txns| {
        for (_, record_bytes) in txns.borrow().iter() {
            if let Ok(record) = candid::decode_one::<TransactionRecord>(&record_bytes.0) {
                records.push(record);
            }
        }
    });
    
    records
}

// Get transaction records for a user
pub fn get_user_transaction_records(user: Principal) -> Vec<TransactionRecord> {
    // First try to get from cache for better performance
    let from_cache = TRANSACTION_CACHE.with(|cache| {
        cache.borrow()
            .iter()
            .filter(|record| record.user == user)
            .cloned()
            .collect::<Vec<_>>()
    });
    
    if !from_cache.is_empty() {
        return from_cache;
    }
    
    // Fall back to stable storage if cache is empty
    get_all_transaction_records()
        .into_iter()
        .filter(|record| record.user == user)
        .collect()
}

// Record a new transaction
pub fn record_transaction(
    user: Principal, 
    amount: u64, 
    transaction_type: TransactionType, 
    description: String
) -> String {
    let transaction_id = generate_transaction_id();
    
    let record = TransactionRecord {
        transaction_id: transaction_id.clone(),
        user,
        amount,
        transaction_type,
        timestamp: ic_cdk::api::time(),
        description,
    };
    
    store_transaction_record(record);
    
    transaction_id
}

// Update user balance
pub fn update_user_balance(user: Principal, amount: u64, is_deposit: bool) -> Result<u64, String> {
    let mut account = get_user_account(user).unwrap_or(UserAccount {
        owner: user,
        balance: 0,
        last_deposit: 0,
        total_deposited: 0,
        total_consumed: 0,
    });
    
    if is_deposit {
        account.balance = account.balance.saturating_add(amount);
        account.last_deposit = ic_cdk::api::time();
        account.total_deposited = account.total_deposited.saturating_add(amount);
    } else {
        if account.balance < amount {
            return Err(format!(
                "Insufficient balance: current balance {} e8s, required {} e8s", 
                account.balance, amount
            ));
        }
        
        account.balance = account.balance.saturating_sub(amount);
        account.total_consumed = account.total_consumed.saturating_add(amount);
    }
    
    store_user_account(account.clone());
    
    Ok(account.balance)
}

// Check if user has sufficient balance
pub fn check_user_balance(user: Principal, amount: u64) -> Result<bool, String> {
    match get_user_account(user) {
        Some(account) => Ok(account.balance >= amount),
        None => Ok(false), // New users have zero balance
    }
}

// Pre-upgrade and post-upgrade functions
pub fn pre_upgrade() {
    // Store memory-only data that needs to survive upgrades
    let upgrade_data = get_upgrade_data();
    
    match ic_cdk::storage::stable_save((upgrade_data,)) {
        Ok(_) => (),
        Err(e) => ic_cdk::trap(&format!("Failed to save stable data: {}", e)),
    }
    
    // Rebuild transaction cache on next startup
    TRANSACTION_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

pub fn post_upgrade() {
    match ic_cdk::storage::stable_restore::<(UpgradeData,)>() {
        Ok((data,)) => {
            restore_upgrade_data(data);
            
            // Rebuild the transaction cache from stable storage
            let transactions = get_all_transaction_records();
            TRANSACTION_CACHE.with(|cache| {
                *cache.borrow_mut() = transactions;
            });
        }
        Err(e) => ic_cdk::trap(&format!("Failed to restore stable data: {}", e)),
    }
} 