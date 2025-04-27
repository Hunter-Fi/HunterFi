use async_trait::async_trait;
use candid::Principal;

use crate::error::ExchangeResult;
use crate::types::*;

/// Interface for basic exchange operations
#[async_trait]
pub trait Exchange {
    /// Get the type of the exchange
    fn get_exchange_type(&self) -> ExchangeType;
    
    /// Get the status of the exchange
    async fn get_status(&self) -> ExchangeResult<ExchangeStatus>;
    
    /// Query token balance
    async fn get_token_balance(&self, token: &TokenInfo, owner: &Principal) -> ExchangeResult<u128>;
    
    /// Check if a trading pair is supported
    async fn is_pair_supported(&self, base: &TokenInfo, quote: &TokenInfo) -> ExchangeResult<bool>;
}

/// Interface for trading functionalities
#[async_trait]
pub trait Trading: Exchange {
    /// Get a trading quote
    async fn get_quote(&self, params: &TradeParams) -> ExchangeResult<QuoteResult>;
    
    /// Execute a trade
    async fn execute_trade(&self, params: &TradeParams) -> ExchangeResult<TradeResult>;
    
    /// Execute multiple trades in a batch
    async fn execute_batch_trade(&self, params: &BatchTradeParams) -> ExchangeResult<BatchTradeResult>;
    
    /// Get trading history
    async fn get_trade_history(&self, user: &Principal, limit: usize, offset: usize) -> ExchangeResult<Vec<TradeHistory>>;
}

/// Interface for liquidity pool operations
#[async_trait]
pub trait LiquidityPool: Exchange {
    /// Get information about a liquidity pool
    async fn get_pool_info(&self, base: &TokenInfo, quote: &TokenInfo) -> ExchangeResult<PoolInfo>;
    
    /// Add liquidity to a pool
    async fn add_liquidity(&self, params: &LiquidityParams) -> ExchangeResult<LiquidityResult>;
    
    /// Remove liquidity from a pool
    async fn remove_liquidity(&self, pool_id: &Principal, liquidity_amount: u128, min_token0: u128, min_token1: u128) -> ExchangeResult<LiquidityResult>;
    
    /// Get a user's liquidity in a specific pool
    async fn get_user_liquidity(&self, pool_id: &Principal, user: &Principal) -> ExchangeResult<u128>;
}

/// Interface for token operations within the exchange
#[async_trait]
pub trait TokenOperations: Exchange {
    /// Deposit tokens into the exchange
    async fn deposit_token(&self, params: &TradeParams,token: &TokenInfo, amount: u128) -> ExchangeResult<u128>;
    
    /// Withdraw tokens from the exchange
    async fn withdraw_token(&self, params: &TradeParams,token: &TokenInfo, amount: u128) -> ExchangeResult<u128>;
    
    /// Get the user's unused token balance (e.g., balance not in orders or pools)
    async fn get_unused_balance(&self, params: &TradeParams, user: &Principal) -> ExchangeResult<(u128,u128)>;
    
    /// Query the user's total balance within the exchange
    async fn get_exchange_balance(&self, token: &TokenInfo, user: &Principal) -> ExchangeResult<(u128,u128)>;
} 