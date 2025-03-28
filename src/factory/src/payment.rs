use candid::{CandidType, Principal};
use ic_cdk::api::call::{call, CallResult};
use ic_cdk::api::time;
use serde::Deserialize;
use std::time::Duration;
use strategy_common::types::DeploymentStatus;

use crate::state::{
    ICP_LEDGER_CANISTER_ID,
    get_deployment_record, update_deployment_status,
    update_user_balance, TransactionType, record_transaction,
    check_user_balance, get_deployment_records_by_status,
    get_user_account,
};

// Transaction constants
const MAX_DEPOSIT_AMOUNT: u64 = 10_000_000_000;  // 100 ICP
const MIN_DEPOSIT_AMOUNT: u64 = 1_000_000;       // 0.01 ICP
const ICP_TRANSFER_FEE: u128 = 10_000;           // 0.0001 ICP
const MAX_WITHDRAWAL_RETRIES: u8 = 3;            // Maximum withdrawal retry attempts
const MIN_CYCLES_BALANCE: u64 = 1_000_000_000;   // Minimum cycles balance (1T)

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

/// Process user deposit
pub async fn process_deposit(user: Principal, amount: u64) -> Result<u64, String> {
    // Security check: Amount must be within reasonable range
    if amount < MIN_DEPOSIT_AMOUNT {
        return Err(format!(
            "Deposit amount cannot be less than {} ICP", 
            MIN_DEPOSIT_AMOUNT as f64 / 100_000_000.0
        ));
    }
    
    if amount > MAX_DEPOSIT_AMOUNT {
        return Err(format!(
            "Deposit amount cannot exceed {} ICP", 
            MAX_DEPOSIT_AMOUNT as f64 / 100_000_000.0
        ));
    }
    
    // Verify caller is not anonymous
    if user == Principal::anonymous() {
        return Err("Anonymous identity cannot make deposits".to_string());
    }
    
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID)
        .map_err(|_| "Invalid ICP ledger ID".to_string())?;
    
    let factory_id = ic_cdk::id();
    
    // Build transfer parameters
    let transfer_args = TransferArgs {
        to: Account {
            owner: factory_id,
            subaccount: None,
        },
        amount: amount as u128,
        fee: Some(ICP_TRANSFER_FEE),
        memo: Some(format!("HunterFi deposit from {}", user).as_bytes().to_vec()),
        created_at_time: Some(time()),
    };
    
    // Call ICRC1 ledger to transfer
    let transfer_result: CallResult<(u128,)> = call(ledger_id, "icrc1_transfer", (transfer_args,)).await;
    
    match transfer_result {
        Ok(_) => {
            // Update user balance
            let new_balance = update_user_balance(user, amount, true)?;
            
            // Record transaction
            let description = format!("Deposit of {:.8} ICP", amount as f64 / 100_000_000.0);
            record_transaction(
                user, 
                amount, 
                TransactionType::Deposit,
                description
            );
            
            ic_cdk::println!("User {} successfully deposited {} e8s", user.to_text(), amount);
            Ok(new_balance)
        },
        Err((code, message)) => {
            ic_cdk::println!("User {} deposit failed: code={:?}, message={}", user.to_text(), code, message);
            Err(format!("Deposit failed: {}", message))
        }
    }
}

/// Process payment from user's balance for deployment
pub fn process_balance_payment(user: Principal, amount: u64, purpose: &str) -> Result<(), String> {
    // Check if amount is valid
    if amount == 0 {
        return Err("Payment amount must be greater than 0".to_string());
    }
    
    // Check if user has sufficient balance
    if !check_user_balance(user, amount)? {
        return Err(format!(
            "Insufficient balance: {:.8} ICP required, but your balance is too low", 
            amount as f64 / 100_000_000.0
        ));
    }
    
    // Deduct from user balance
    update_user_balance(user, amount, false)?;
    
    // Record transaction
    record_transaction(
        user,
        amount,
        TransactionType::DeploymentFee,
        purpose.to_string()
    );
    
    ic_cdk::println!("User {} paid {} e8s for: {}", user.to_text(), amount, purpose);
    Ok(())
}

/// Process refund back to user balance
pub fn process_balance_refund(user: Principal, amount: u64, deployment_id: &str) -> Result<(), String> {
    // Verify refund amount is valid
    if amount == 0 {
        return Err("Refund amount must be greater than 0".to_string());
    }
    
    // Add to user balance
    update_user_balance(user, amount, true)?;
    
    // Record transaction
    let description = format!("Refund for failed deployment (ID: {})", deployment_id);
    record_transaction(
        user,
        amount,
        TransactionType::Refund,
        description
    );
    
    ic_cdk::println!("Successfully processed refund of {} e8s for user {}, deployment ID: {}", amount, user.to_text(), deployment_id);
    Ok(())
}

/// Allow user to withdraw their funds to external wallet
pub async fn user_withdraw_funds(user: Principal, amount: u64) -> Result<u64, String> {
    // Security checks
    if amount == 0 {
        return Err("Withdrawal amount must be greater than 0".to_string());
    }
    
    if amount > MAX_DEPOSIT_AMOUNT {
        return Err(format!(
            "Withdrawal amount cannot exceed {} ICP", 
            MAX_DEPOSIT_AMOUNT as f64 / 100_000_000.0
        ));
    }
    
    if user == Principal::anonymous() {
        return Err("Anonymous identity cannot withdraw funds".to_string());
    }
    
    // Check if user has sufficient balance
    let user_account = get_user_account(user).ok_or_else(|| "User account not found".to_string())?;
    
    if user_account.balance < amount {
        return Err(format!(
            "Insufficient balance: you have {:.8} ICP but requested {:.8} ICP",
            user_account.balance as f64 / 100_000_000.0,
            amount as f64 / 100_000_000.0
        ));
    }
    
    // Verify canister has sufficient cycles
    let canister_balance = ic_cdk::api::canister_balance();
    
    if canister_balance < MIN_CYCLES_BALANCE {
        return Err(format!(
            "System cycles balance too low for safe operation. Please try again later or contact support.",
        ));
    }
    
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID)
        .map_err(|_| "Invalid ICP ledger ID".to_string())?;
    
    let args = TransferArgs {
        to: Account {
            owner: user,
            subaccount: None,
        },
        amount: amount as u128,
        fee: Some(ICP_TRANSFER_FEE),
        memo: Some(b"Withdrawal from HunterFi".to_vec()),
        created_at_time: Some(time()),
    };
    
    // Use retry logic to enhance reliability
    let mut retries = 0;
    let mut last_error = "Unknown error".to_string();
    
    while retries < MAX_WITHDRAWAL_RETRIES {
        let call_result: CallResult<(u128,)> = call(ledger_id, "icrc1_transfer", (args.clone(),)).await;
        
        match call_result {
            Ok(_) => {
                // Update user balance
                let new_balance = update_user_balance(user, amount, false)?;
                
                // Record transaction
                let description = format!("Withdrawal of {:.8} ICP to user wallet", amount as f64 / 100_000_000.0);
                record_transaction(
                    user,
                    amount,
                    TransactionType::Refund, // Using Refund type as it's a transfer out
                    description
                );
                
                ic_cdk::println!("User {} successfully withdrawn {} e8s. New balance: {}", user.to_text(), amount, new_balance);
                return Ok(new_balance);
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
                
                if is_temporary_error && retries < MAX_WITHDRAWAL_RETRIES {
                    // IC environment doesn't have sleep, if this is the last attempt, exit directly
                    if retries == MAX_WITHDRAWAL_RETRIES - 1 {
                        break;
                    }
                    
                    ic_cdk::println!("User withdrawal temporarily failed, retrying immediately ({}/{})", retries, MAX_WITHDRAWAL_RETRIES);
                    continue;
                }
                
                break;
            }
        }
    }
    
    Err(format!("Withdrawal failed after {} attempts: {}", retries, last_error))
}

/// Withdraw funds from system to external account (admin only)
pub async fn withdraw_funds(recipient: Principal, amount: u64) -> Result<(), String> {
    // Security checks
    if amount == 0 {
        return Err("Withdrawal amount must be greater than 0".to_string());
    }
    
    if amount > MAX_DEPOSIT_AMOUNT {
        return Err(format!(
            "Withdrawal amount cannot exceed {} ICP", 
            MAX_DEPOSIT_AMOUNT as f64 / 100_000_000.0
        ));
    }
    
    if recipient == Principal::anonymous() {
        return Err("Cannot withdraw to anonymous identity".to_string());
    }
    
    // Verify canister has sufficient cycles
    let canister_balance = ic_cdk::api::canister_balance();
    
    if canister_balance < MIN_CYCLES_BALANCE {
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
        fee: Some(ICP_TRANSFER_FEE),
        memo: Some(b"Withdrawal from HunterFi Factory".to_vec()),
        created_at_time: Some(time()),
    };
    
    // Use retry logic to enhance reliability
    let mut retries = 0;
    let mut last_error = "Unknown error".to_string();
    
    while retries < MAX_WITHDRAWAL_RETRIES {
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
                
                if is_temporary_error && retries < MAX_WITHDRAWAL_RETRIES {
                    // IC environment doesn't have sleep, if this is the last attempt, exit directly
                    if retries == MAX_WITHDRAWAL_RETRIES - 1 {
                        break;
                    }
                    
                    // Simple retry without waiting - IC environment has no reliable waiting mechanism
                    ic_cdk::println!("Withdrawal temporarily failed, retrying immediately ({}/{})", retries, MAX_WITHDRAWAL_RETRIES);
                    continue;
                }
                
                break;
            }
        }
    }
    
    Err(format!("Withdrawal failed after {} attempts: {}", retries, last_error))
}

/// Process all records in DeploymentFailed status for refunds
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
        match process_balance_refund(record.owner, record.fee_amount, &record.deployment_id) {
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
                ic_cdk::println!("Failed to process refund for deployment {}: {}", record.deployment_id, e);
                errors.push(format!("Refund error (ID: {}): {}", record.deployment_id, e));
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