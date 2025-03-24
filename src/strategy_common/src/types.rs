use candid::{CandidType, Deserialize, Principal};
use ic_ledger_types::Tokens;
use serde::Serialize;
use std::collections::HashMap;
use serde_bytes::ByteBuf;

/// Defines available strategy types in the system
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum StrategyType {
    DollarCostAveraging,
    ValueAveraging,
    FixedBalance,
    LimitOrder,
    SelfHedging,
}

/// Defines the status of a strategy
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum StrategyStatus {
    Created,
    Running,
    Paused,
    EmergencyStopped,
    Terminated,
}

/// Defines the supported DEXes
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Exchange {
    ICPSwap,
    KongSwap,
    Sonic,
    InfinitySwap,
    ICDex,
}

/// Defines token metadata
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TokenMetadata {
    pub canister_id: Principal,
    pub symbol: String,
    pub decimals: u8,
}

/// Defines a trading pair
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TradingPair {
    pub base_token: TokenMetadata,
    pub quote_token: TokenMetadata,
}

/// Defines order type (buy/sell)
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum OrderType {
    Buy,
    Sell,
}

/// Defines a trade transaction
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
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
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Executed,
    Failed,
}

/// Result type for strategy operations
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum StrategyResult {
    Success,
    Error(String),
}

/// Strategy metadata
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
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

/// Factory canister response for strategy deployment
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum DeploymentResult {
    Success(Principal),
    Error(String),
} 