use candid::{CandidType, Deserialize};
use serde::Serialize;
use crate::types::{TokenMetadata, TradingPair, OrderType, Transaction, TransactionStatus};

/// Exchange price information
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct PriceInfo {
    pub base_token: TokenMetadata,
    pub quote_token: TokenMetadata,
    pub price: f64,         // Price in quote token
    pub timestamp: u64,     // Timestamp of the price data
}

/// Swap parameters
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SwapParams {
    pub trading_pair: TradingPair,
    pub direction: OrderType,
    pub amount_in: u128,
    pub min_amount_out: u128,
    pub max_slippage_percentage: u64,
}

/// Swap result
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SwapResult {
    Success {
        transaction_id: String,
        amount_in: u128,
        amount_out: u128,
    },
    Error(String),
}

/// Order status
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    PartiallyFilled,
    Filled,
    Canceled,
    Failed,
}

/// Order information
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct OrderInfo {
    pub order_id: String,
    pub trading_pair: TradingPair,
    pub direction: OrderType,
    pub amount_in: u128,
    pub amount_out: u128,
    pub status: OrderStatus,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Liquidity pool information
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct PoolInfo {
    pub canister_id: String,
    pub trading_pair: TradingPair,
    pub token0_reserve: u128,
    pub token1_reserve: u128,
    pub fee_percentage: u64,
}

/// Exchange trait - defines methods all exchange implementations must support
#[async_trait::async_trait]
pub trait Exchange {
    /// Get price information for a trading pair
    async fn get_price(&self, trading_pair: &TradingPair) -> Result<PriceInfo, String>;
    
    /// Execute a swap
    async fn swap(&self, params: SwapParams) -> Result<SwapResult, String>;
    
    /// Get order information
    async fn get_order_info(&self, order_id: &str) -> Result<OrderInfo, String>;
    
    /// Get liquidity pool information 
    async fn get_pool_info(&self, trading_pair: &TradingPair) -> Result<PoolInfo, String>;
    
    /// Calculate expected output amount
    async fn get_expected_output(
        &self,
        trading_pair: &TradingPair,
        direction: OrderType,
        amount_in: u128,
    ) -> Result<u128, String>;
    
    /// Check if trading pair is supported
    async fn is_trading_pair_supported(&self, trading_pair: &TradingPair) -> bool;
    
    /// Get exchange name
    fn get_name(&self) -> String;
}