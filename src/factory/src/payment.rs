use candid::{CandidType, Principal};
use ic_cdk::api::call::{call, CallResult};
use ic_cdk::api::time;
use serde::Deserialize;
use std::fmt;
use strategy_common::types::DeploymentStatus;

use crate::state::{
    ICP_LEDGER_CANISTER_ID,
    update_deployment_status,
    update_user_balance, 
    TransactionType, 
    record_transaction,
    check_user_balance,
    get_user_account,
    get_deployment_records_by_status,
};

// Payment module error type for consistent error handling
#[derive(Debug, Clone)]
pub enum PaymentError {
    InvalidAmount(String),
    InsufficientBalance(String),
    TransferFailed(String),
    UserNotFound(String),
    SystemError(String),
    InvalidPrincipal(String),
    #[allow(dead_code)]
    OperationFailed(String),
    TransactionError(String),
}

impl fmt::Display for PaymentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PaymentError::InvalidAmount(msg) => write!(f, "Invalid amount: {}", msg),
            PaymentError::InsufficientBalance(msg) => write!(f, "Insufficient balance: {}", msg),
            PaymentError::TransferFailed(msg) => write!(f, "Transfer failed: {}", msg),
            PaymentError::UserNotFound(msg) => write!(f, "User not found: {}", msg),
            PaymentError::SystemError(msg) => write!(f, "System error: {}", msg),
            PaymentError::InvalidPrincipal(msg) => write!(f, "Invalid principal: {}", msg),
            PaymentError::OperationFailed(msg) => write!(f, "Operation failed: {}", msg),
            PaymentError::TransactionError(msg) => write!(f, "Transaction error: {}", msg),
        }
    }
}

impl PaymentError {
    pub fn to_string(&self) -> String {
        format!("{}", self)
    }
}

// Type alias for payment results
type PaymentResult<T> = Result<T, PaymentError>;

// Transaction constants
pub struct PaymentConfig {
    pub max_deposit_amount: u64,   // Maximum deposit amount
    pub min_deposit_amount: u64,   // Minimum deposit amount
    pub icp_transfer_fee: u128,    // ICP transfer fee
    pub max_withdrawal_retries: u8, // Maximum withdrawal retry attempts
    pub min_cycles_balance: u64,   // Minimum cycles balance
}

// Global payment configuration
pub static PAYMENT_CONFIG: PaymentConfig = PaymentConfig {
    max_deposit_amount: 10_000_000_000,  // 100 ICP
    min_deposit_amount: 1_000_000,       // 0.01 ICP
    icp_transfer_fee: 10_000,            // 0.0001 ICP
    max_withdrawal_retries: 3,           // Maximum withdrawal retry attempts
    min_cycles_balance: 1_000_000_000,   // Minimum cycles balance (1T)
};

// ICRC1 Account type
#[derive(CandidType, Deserialize, Debug, Clone)]
struct Account {
    owner: Principal,
    subaccount: Option<Vec<u8>>,
}

// ICRC1 Transfer arguments
#[derive(CandidType, Debug, Clone)]
struct TransferArgs {
    to: Account,
    amount: u128,
    fee: Option<u128>,
    memo: Option<Vec<u8>>,
    created_at_time: Option<u64>,
}

// Helper function to validate payment amount
fn validate_amount(amount: u64, min: u64, max: u64, operation: &str) -> PaymentResult<()> {
    if amount == 0 {
        return Err(PaymentError::InvalidAmount(format!(
            "{} amount must be greater than 0", operation
        )));
    }
    
    if amount < min {
        return Err(PaymentError::InvalidAmount(format!(
            "{} amount cannot be less than {} ICP", 
            operation, min as f64 / 100_000_000.0
        )));
    }
    
    if amount > max {
        return Err(PaymentError::InvalidAmount(format!(
            "{} amount cannot exceed {} ICP", 
            operation, max as f64 / 100_000_000.0
        )));
    }
    
    Ok(())
}

// Helper function to record payment transaction
async fn record_payment_transaction(
    user: Principal, 
    amount: u64, 
    transaction_type: &TransactionType,
    description: &str
) {
    let _ = record_transaction(
        user,
        amount,
        transaction_type.clone(),
        description.to_string()
    ).await;
    
    ic_cdk::println!(
        "Transaction: User {} - {:?} - {} e8s - {}", 
        user.to_text(), 
        transaction_type, 
        amount, 
        description
    );
}

/// Process user deposit
pub async fn process_deposit(user: Principal, amount: u64) -> PaymentResult<u64> {
    // Security check: Amount must be within reasonable range
    validate_amount(
        amount, 
        PAYMENT_CONFIG.min_deposit_amount, 
        PAYMENT_CONFIG.max_deposit_amount,
        "Deposit"
    )?;
    
    // Verify caller is not anonymous
    if user == Principal::anonymous() {
        return Err(PaymentError::InvalidPrincipal("Anonymous identity cannot make deposits".to_string()));
    }
    
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID)
        .map_err(|_| PaymentError::SystemError("Invalid ICP ledger ID".to_string()))?;
    
    let factory_id = ic_cdk::id();
    
    // Build transfer parameters
    let transfer_args = TransferArgs {
        to: Account {
            owner: factory_id,
            subaccount: None,
        },
        amount: amount as u128,
        fee: Some(PAYMENT_CONFIG.icp_transfer_fee),
        memo: Some(format!("HunterFi deposit from {}", user).as_bytes().to_vec()),
        created_at_time: Some(time()),
    };
    
    // Call ICRC1 ledger to transfer
    let transfer_result: CallResult<(u128,)> = call(ledger_id, "icrc1_transfer", (transfer_args,)).await;
    
    match transfer_result {
        Ok(_) => {
            // Update user balance
            let new_balance = update_user_balance(user, amount, true)
                .map_err(|e| PaymentError::SystemError(e))?;
            
            // Record transaction
            let description = format!("Deposit of {:.8} ICP", amount as f64 / 100_000_000.0);
            record_payment_transaction(user, amount, &TransactionType::Deposit, &description).await;
            
            Ok(new_balance)
        },
        Err((code, message)) => {
            ic_cdk::println!("User {} deposit failed: code={:?}, message={}", user.to_text(), code, message);
            Err(PaymentError::TransferFailed(format!("Deposit failed: {}", message)))
        }
    }
}

/// Process payment from user's balance for deployment
pub async fn process_balance_payment(user: Principal, amount: u64, purpose: &str) -> PaymentResult<()> {
    // Check if amount is valid
    validate_amount(amount, 1, u64::MAX, "Payment")?;
    
    // Check if user has sufficient balance
    if !check_user_balance(user, amount)
        .map_err(|e| PaymentError::SystemError(e))? 
    {
        return Err(PaymentError::InsufficientBalance(format!(
            "{:.8} ICP required, but your balance is too low", 
            amount as f64 / 100_000_000.0
        )));
    }
    
    // Deduct from user balance
    update_user_balance(user, amount, false)
        .map_err(|e| PaymentError::SystemError(e))?;
    
    // Record transaction
    record_payment_transaction(user, amount, &TransactionType::DeploymentFee, purpose).await;
    
    Ok(())
}

/// Process refund back to user balance
pub async fn process_balance_refund(user: Principal, amount: u64, deployment_id: &str) -> PaymentResult<()> {
    // Verify refund amount is valid
    validate_amount(amount, 1, u64::MAX, "Refund")?;
    
    // Add to user balance
    update_user_balance(user, amount, true)
        .map_err(|e| PaymentError::SystemError(e))?;
    
    // Record transaction
    let description = format!("Refund for deployment: {}", deployment_id);
    record_payment_transaction(user, amount, &TransactionType::Refund, &description).await;
    
    // Update deployment status to refunded
    let _ = update_deployment_status(
        deployment_id, 
        DeploymentStatus::Refunded, 
        None, 
        Some("Deployment fee refunded".to_string())
    );
    
    Ok(())
}

/// Allow user to withdraw their funds to external wallet
pub async fn user_withdraw_funds(user: Principal, amount: u64) -> PaymentResult<u64> {
    // Security checks
    if amount == 0 {
        return Err(PaymentError::InvalidAmount("Withdrawal amount must be greater than 0".to_string()));
    }
    
    if amount > PAYMENT_CONFIG.max_deposit_amount {
        return Err(PaymentError::InvalidAmount(format!(
            "Withdrawal amount cannot exceed {} ICP", 
            PAYMENT_CONFIG.max_deposit_amount as f64 / 100_000_000.0
        )));
    }
    
    if user == Principal::anonymous() {
        return Err(PaymentError::InvalidPrincipal("Anonymous identity cannot withdraw funds".to_string()));
    }
    
    // Check if user has sufficient balance
    let user_account = get_user_account(user);
    
    if user_account.balance < amount {
        return Err(PaymentError::InsufficientBalance(format!(
            "You have {:.8} ICP but requested {:.8} ICP",
            user_account.balance as f64 / 100_000_000.0,
            amount as f64 / 100_000_000.0
        )));
    }
    
    // Verify canister has sufficient cycles
    let canister_balance = ic_cdk::api::canister_balance();
    
    if canister_balance < PAYMENT_CONFIG.min_cycles_balance {
        return Err(PaymentError::SystemError(
            "System cycles balance too low for safe operation. Please try again later or contact support.".to_string()
        ));
    }
    
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID)
        .map_err(|_| PaymentError::SystemError("Invalid ICP ledger ID".to_string()))?;
    
    // Build transfer parameters
    let transfer_args = TransferArgs {
        to: Account {
            owner: user,
            subaccount: None,
        },
        amount: amount as u128,
        fee: Some(PAYMENT_CONFIG.icp_transfer_fee),
        memo: Some(format!("HunterFi withdrawal to {}", user).as_bytes().to_vec()),
        created_at_time: Some(time()),
    };
    
    // Call ICRC1 ledger to transfer
    let transfer_result: CallResult<(u128,)> = call(ledger_id, "icrc1_transfer", (transfer_args,)).await;
    
    match transfer_result {
        Ok(_) => {
            // Deduct from user balance
            let new_balance = update_user_balance(user, amount, false)
                .map_err(|e| PaymentError::SystemError(e))?;
            
            // Record transaction
            let description = format!("Withdrawal of {:.8} ICP", amount as f64 / 100_000_000.0);
            record_payment_transaction(user, amount, &TransactionType::Withdrawal, &description).await;
            
            ic_cdk::println!("User {} successfully withdrew {} e8s", user.to_text(), amount);
            Ok(new_balance)
        },
        Err((code, message)) => {
            ic_cdk::println!("User {} withdrawal failed: code={:?}, message={}", user.to_text(), code, message);
            Err(PaymentError::TransferFailed(format!("Withdrawal failed: {}", message)))
        }
    }
}

/// Withdraw funds from system to external account (admin only)
pub async fn withdraw_funds(recipient: Principal, amount: u64) -> Result<(), String> {
    // Security checks
    if amount == 0 {
        return Err("Withdrawal amount must be greater than 0".to_string());
    }
    
    if amount > PAYMENT_CONFIG.max_deposit_amount {
        return Err(format!(
            "Withdrawal amount cannot exceed {} ICP", 
            PAYMENT_CONFIG.max_deposit_amount as f64 / 100_000_000.0
        ));
    }
    
    if recipient == Principal::anonymous() {
        return Err("Cannot withdraw to anonymous identity".to_string());
    }
    
    // Verify canister has sufficient cycles
    let canister_balance = ic_cdk::api::canister_balance();
    
    if canister_balance < PAYMENT_CONFIG.min_cycles_balance {
        return Err(format!(
            "Canister cycles balance too low for safe operation. Current balance: {} cycles",
            canister_balance
        ));
    }
    
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID)
        .map_err(|_| "Invalid ICP ledger ID".to_string())?;
    
    let args = TransferArgs {
        to: Account {
            owner: recipient,
            subaccount: None,
        },
        amount: amount as u128,
        fee: Some(PAYMENT_CONFIG.icp_transfer_fee),
        memo: Some(b"Withdrawal from HunterFi Factory".to_vec()),
        created_at_time: Some(time()),
    };
    
    // Use retry logic to enhance reliability
    let mut retries = 0;
    let mut last_error = "Unknown error".to_string();
    
    while retries < PAYMENT_CONFIG.max_withdrawal_retries {
        let call_result: CallResult<(u128,)> = call(ledger_id, "icrc1_transfer", (args.clone(),)).await;
        
        match call_result {
            Ok(_) => {
                ic_cdk::println!("Successfully withdrawn {} e8s to account {}", amount, recipient.to_text());
                return Ok(());
            },
            Err((code, message)) => {
                last_error = format!("code={:?}, message={}", code, message);
                retries += 1;
                
                // Check if temporary error
                let is_temporary_error = match code {
                    ic_cdk::api::call::RejectionCode::SysFatal => false,
                    ic_cdk::api::call::RejectionCode::SysTransient => true,
                    _ => false
                };
                
                if is_temporary_error && retries < PAYMENT_CONFIG.max_withdrawal_retries {
                    // IC environment doesn't have sleep, if this is the last attempt, exit directly
                    if retries == PAYMENT_CONFIG.max_withdrawal_retries - 1 {
                        break;
                    }
                    
                    // Simple retry without waiting - IC environment has no reliable waiting mechanism
                    ic_cdk::println!("Withdrawal temporarily failed, retrying immediately ({}/{})", retries, PAYMENT_CONFIG.max_withdrawal_retries);
                    continue;
                }
                
                break;
            }
        }
    }
    
    Err(format!("Withdrawal failed after {} attempts: {}", retries, last_error))
}

/// Process all records in DeploymentFailed status for refunds
#[allow(dead_code)]
pub async fn process_failed_deployments() -> Result<usize, String> {
    // Get all failed deployments
    let failed_deployments = get_deployment_records_by_status(DeploymentStatus::DeploymentFailed);
    let mut processed_count = 0;
    let mut errors = Vec::new();
    
    for record in failed_deployments {
        // Skip already processed refunds
        if record.status == DeploymentStatus::Refunded {
            continue;
        }
        
        // Process refund (add back to user balance)
        match process_balance_refund(record.owner, record.fee_amount, &record.deployment_id).await {
            Ok(_) => {
                // Update deployment status
                match update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::Refunded,
                    None,
                    None
                ) {
                    Ok(_) => {
                        processed_count += 1;
                        ic_cdk::println!("Successfully processed refund for deployment ID {}", record.deployment_id);
                    },
                    Err(e) => {
                        ic_cdk::println!("Failed to update status for deployment {}: {}", record.deployment_id, e);
                        errors.push(format!("Status update error (ID: {}): {}", record.deployment_id, e));
                    }
                }
            },
            Err(e) => {
                ic_cdk::println!("Failed to process refund for deployment {}: {:?}", record.deployment_id, e);
                errors.push(format!("Refund error (ID: {}): {:?}", record.deployment_id, e));
            }
        }
    }
    
    // Return processing results and error details
    if !errors.is_empty() && processed_count == 0 {
        // Only return error if no records were processed
        Err(format!("Could not process any refunds. Errors: {}", errors.join(", ")))
    } else {
        Ok(processed_count)
    }
}

// Helper function to convert PaymentError to String for API compatibility
pub fn payment_error_to_string(error: PaymentError) -> String {
    error.to_string()
} 