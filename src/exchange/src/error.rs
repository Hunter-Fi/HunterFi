use candid::{CandidType, Deserialize};
use serde::Serialize;
use std::fmt;

/// Errors related to exchange operations
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExchangeError {
    // General errors
    NotImplemented,
    InternalError(String),
    
    // Network and communication related errors
    CanisterCallError(String),
    Timeout,
    RateLimit,
    
    // Trading related errors
    InsufficientFunds,
    SlippageExceeded,
    PriceChanged,
    TradeRejected(String),
    TransactionFailed(String),
    
    // Liquidity related errors
    InsufficientLiquidity,
    PoolNotFound,
    
    // Token related errors
    UnsupportedToken(String),
    InvalidTokenStandard,
    TokenTransferFailed(String),
    TokenApprovalFailed(String),
    
    // Parameter related errors
    InvalidParameters(String),
    InvalidAmount,
    
    // Permission related errors
    Unauthorized,
    
    // User action related errors
    UserRejected,
    
    // Other errors
    Unknown(String),
}

impl fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotImplemented => write!(f, "Functionality not implemented yet"),
            Self::InternalError(msg) => write!(f, "Internal error: {}", msg),
            Self::CanisterCallError(msg) => write!(f, "Canister call error: {}", msg),
            Self::Timeout => write!(f, "Operation timed out"),
            Self::RateLimit => write!(f, "Rate limit reached"),
            Self::InsufficientFunds => write!(f, "Insufficient funds"),
            Self::SlippageExceeded => write!(f, "Slippage tolerance exceeded"),
            Self::PriceChanged => write!(f, "Price has changed"),
            Self::TradeRejected(reason) => write!(f, "Trade rejected: {}", reason),
            Self::TransactionFailed(reason) => write!(f, "Transaction failed: {}", reason),
            Self::InsufficientLiquidity => write!(f, "Insufficient liquidity"),
            Self::PoolNotFound => write!(f, "Liquidity pool not found"),
            Self::UnsupportedToken(token) => write!(f, "Unsupported token: {}", token),
            Self::InvalidTokenStandard => write!(f, "Invalid token standard"),
            Self::TokenTransferFailed(reason) => write!(f, "Token transfer failed: {}", reason),
            Self::TokenApprovalFailed(reason) => write!(f, "Token approval failed: {}", reason),
            Self::InvalidParameters(msg) => write!(f, "Invalid parameters: {}", msg),
            Self::InvalidAmount => write!(f, "Invalid amount"),
            Self::Unauthorized => write!(f, "Unauthorized operation"),
            Self::UserRejected => write!(f, "User rejected operation"),
            Self::Unknown(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for ExchangeError {}

/// Result type for exchange operations
pub type ExchangeResult<T> = Result<T, ExchangeError>; 