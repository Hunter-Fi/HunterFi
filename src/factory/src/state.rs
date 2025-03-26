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

// ICP Ledger canister ID
pub const ICP_LEDGER_CANISTER_ID: &str = "ryjl3-tyaaa-aaaaa-aaaba-cai";

// Default deployment fee (1 ICP)
pub const DEFAULT_DEPLOYMENT_FEE: u64 = 100_000_000; // 1 ICP in e8s

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

    // Map to store owner to strategies mapping
    pub static OWNER_STRATEGIES: RefCell<HashMap<Principal, Vec<Principal>>> = RefCell::new(HashMap::new());

    // Deployment fee in e8s (100_000_000 = 1 ICP)
    pub static DEPLOYMENT_FEE: Cell<u64> = Cell::new(DEFAULT_DEPLOYMENT_FEE);
    
    // Set of admin principals
    pub static ADMINS: RefCell<HashSet<Principal>> = RefCell::new(HashSet::new());
    
    // Counter for deployment IDs
    pub static DEPLOYMENT_ID_COUNTER: Cell<u64> = Cell::new(0);
}

// WASM module record for strategy installations
#[derive(Serialize, Deserialize, Clone, Debug)]
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
) -> Result<DeploymentRecord, String> {
    if let Some(mut record) = get_deployment_record(deployment_id) {
        record.status = status;
        record.last_updated = ic_cdk::api::time();
        
        if let Some(cid) = canister_id {
            record.canister_id = Some(cid);
        }
        
        if let Some(error) = error_message {
            record.error_message = Some(error);
        }
        
        store_deployment_record(record.clone());
        Ok(record)
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
            if let Ok(record) = candid::decode_one(&record_bytes.0) {
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

// Upgrade data structure
#[derive(Serialize, Deserialize, CandidType)]
pub struct UpgradeData {
    pub owner_strategies: HashMap<Principal, Vec<Principal>>,
    pub admins: HashSet<Principal>,
}

// Pre-upgrade data
pub fn get_upgrade_data() -> UpgradeData {
    UpgradeData {
        owner_strategies: OWNER_STRATEGIES.with(|m| m.borrow().clone()),
        admins: ADMINS.with(|a| a.borrow().clone()),
    }
}

// Restore upgrade data
pub fn restore_upgrade_data(data: UpgradeData) {
    OWNER_STRATEGIES.with(|m| *m.borrow_mut() = data.owner_strategies);
    ADMINS.with(|a| *a.borrow_mut() = data.admins);
} 