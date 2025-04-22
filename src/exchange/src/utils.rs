use candid::Principal;
use crate::types::*;
use crate::error::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Converts a Principal to a Blob representation for subaccounts.
pub fn principal_to_subaccount(principal: &Principal) -> Vec<u8> {
    let mut bytes = principal.as_slice().to_vec();
    let mut default_arr = vec![0; 32];
    default_arr[0] = bytes.len() as u8;
    
    for (i, byte) in bytes.iter().enumerate() {
        if i < 31 {
            default_arr[i + 1] = *byte;
        }
    }
    
    default_arr
}

/// Calculates the slippage percentage.
pub fn calculate_slippage(expected: u128, actual: u128) -> f64 {
    if expected == 0 {
        return 0.0;
    }
    
    let expected_f = expected as f64;
    let actual_f = actual as f64;
    
    if actual_f >= expected_f {
        return 0.0;
    }
    
    ((expected_f - actual_f) / expected_f) * 100.0
}

/// Checks if the slippage exceeds the tolerance.
pub fn is_slippage_exceeded(expected: u128, actual: u128, tolerance: f64) -> bool {
    calculate_slippage(expected, actual) > tolerance
}

/// Gets the current timestamp in seconds.
pub fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

/// Gets the current timestamp in nanoseconds.
pub fn current_timestamp_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as u64
}

/// Generates a unique trade ID.
pub fn generate_trade_id(user: &Principal, timestamp: u64) -> String {
    format!("{}_{}", user.to_string(), timestamp)
}

/// Validates if the token standard is compatible.
pub fn validate_token_standard(token: &TokenInfo) -> ExchangeResult<()> {
    match token.standard {
        TokenStandard::ICRC1 | TokenStandard::ICRC2 | TokenStandard::DIP20 | 
        TokenStandard::EXT | TokenStandard::ICP => Ok(()),
        _ => Err(ExchangeError::InvalidTokenStandard),
    }
}

/// Converts an amount to a human-readable format considering decimals.
pub fn amount_to_human_readable(amount: u128, decimals: u8) -> f64 {
    let divisor = 10u128.pow(decimals as u32) as f64;
    amount as f64 / divisor
}

/// Converts a human-readable amount to its on-chain representation considering decimals.
pub fn amount_from_human_readable(amount: f64, decimals: u8) -> u128 {
    let multiplier = 10u128.pow(decimals as u32) as f64;
    (amount * multiplier) as u128
}

/// Validates the trade parameters.
pub fn validate_trade_params(params: &TradeParams) -> ExchangeResult<()> {
    // Validate amount
    if params.amount == 0 {
        return Err(ExchangeError::InvalidAmount);
    }
    
    // Validate slippage tolerance
    if params.slippage_tolerance < 0.0 || params.slippage_tolerance > 100.0 {
        return Err(ExchangeError::InvalidParameters("Slippage tolerance must be between 0 and 100".to_string()));
    }
    
    // Validate deadline (if it exists)
    if let Some(deadline) = params.deadline_secs {
        if deadline < current_timestamp_secs() {
            return Err(ExchangeError::InvalidParameters("Deadline has passed".to_string()));
        }
    }
    
    // Validate token standards
    validate_token_standard(&params.pair.base_token)?;
    validate_token_standard(&params.pair.quote_token)?;
    
    Ok(())
} 