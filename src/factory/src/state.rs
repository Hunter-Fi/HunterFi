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
use std::marker::PhantomData;
use bincode;

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
#[derive(CandidType, Deserialize, Serialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserAccount {
    pub owner: Principal,
    pub balance: u64,        // User balance in e8s
    pub last_deposit: u64,   // Timestamp of last deposit
    pub total_deposited: u64, // Total amount deposited
    pub total_consumed: u64,  // Total amount consumed
}

// Transaction types for tracking financial operations
#[derive(CandidType, Deserialize, Serialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransactionType {
    Deposit,
    DeploymentFee,
    Refund,
    AdminAdjustment,
    Withdrawal,
    Transfer,
}

// Transaction record for financial history
#[derive(CandidType, Deserialize, Serialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

// Generic Bytes wrapper for any type that can be serialized
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GenericBytes<T: Serialize + for<'de> Deserialize<'de> + Ord> {
    pub data: Vec<u8>,
    _marker: PhantomData<T>,
}

impl<T: Serialize + for<'de> Deserialize<'de> + Ord> GenericBytes<T> {
    pub fn new(value: &T) -> Self {
        let bytes = bincode::serialize(value).unwrap_or_default();
        Self {
            data: bytes,
            _marker: PhantomData,
        }
    }

    pub fn into_inner(&self) -> Option<T> {
        bincode::deserialize(&self.data).ok()
    }
}

impl<T: Serialize + for<'de> Deserialize<'de> + Ord> Storable for GenericBytes<T> {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.data)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self {
            data: bytes.to_vec(),
            _marker: PhantomData,
        }
    }
}

// Type aliases for common types
pub type StrategyTypeBytes = GenericBytes<StrategyType>;
pub type PrincipalBytes = GenericBytes<Principal>;
pub type StrategyMetadataBytes = GenericBytes<StrategyMetadata>;
pub type DeploymentIdBytes = GenericBytes<String>;
pub type DeploymentRecordBytes = GenericBytes<DeploymentRecord>;
pub type TransactionRecordBytes = GenericBytes<TransactionRecord>;
pub type UserAccountBytes = GenericBytes<UserAccount>;

// Global state
thread_local! {
    // Memory manager for stable storage
    pub static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
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

// Generic function to store data in a StableBTreeMap
fn store_in_map<K, V, M>(
    map: &RefCell<StableBTreeMap<GenericBytes<K>, GenericBytes<V>, M>>,
    key: &K,
    value: &V,
) where
    K: Serialize + for<'de> Deserialize<'de> + Clone + Ord,
    V: Serialize + for<'de> Deserialize<'de> + Clone + Ord,
    M: ic_stable_structures::Memory,
{
    map.borrow_mut().insert(
        GenericBytes::new(key),
        GenericBytes::new(value),
    );
}

// Generic function to get data from a StableBTreeMap
fn get_from_map<K, V, M>(
    map: &RefCell<StableBTreeMap<GenericBytes<K>, GenericBytes<V>, M>>,
    key: &K,
) -> Option<V> where
    K: Serialize + for<'de> Deserialize<'de> + Clone + Ord,
    V: Serialize + for<'de> Deserialize<'de> + Clone + Ord,
    M: ic_stable_structures::Memory,
{
    map.borrow()
        .get(&GenericBytes::new(key))
        .and_then(|bytes| bytes.into_inner())
}

// Store deployment record using the generic function
pub fn store_deployment_record(record: DeploymentRecord) {
    DEPLOYMENT_RECORDS.with(|records| {
        store_in_map(records, &record.deployment_id.clone(), &record);
    });
}

// Get deployment record using the generic function
pub fn get_deployment_record(deployment_id: &str) -> Option<DeploymentRecord> {
    DEPLOYMENT_RECORDS.with(|records| {
        get_from_map(records, &deployment_id.to_string())
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

// Store strategy metadata with optimized code
pub fn store_strategy_metadata(metadata: StrategyMetadata) {
    // Store in stable storage using generic function
    STRATEGIES.with(|s| {
        store_in_map(s, &metadata.canister_id, &metadata);
    });
    
    // Update owner to strategies mapping
    OWNER_STRATEGIES.with(|owner_strategies| {
        let mut map = owner_strategies.borrow_mut();
        map.entry(metadata.owner)
           .or_insert_with(Vec::new)
           .push(metadata.canister_id);
    });
}

// Get strategy metadata using the generic function
pub fn get_strategy_metadata(canister_id: Principal) -> Option<StrategyMetadata> {
    STRATEGIES.with(|s| {
        get_from_map(s, &canister_id)
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

// Get all deployment records
pub fn get_all_deployment_records() -> Vec<DeploymentRecord> {
    let mut records = Vec::new();
    
    DEPLOYMENT_RECORDS.with(|recs| {
        for (_, record_bytes) in recs.borrow().iter() {
            if let Some(record) = record_bytes.into_inner() {
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
            let key = DeploymentIdBytes::new(&deployment_id);
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
    let max_memory = 4 * 1024 * 1024;
    
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
    #[serde(default)]
    pub transactions: Option<Vec<(String, TransactionRecord)>>,
    #[serde(default)]
    pub user_accounts: Option<HashMap<Principal, UserAccount>>,
    #[serde(default)]
    pub strategies: Option<Vec<(Principal, StrategyMetadata)>>,
}

// Pre-upgrade data
pub fn get_upgrade_data() -> UpgradeData {
    let transactions = TRANSACTIONS.with(|t| {
        let mut tx_vec = Vec::new();
        for (key_bytes, val_bytes) in t.borrow().iter() {
            if let (Some(tx_id), Some(record)) = (key_bytes.into_inner(), val_bytes.into_inner()) {
                tx_vec.push((tx_id, record));
            }
        }
        tx_vec
    });
    
    let user_accounts = USER_ACCOUNTS.with(|accounts| {
        let mut user_map = HashMap::new();
        for (key_bytes, val_bytes) in accounts.borrow().iter() {
            if let (Some(principal), Some(account)) = (key_bytes.into_inner(), val_bytes.into_inner()) {
                user_map.insert(principal, account);
            }
        }
        user_map
    });
    
    let strategies = STRATEGIES.with(|s| {
        let mut strategies_vec = Vec::new();
        for (key_bytes, val_bytes) in s.borrow().iter() {
            if let (Some(principal), Some(metadata)) = (key_bytes.into_inner(), val_bytes.into_inner()) {
                strategies_vec.push((principal, metadata));
            }
        }
        strategies_vec
    });
    
    UpgradeData {
        owner_strategies: OWNER_STRATEGIES.with(|m| m.borrow().clone()),
        admins: ADMINS.with(|a| a.borrow().clone()),
        deployment_id_counter: DEPLOYMENT_ID_COUNTER.with(|c| c.get()),
        deployment_fee: DEPLOYMENT_FEE.with(|f| f.get()),
        transactions: Some(transactions),
        user_accounts: Some(user_accounts),
        strategies: Some(strategies),
    }
}

// Restore upgrade data
pub fn restore_upgrade_data(data: UpgradeData) {
    OWNER_STRATEGIES.with(|m| *m.borrow_mut() = data.owner_strategies);
    ADMINS.with(|a| *a.borrow_mut() = data.admins);
    DEPLOYMENT_ID_COUNTER.with(|c| c.set(data.deployment_id_counter));
    DEPLOYMENT_FEE.with(|f| f.set(data.deployment_fee));
    
    if let Some(transactions) = data.transactions {
        TRANSACTIONS.with(|t| {
            let mut transactions_map = t.borrow_mut();
            for (tx_id, record) in transactions {
                transactions_map.insert(GenericBytes::new(&tx_id), GenericBytes::new(&record));
            }
        });
    }
    
    if let Some(accounts) = data.user_accounts {
        USER_ACCOUNTS.with(|accounts_ref| {
            let mut accounts_map = accounts_ref.borrow_mut();
            for (principal, account) in accounts {
                accounts_map.insert(GenericBytes::new(&principal), GenericBytes::new(&account));
            }
        });
    }
    
    if let Some(strategies) = data.strategies {
        STRATEGIES.with(|s| {
            let mut strategies_map = s.borrow_mut();
            for (principal, metadata) in strategies {
                strategies_map.insert(GenericBytes::new(&principal), GenericBytes::new(&metadata));
            }
        });
    }
}

// Generate a unique transaction ID
pub async fn generate_transaction_id() -> String {
    let timestamp = ic_cdk::api::time();
    let caller = ic_cdk::api::caller().to_text();
    let id = ic_cdk::api::id();
    let random_bytes = id.as_slice();
    let random_hex = hex::encode(random_bytes);
    
    format!("{}-{}-{}", timestamp, caller, random_hex)
}

// Store user account using the generic function
pub fn store_user_account(account: UserAccount) {
    USER_ACCOUNTS.with(|accounts| {
        store_in_map(accounts, &account.owner, &account);
    });
}

// Get user account using the generic function
pub fn get_user_account(user: Principal) -> UserAccount {
    USER_ACCOUNTS.with(|accounts| {
        get_from_map(accounts, &user).unwrap_or_else(|| UserAccount {
            owner: user,
            balance: 0,
            last_deposit: 0,
            total_deposited: 0,
            total_consumed: 0,
        })
    })
}

// Store transaction record using the generic function
pub fn store_transaction_record(record: TransactionRecord) {
    TRANSACTIONS.with(|transactions| {
        store_in_map(transactions, &record.transaction_id.clone(), &record);
    });
    
    // Maintain in-memory cache for fast access
    TRANSACTION_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.push(record.clone());
        
        // Limit cache size
        let max_size = MAX_CACHE_SIZE.with(|max| max.get());
        if cache.len() > max_size {
            cache.remove(0); // Remove oldest
        }
    });
}

// Get all transaction records
pub fn get_all_transaction_records() -> Vec<TransactionRecord> {
    TRANSACTION_CACHE.with(|cache| cache.borrow().clone())
}

// Get transaction records for a user
pub fn get_user_transaction_records(user: Principal) -> Vec<TransactionRecord> {
    TRANSACTION_CACHE.with(|cache| {
        cache.borrow()
            .iter()
            .filter(|record| record.user == user)
            .cloned()
            .collect()
    })
}

// Record a transaction
pub async fn record_transaction(
    user: Principal,
    amount: u64,
    transaction_type: TransactionType,
    description: String,
) -> String {
    let transaction_id = generate_transaction_id().await;
    
    let record = TransactionRecord {
        transaction_id: transaction_id.clone(),
        user,
        amount,
        transaction_type,
        timestamp: ic_cdk::api::time(),
        description,
    };
    
    store_transaction_record(record.clone());
    
    transaction_id
}

// Update user balance
pub fn update_user_balance(user: Principal, amount: u64, is_deposit: bool) -> Result<u64, String> {
    let mut account = get_user_account(user);
    
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
    Ok(get_user_account(user).balance >= amount)
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

pub async fn process_balance_refund(user: Principal, amount: u64, deployment_id: &str) -> Result<(), String> {
    // Verify refund amount is valid
    if amount == 0 {
        return Err("Refund amount must be greater than 0".to_string());
    }
    
    // Add to user balance
    update_user_balance(user, amount, true)
        .map_err(|e| format!("Failed to update user balance: {}", e))?;
    
    // Record transaction
    let description = format!("Refund for failed deployment (ID: {})", deployment_id);
    record_transaction(
        user,
        amount,
        TransactionType::Refund,
        description
    ).await;
    
    ic_cdk::println!("Successfully processed refund of {} e8s for user {}, deployment ID: {}", amount, user.to_text(), deployment_id);
    Ok(())
}

#[allow(dead_code)]
pub async fn process_deposit(user: Principal, amount: u64) -> Result<u64, String> {
    // Verify amount is valid
    if amount == 0 {
        return Err("Deposit amount must be greater than 0".to_string());
    }
    
    // Add to user balance
    let new_balance = update_user_balance(user, amount, true)
        .map_err(|e| format!("Failed to update user balance: {}", e))?;
    
    // Record transaction
    let description = format!("Deposit of {:.8} ICP", amount as f64 / 100_000_000.0);
    record_transaction(
        user,
        amount,
        TransactionType::Deposit,
        description
    ).await;
    
    Ok(new_balance)
}

#[allow(dead_code)]
pub async fn process_withdrawal(user: Principal, amount: u64) -> Result<u64, String> {
    // Verify amount is valid
    if amount == 0 {
        return Err("Withdrawal amount must be greater than 0".to_string());
    }
    
    // Check if user has sufficient balance
    if !check_user_balance(user, amount)
        .map_err(|e| format!("Failed to check user balance: {}", e))? 
    {
        return Err("Insufficient balance".to_string());
    }
    
    // Deduct from user balance
    let new_balance = update_user_balance(user, amount, false)
        .map_err(|e| format!("Failed to update user balance: {}", e))?;
    
    // Record transaction
    let description = format!("Withdrawal of {:.8} ICP", amount as f64 / 100_000_000.0);
    record_transaction(
        user,
        amount,
        TransactionType::Withdrawal,
        description
    ).await;
    
    Ok(new_balance)
}

#[allow(dead_code)]
pub async fn process_deployment_fee(user: Principal, amount: u64, deployment_id: &str) -> Result<(), String> {
    // Verify amount is valid
    if amount == 0 {
        return Err("Deployment fee must be greater than 0".to_string());
    }
    
    // Check if user has sufficient balance
    if !check_user_balance(user, amount)
        .map_err(|e| format!("Failed to check user balance: {}", e))? 
    {
        return Err("Insufficient balance for deployment fee".to_string());
    }
    
    // Deduct from user balance
    update_user_balance(user, amount, false)
        .map_err(|e| format!("Failed to update user balance: {}", e))?;
    
    // Record transaction
    let description = format!("Deployment fee for deployment (ID: {})", deployment_id);
    record_transaction(
        user,
        amount,
        TransactionType::DeploymentFee,
        description
    ).await;
    
    Ok(())
}

#[allow(dead_code)]
pub async fn process_admin_adjustment(user: Principal, amount: u64, reason: String) -> Result<(), String> {
    // Verify amount is valid
    if amount == 0 {
        return Err("Adjustment amount must be greater than 0".to_string());
    }
    
    // Add to user balance
    update_user_balance(user, amount, true)
        .map_err(|e| format!("Failed to update user balance: {}", e))?;
    
    // Record transaction
    record_transaction(
        user,
        amount,
        TransactionType::AdminAdjustment,
        reason
    ).await;
    
    Ok(())
}

#[allow(dead_code)]
pub async fn process_balance_payment(user: Principal, amount: u64, purpose: &str) -> Result<(), String> {
    // Verify amount is valid
    if amount == 0 {
        return Err("Payment amount must be greater than 0".to_string());
    }
    
    // Check if user has sufficient balance
    if !check_user_balance(user, amount)
        .map_err(|e| format!("Failed to check user balance: {}", e))? 
    {
        return Err("Insufficient balance".to_string());
    }
    
    // Deduct from user balance
    update_user_balance(user, amount, false)
        .map_err(|e| format!("Failed to update user balance: {}", e))?;
    
    // Record transaction
    record_transaction(
        user,
        amount,
        TransactionType::DeploymentFee,
        purpose.to_string()
    ).await;
    
    Ok(())
}

#[allow(dead_code)]
pub async fn process_balance_transfer(from: Principal, to: Principal, amount: u64, purpose: &str) -> Result<(), String> {
    // Verify amount is valid
    if amount == 0 {
        return Err("Transfer amount must be greater than 0".to_string());
    }
    
    // Check if sender has sufficient balance
    if !check_user_balance(from, amount)
        .map_err(|e| format!("Failed to check sender balance: {}", e))? 
    {
        return Err("Insufficient balance".to_string());
    }
    
    // Deduct from sender balance
    update_user_balance(from, amount, false)
        .map_err(|e| format!("Failed to update sender balance: {}", e))?;
    
    // Add to recipient balance
    update_user_balance(to, amount, true)
        .map_err(|e| format!("Failed to update recipient balance: {}", e))?;
    
    // Record transaction for sender
    let sender_description = format!("Transfer to {}: {}", to.to_text(), purpose);
    record_transaction(
        from,
        amount,
        TransactionType::Transfer,
        sender_description
    ).await;
    
    // Record transaction for recipient
    let recipient_description = format!("Transfer from {}: {}", from.to_text(), purpose);
    record_transaction(
        to,
        amount,
        TransactionType::Transfer,
        recipient_description
    ).await;
    
    Ok(())
} 