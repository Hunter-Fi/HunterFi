use candid::{CandidType, Principal};
use ic_cdk::api::call::{call, CallResult};
use ic_cdk::api::time;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;
use ic_cdk_timers::TimerId;
use strategy_common::types::{DeploymentStatus};

use crate::state::{
    ICP_LEDGER_CANISTER_ID, MAX_REFUND_ATTEMPTS, RefundStatus,
    get_deployment_record, update_deployment_status,
    update_refund_status, update_refund_tx_id, REFUND_LOCKS
};

thread_local! {
    // Track current refunds in progress
    static REFUND_TIMERS: RefCell<HashMap<String, TimerId>> = RefCell::new(HashMap::new());
}

// ICRC2 Allowance args and response
#[derive(CandidType, Debug)]
struct AllowanceArgs {
    account: Account,
    spender: Account,
}

#[derive(CandidType, Deserialize, Debug)]
struct Allowance {
    allowance: u128,
    expires_at: Option<u64>,
}

// ICRC1/2 Account type
#[derive(CandidType, Deserialize, Debug, Clone)]
struct Account {
    owner: Principal,
    subaccount: Option<Vec<u8>>,
}

// ICRC2 TransferFrom args
#[derive(CandidType, Debug)]
struct TransferFromArgs {
    from: Account,
    to: Account,
    amount: u128,
    fee: Option<u128>,
    memo: Option<Vec<u8>>,
    created_at_time: Option<u64>,
}

// ICRC1 Transfer args
#[derive(CandidType, Debug)]
struct TransferArgs {
    to: Account,
    amount: u128,
    fee: Option<u128>,
    memo: Option<Vec<u8>>,
    created_at_time: Option<u64>,
}

/// Check allowance for the fee
pub async fn check_allowance(owner: Principal, fee: u64) -> Result<bool, String> {
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID)
        .map_err(|_| "Invalid ICP ledger ID".to_string())?;
    
    let factory_id = ic_cdk::id();
    
    let args = AllowanceArgs {
        account: Account {
            owner,
            subaccount: None,
        },
        spender: Account {
            owner: factory_id,
            subaccount: None,
        },
    };
    
    let call_result: CallResult<(Allowance,)> = call(ledger_id, "icrc2_allowance", (args,)).await;
    
    match call_result {
        Ok((allowance,)) => {
            // Ensure the allowance is sufficient
            if allowance.allowance >= fee as u128 {
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Err((code, message)) => {
            Err(format!("Failed to check allowance: {}: {}", code as u8, message))
        }
    }
}

/// Collect the deployment fee using ICRC2 transfer_from
pub async fn collect_fee(deployment_id: &str) -> Result<(), String> {
    // Get deployment record
    let record = get_deployment_record(deployment_id)
        .ok_or_else(|| format!("Deployment record not found for ID: {}", deployment_id))?;
    
    // Verify status is correct
    if record.status != DeploymentStatus::AuthorizationConfirmed {
        return Err(format!(
            "Invalid deployment status: {:?}, expected: {:?}", 
            record.status, 
            DeploymentStatus::AuthorizationConfirmed
        ));
    }
    
    // Update status to processing
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::PaymentReceived, 
        None, 
        None
    )?;
    
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID)
        .map_err(|_| "Invalid ICP ledger ID".to_string())?;
    
    let factory_id = ic_cdk::id();
    
    let args = TransferFromArgs {
        from: Account {
            owner: record.owner,
            subaccount: None,
        },
        to: Account {
            owner: factory_id,
            subaccount: None,
        },
        amount: record.fee_amount as u128,
        fee: Some(10_000), // 0.0001 ICP
        memo: Some(format!("Deployment fee for {}", deployment_id).as_bytes().to_vec()),
        created_at_time: Some(time()),
    };
    
    let call_result: CallResult<(u128,)> = call(ledger_id, "icrc2_transfer_from", (args,)).await;
    
    match call_result {
        Ok(_) => {
            // Update status to payment received
            update_deployment_status(
                deployment_id, 
                DeploymentStatus::PaymentReceived, 
                None, 
                None
            )?;
            Ok(())
        }
        Err((code, message)) => {
            // Update status to failed
            update_deployment_status(
                deployment_id, 
                DeploymentStatus::DeploymentFailed, 
                None, 
                Some(format!("Fee collection failed: {}: {}", code as u8, message))
            )?;
            
            Err(format!("Failed to collect fee: {}: {}", code as u8, message))
        }
    }
}

/// Process refund to the user with improved handling to prevent duplicate refunds
pub async fn process_refund(deployment_id: &str) -> Result<(), String> {
    // Use locking to prevent concurrent processing of the same refund
    REFUND_LOCKS.with(|locks| {
        if locks.borrow().contains(deployment_id) {
            return Err(format!("Refund already being processed for: {}", deployment_id));
        }
        locks.borrow_mut().insert(deployment_id.to_string());
        Ok(())
    })?;
    
    // Ensure lock is released when function completes
    let result = process_refund_internal(deployment_id).await;
    
    REFUND_LOCKS.with(|locks| {
        locks.borrow_mut().remove(deployment_id);
    });
    
    result
}

/// Internal implementation of refund processing
async fn process_refund_internal(deployment_id: &str) -> Result<(), String> {
    // Get deployment record
    let record = get_deployment_record(deployment_id)
        .ok_or_else(|| format!("Deployment record not found for ID: {}", deployment_id))?;
    
    // Verify status - only failed deployments can be refunded
    if record.status != DeploymentStatus::DeploymentFailed && record.status != DeploymentStatus::Refunding {
        return Err(format!("Invalid status for refund: {:?}", record.status));
    }
    
    // Check current refund status
    match &record.refund_status {
        Some(RefundStatus::Completed { timestamp }) => {
            return Err(format!("Refund already completed at timestamp: {}", timestamp));
        }
        Some(RefundStatus::Failed { reason }) => {
            return Err(format!("Refund previously failed permanently: {}", reason));
        }
        Some(RefundStatus::InProgress { attempts }) => {
            // Check if we've exceeded max attempts
            if *attempts >= MAX_REFUND_ATTEMPTS {
                // Mark as permanently failed
                update_refund_status(
                    deployment_id,
                    RefundStatus::Failed { 
                        reason: format!("Exceeded maximum of {} refund attempts", MAX_REFUND_ATTEMPTS) 
                    }
                )?;
                
                return Err(format!("Maximum refund attempts ({}) exceeded", MAX_REFUND_ATTEMPTS));
            }
            
            // Update attempts count
            update_refund_status(
                deployment_id,
                RefundStatus::InProgress { attempts: attempts + 1 }
            )?;
        }
        _ => {
            // First attempt
            update_refund_status(
                deployment_id,
                RefundStatus::InProgress { attempts: 1 }
            )?;
        }
    }
    
    // Update status to refunding if not already
    if record.status != DeploymentStatus::Refunding {
        update_deployment_status(
            deployment_id, 
            DeploymentStatus::Refunding, 
            None, 
            None
        )?;
    }
    
    // Skip actual refund transfer if we're in testing or the fee is zero
    if record.fee_amount == 0 {
        update_refund_status(
            deployment_id,
            RefundStatus::Completed { timestamp: time() }
        )?;
        
        update_deployment_status(
            deployment_id, 
            DeploymentStatus::Refunded, 
            None, 
            None
        )?;
        
        return Ok(());
    }
    
    // Perform the actual refund transfer
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID)
        .map_err(|_| "Invalid ICP ledger ID".to_string())?;
    
    let args = TransferArgs {
        to: Account {
            owner: record.owner,
            subaccount: None,
        },
        amount: record.fee_amount as u128,
        fee: Some(10_000), // 0.0001 ICP
        memo: Some(format!("Refund for failed deployment {}", deployment_id).as_bytes().to_vec()),
        created_at_time: Some(time()),
    };
    
    let call_result: CallResult<(u128,)> = call(ledger_id, "icrc1_transfer", (args,)).await;
    
    match call_result {
        Ok((tx_id,)) => {
            // Update status to refunded
            update_deployment_status(
                deployment_id, 
                DeploymentStatus::Refunded, 
                None, 
                None
            )?;
            
            // Update refund status and transaction ID
            update_refund_status(
                deployment_id,
                RefundStatus::Completed { timestamp: time() }
            )?;
            
            update_refund_tx_id(deployment_id, Some(tx_id))?;
            
            // Clear any scheduled retries
            cancel_refund_retry(deployment_id)?;
            
            Ok(())
        }
        Err((code, message)) => {
            let error = format!("Refund failed: {}: {}", code as u8, message);
            
            // Get current attempts
            let attempts = match record.refund_status {
                Some(RefundStatus::InProgress { attempts }) => attempts,
                _ => 1,
            };
            
            if attempts < MAX_REFUND_ATTEMPTS {
                // Schedule retry with exponential backoff
                schedule_refund_retry(deployment_id.to_string(), attempts);
                Err(error)
            } else {
                // Mark as permanently failed
                update_refund_status(
                    deployment_id,
                    RefundStatus::Failed { reason: error.clone() }
                )?;
                
                Err(error)
            }
        }
    }
}

/// Schedule a refund retry with exponential backoff
fn schedule_refund_retry(deployment_id: String, attempts: u8) {
    // Cancel any existing retry timer
    REFUND_TIMERS.with(|timers| {
        if let Some(timer_id) = timers.borrow().get(&deployment_id) {
            ic_cdk_timers::clear_timer(*timer_id);
        }
    });
    
    // Calculate delay with exponential backoff
    let base_delay_secs = 60; // 1 minute base delay
    let max_delay_secs = 3600; // Max 1 hour
    
    // Calculate delay: min(base_delay * 2^attempts, max_delay)
    let backoff_factor = 1u64 << (attempts as u32); // 2^attempts
    let delay_secs = std::cmp::min(
        base_delay_secs * backoff_factor,
        max_delay_secs
    );
    
    let deployment_id_clone = deployment_id.clone();
    
    let timer_id = ic_cdk_timers::set_timer(Duration::from_secs(delay_secs), move || {
        let deployment_id = deployment_id_clone.clone();
        ic_cdk::spawn(async move {
            match process_refund(&deployment_id).await {
                Ok(_) => {
                    // Refund succeeded, remove timer
                    REFUND_TIMERS.with(|timers| {
                        timers.borrow_mut().remove(&deployment_id);
                    });
                }
                Err(e) => {
                    ic_cdk::println!("Scheduled refund retry failed for {}: {}", deployment_id, e);
                    // Will be retried if attempts < max
                }
            }
        });
    });
    
    // Store timer ID
    REFUND_TIMERS.with(|timers| {
        timers.borrow_mut().insert(deployment_id.clone(), timer_id);
    });
    
    ic_cdk::println!("Scheduled refund retry for {} in {} seconds (attempt {})", 
                     deployment_id, delay_secs, attempts + 1);
}

/// Cancel a refund retry
pub fn cancel_refund_retry(deployment_id: &str) -> Result<(), String> {
    REFUND_TIMERS.with(|timers| {
        let mut timers = timers.borrow_mut();
        if let Some(timer_id) = timers.remove(deployment_id) {
            ic_cdk_timers::clear_timer(timer_id);
            Ok(())
        } else {
            // Not an error if no timer exists
            Ok(())
        }
    })
}

/// Schedule automatic refunds for all failed deployments
pub fn schedule_refunds_for_failed_deployments() {
    use crate::state::get_deployment_records_by_status;
    
    // Get all failed deployments that haven't started refunding
    let failed_deployments = get_deployment_records_by_status(DeploymentStatus::DeploymentFailed);
    
    for record in failed_deployments {
        // Skip refunding if this failure occurred before payment
        if let Some(error_msg) = &record.error_message {
            if error_msg.contains("Fee collection failed") ||
               error_msg.contains("Payment timeout exceeded") ||
               error_msg.contains("Authorization timeout exceeded") {
                continue;
            }
        }
        
        // Skip if already refunded or permanently failed
        match &record.refund_status {
            Some(RefundStatus::Completed { .. }) => continue,
            Some(RefundStatus::Failed { .. }) => continue,
            _ => {}
        }
        
        // Process one at a time to avoid overwhelming the system
        let deployment_id = record.deployment_id.clone();
        ic_cdk::spawn(async move {
            let _ = process_refund(&deployment_id).await;
        });
    }
}

/// Withdraw funds from the canister to an account
pub async fn withdraw_funds(recipient: Principal, amount: u64) -> Result<(), String> {
    let ledger_id = Principal::from_text(ICP_LEDGER_CANISTER_ID)
        .map_err(|_| "Invalid ICP ledger ID".to_string())?;
    
    let args = TransferArgs {
        to: Account {
            owner: recipient,
            subaccount: None,
        },
        amount: amount as u128,
        fee: Some(10_000), // 0.0001 ICP
        memo: Some(b"Withdrawal from factory canister".to_vec()),
        created_at_time: Some(time()),
    };
    
    let call_result: CallResult<(u128,)> = call(ledger_id, "icrc1_transfer", (args,)).await;
    
    match call_result {
        Ok(_) => Ok(()),
        Err((code, message)) => {
            Err(format!("Withdrawal failed: {}: {}", code as u8, message))
        }
    }
} 