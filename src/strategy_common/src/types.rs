use candid::{CandidType, Deserialize, Principal};
use serde::Serialize;
use std::collections::HashMap;
use serde_bytes::ByteBuf;

/// Defines available strategy types in the system
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StrategyType {
    DollarCostAveraging,
    ValueAveraging,
    FixedBalance,
    LimitOrder,
    SelfHedging,
}

/// Defines the status of a strategy
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StrategyStatus {
    Created,
    Running,
    Paused,
    EmergencyStopped,
    Terminated,
}

/// Defines the supported DEXes
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Exchange {
    ICPSwap,
    KongSwap,
    Sonic,
    InfinitySwap,
    ICDex,
}

/// Defines token metadata
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenMetadata {
    pub canister_id: Principal,
    pub symbol: String,
    pub decimals: u8,
}

/// Defines a trading pair
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TradingPair {
    pub base_token: TokenMetadata,
    pub quote_token: TokenMetadata,
}

/// Defines order type (buy/sell)
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrderType {
    Buy,
    Sell,
}

/// Defines a trade transaction
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Transaction {
    pub id: u64,
    pub timestamp: u64,
    pub direction: OrderType,
    pub input_token: TokenMetadata,
    pub output_token: TokenMetadata,
    pub input_amount: u128,
    pub output_amount: u128,
    pub exchange: Exchange,
    pub status: TransactionStatus,
    pub error_message: Option<String>,
}

/// Defines transaction status
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransactionStatus {
    Pending,
    Executed,
    Failed,
}

/// Result type for strategy operations
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StrategyResult {
    Success,
    Error(String),
}

/// Strategy metadata
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StrategyMetadata {
    pub canister_id: Principal,
    pub strategy_type: StrategyType,
    pub owner: Principal,
    pub created_at: u64,
    pub status: StrategyStatus,
    pub exchange: Exchange,
    pub trading_pair: TradingPair,
}

/// Dollar Cost Averaging Strategy Configuration
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DCAConfig {
    pub exchange: Exchange,              // Exchange type
    pub base_token: TokenMetadata,       // Base token
    pub quote_token: TokenMetadata,      // Quote token
    pub amount_per_execution: u128,      // Amount per execution
    pub interval_secs: u64,              // Execution interval (seconds)
    pub max_executions: Option<u64>,     // Maximum executions, None means unlimited
    pub slippage_tolerance: f64,         // Slippage tolerance (percentage, e.g., 1.0 = 1%)
}

/// Value Averaging Strategy Configuration
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ValueAvgConfig {
    pub exchange: Exchange,              // Exchange type
    pub base_token: TokenMetadata,       // Base token
    pub quote_token: TokenMetadata,      // Quote token
    pub target_value_increase: u128,     // Target value increase per period
    pub interval_secs: u64,              // Execution interval (seconds)
    pub max_executions: Option<u64>,     // Maximum executions, None means unlimited
    pub slippage_tolerance: f64,         // Slippage tolerance
}

/// Fixed Balance Strategy Configuration
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct FixedBalanceConfig {
    pub exchange: Exchange,              // Exchange type
    pub token_allocations: HashMap<TokenMetadata, f64>, // Token allocation ratios
    pub rebalance_threshold: f64,        // Rebalance threshold (percentage deviation)
    pub interval_secs: u64,              // Check interval (seconds)
    pub slippage_tolerance: f64,         // Slippage tolerance
}

/// Limit Order Strategy Configuration
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct LimitOrderConfig {
    pub exchange: Exchange,              // Exchange type
    pub base_token: TokenMetadata,       // Base token
    pub quote_token: TokenMetadata,      // Quote token
    pub order_type: OrderType,           // Order type (buy/sell)
    pub price: u128,                     // Price
    pub amount: u128,                    // Amount
    pub expiration: Option<u64>,         // Expiration time (seconds), None means never expire
}

/// Self-Hedging Strategy Configuration
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SelfHedgingConfig {
    pub exchange: Exchange,              // Exchange type
    pub trading_token: TokenMetadata,    // Token to generate volume for
    pub transaction_size: u128,          // Size of each transaction (amount of tokens)
    pub order_split_type: OrderSplitType,// Type of order splitting to perform
    pub check_interval_secs: u64,        // Execution interval (seconds)
    pub slippage_tolerance: f64,         // Slippage tolerance
}

/// Order splitting strategy type
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum OrderSplitType {
    NoSplit,                            // No splitting, single buy and sell
    SplitBuy,                           // Split buy orders only
    SplitSell,                          // Split sell orders only
    SplitBoth,                          // Split both buy and sell orders
}

/// Deployment process status
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeploymentStatus {
    PendingPayment,
    AuthorizationConfirmed,
    PaymentReceived,
    CanisterCreated,
    CodeInstalled,
    Initialized,
    Deployed,
    DeploymentCancelled,
    DeploymentFailed,
    Refunding,
    Refunded,
}

/// Deployment record
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeploymentRecord {
    pub deployment_id: String,
    pub strategy_type: StrategyType,
    pub owner: Principal,
    pub fee_amount: u64,
    pub request_time: u64,
    pub status: DeploymentStatus,
    pub canister_id: Option<Principal>,
    pub config_data: ByteBuf,  // Serialized config
    pub error_message: Option<String>,
    pub last_updated: u64,
}

/// Factory canister response for deployment requests
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DeploymentRequest {
    pub deployment_id: String,
    pub fee_amount: u64,
    pub strategy_type: StrategyType,
}

/// Factory canister response for strategy deployment
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DeploymentResult {
    Success(Principal),
    Error(String),
}

/// Trait for strategy configurations to provide common validation functionality
pub trait StrategyConfig {
    /// Validate the configuration parameters
    fn validate(&self) -> Result<(), String>;
    
    /// Get the strategy type
    fn get_strategy_type(&self) -> StrategyType;
    
    /// Get the exchange being used
    fn get_exchange(&self) -> Exchange;
}

impl StrategyConfig for DCAConfig {
    fn validate(&self) -> Result<(), String> {
        if self.amount_per_execution == 0 {
            return Err("Amount per execution must be greater than 0".to_string());
        }
        
        if self.interval_secs == 0 {
            return Err("Interval must be greater than 0".to_string());
        }
        
        Ok(())
    }
    
    fn get_strategy_type(&self) -> StrategyType {
        StrategyType::DollarCostAveraging
    }
    
    fn get_exchange(&self) -> Exchange {
        self.exchange.clone()
    }
}

impl StrategyConfig for ValueAvgConfig {
    fn validate(&self) -> Result<(), String> {
        if self.target_value_increase == 0 {
            return Err("Target value increase must be greater than 0".to_string());
        }
        
        if self.interval_secs == 0 {
            return Err("Interval must be greater than 0".to_string());
        }
        
        Ok(())
    }
    
    fn get_strategy_type(&self) -> StrategyType {
        StrategyType::ValueAveraging
    }
    
    fn get_exchange(&self) -> Exchange {
        self.exchange.clone()
    }
}

impl StrategyConfig for FixedBalanceConfig {
    fn validate(&self) -> Result<(), String> {
        if self.token_allocations.is_empty() {
            return Err("Token allocations cannot be empty".to_string());
        }
        
        if self.interval_secs == 0 {
            return Err("Interval must be greater than 0".to_string());
        }
        
        Ok(())
    }
    
    fn get_strategy_type(&self) -> StrategyType {
        StrategyType::FixedBalance
    }
    
    fn get_exchange(&self) -> Exchange {
        self.exchange.clone()
    }
}

impl StrategyConfig for LimitOrderConfig {
    fn validate(&self) -> Result<(), String> {
        if self.amount == 0 {
            return Err("Amount must be greater than 0".to_string());
        }
        
        if self.price == 0 {
            return Err("Price must be greater than 0".to_string());
        }
        
        Ok(())
    }
    
    fn get_strategy_type(&self) -> StrategyType {
        StrategyType::LimitOrder
    }
    
    fn get_exchange(&self) -> Exchange {
        self.exchange.clone()
    }
}

impl StrategyConfig for SelfHedgingConfig {
    fn validate(&self) -> Result<(), String> {
        if self.transaction_size == 0 {
            return Err("Transaction size must be greater than 0".to_string());
        }
        
        if self.check_interval_secs == 0 {
            return Err("Check interval must be greater than 0".to_string());
        }
        
        Ok(())
    }
    
    fn get_strategy_type(&self) -> StrategyType {
        StrategyType::SelfHedging
    }
    
    fn get_exchange(&self) -> Exchange {
        self.exchange.clone()
    }
} 