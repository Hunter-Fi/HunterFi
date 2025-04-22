use candid::{CandidType, Deserialize, Principal};
use serde::Serialize;
use std::collections::HashMap;

/// Enum representing the type of exchange
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExchangeType {
    ICPSwap,
    KongSwap,
    Sonic,
    ICDex,
}

/// Information about a token
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TokenInfo {
    pub canister_id: Principal,
    pub symbol: String,
    pub decimals: u8,
    pub standard: TokenStandard,
}

/// Token standard enum
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TokenStandard {
    ICRC1,
    ICRC2,
    DIP20,
    EXT,
    ICP,
}

/// Information about a trading pair
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct TradingPair {
    pub base_token: TokenInfo,
    pub quote_token: TokenInfo,
    pub exchange: ExchangeType,
}

/// Direction of a trade
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TradeDirection {
    Buy,
    Sell,
}

/// Parameters for a trade
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct TradeParams {
    pub pair: TradingPair,
    pub direction: TradeDirection,
    pub amount: u128,
    pub slippage_tolerance: f64,  // Represented as a percentage, e.g., 0.5 for 0.5%
    pub deadline_secs: Option<u64>,
}

/// Result of a trade operation
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct TradeResult {
    pub input_amount: u128,
    pub output_amount: u128,
    pub fee_amount: u128,
    pub price: f64,
    pub timestamp: u64,
    pub transaction_id: Option<String>,
}

/// Result of a quote request
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct QuoteResult {
    pub input_amount: u128,
    pub output_amount: u128,
    pub price: f64,
    pub fee_amount: u128,
    pub price_impact: f64,  // Price impact, represented as a percentage
}

/// Information about a liquidity pool
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct PoolInfo {
    pub pool_id: Principal,
    pub token0: TokenInfo,
    pub token1: TokenInfo,
    pub fee: u64,            // Trading fee, in parts per million (ppm) e.g. 3000 for 0.3%
    pub total_liquidity: u128,
    pub token0_reserves: u128,
    pub token1_reserves: u128,
}

/// Status of an exchange
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ExchangeStatus {
    pub exchange_type: ExchangeType,
    pub is_available: bool,
    pub supported_tokens: Vec<TokenInfo>,
    pub supported_pairs: Vec<TradingPair>,
    pub last_updated: u64,
}

/// Configuration for an exchange connector
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ExchangeConfig {
    pub exchange_type: ExchangeType,
    pub canister_id: Principal, // Usually the factory or router canister ID
    pub default_slippage: f64,
    pub max_slippage: f64,
    pub timeout_secs: u64,
    pub retry_count: u8,
}

/// Parameters for executing multiple trades in a batch
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BatchTradeParams {
    pub trades: Vec<TradeParams>,
    pub require_all_success: bool,
}

/// Result of executing multiple trades in a batch
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BatchTradeResult {
    pub results: Vec<Result<TradeResult, String>>, // String represents the error message if failed
    pub all_succeeded: bool,
    pub timestamp: u64,
}

/// Record of a past trade
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct TradeHistory {
    pub trade_id: String,
    pub pair: TradingPair,
    pub direction: TradeDirection,
    pub input_amount: u128,
    pub output_amount: u128,
    pub price: f64,
    pub timestamp: u64,
    pub status: TradeStatus,
    pub transaction_id: Option<String>,
}

/// Status of a trade
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TradeStatus {
    Pending,
    Completed,
    Failed,
    Refunded,
}

/// Parameters for adding liquidity
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct LiquidityParams {
    pub pair: TradingPair,
    pub token0_amount: u128, // Desired amount of token0
    pub token1_amount: u128, // Desired amount of token1
    // Consider adding min_amount fields for slippage control if the exchange supports it
    pub slippage_tolerance: f64,
    pub deadline_secs: Option<u64>,
}

/// Result of adding liquidity
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct LiquidityResult {
    pub liquidity_added: u128, // Amount of LP tokens received or liquidity units added
    pub token0_amount: u128, // Actual amount of token0 deposited
    pub token1_amount: u128, // Actual amount of token1 deposited
    pub pool_id: Principal,
    pub transaction_id: Option<String>,
    pub timestamp: u64,
} 