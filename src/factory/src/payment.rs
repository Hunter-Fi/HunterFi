use candid::{CandidType, Principal};
use ic_cdk::api::call::{call, CallResult};
use ic_cdk::api::time;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;
use ic_cdk_timers::TimerId;
use strategy_common::types::{DeploymentStatus};

use crate::state::{ICP_LEDGER_CANISTER_ID, get_deployment_record, update_deployment_status};

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

/// Process refund to the user
pub async fn process_refund(deployment_id: &str) -> Result<(), String> {
    // Get deployment record
    let record = get_deployment_record(deployment_id)
        .ok_or_else(|| format!("Deployment record not found for ID: {}", deployment_id))?;
    
    // Verify status
    if record.status != DeploymentStatus::DeploymentFailed && record.status != DeploymentStatus::Refunding {
        return Err(format!("Invalid status for refund: {:?}", record.status));
    }
    
    // Update status to refunding
    update_deployment_status(
        deployment_id, 
        DeploymentStatus::Refunding, 
        None, 
        None
    )?;
    
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
        Ok(_) => {
            // Update status to refunded
            update_deployment_status(
                deployment_id, 
                DeploymentStatus::Refunded, 
                None, 
                None
            )?;
            Ok(())
        }
        Err((code, message)) => {
            let error = format!("Refund failed: {}: {}", code as u8, message);
            
            // Keep status as refunding and schedule retry
            schedule_refund_retry(deployment_id.to_string());
            
            Err(error)
        }
    }
}

/// Schedule a refund retry after a delay
fn schedule_refund_retry(deployment_id: String) {
    let deployment_id_clone = deployment_id.clone();
    
    let timer_id = ic_cdk_timers::set_timer(Duration::from_secs(300), move || {
        let deployment_id = deployment_id_clone.clone();
        ic_cdk::spawn(async move {
            match process_refund(&deployment_id).await {
                Ok(_) => {
                    // Refund succeeded, remove timer
                    REFUND_TIMERS.with(|timers| {
                        timers.borrow_mut().remove(&deployment_id);
                    });
                }
                Err(_) => {
                    // Will be retried by the timer
                }
            }
        });
    });
    
    // Store timer ID
    REFUND_TIMERS.with(|timers| {
        timers.borrow_mut().insert(deployment_id, timer_id);
    });
}

/// Cancel a refund retry
pub fn cancel_refund_retry(deployment_id: &str) -> Result<(), String> {
    REFUND_TIMERS.with(|timers| {
        let mut timers = timers.borrow_mut();
        if let Some(timer_id) = timers.remove(deployment_id) {
            ic_cdk_timers::clear_timer(timer_id);
            Ok(())
        } else {
            Err(format!("No refund timer found for deployment ID: {}", deployment_id))
        }
    })
}

/// Schedule automatic refunds for all failed deployments
pub fn schedule_refunds_for_failed_deployments() {
    use crate::state::get_deployment_records_by_status;
    
    // Get all failed deployments that haven't started refunding
    let failed_deployments = get_deployment_records_by_status(DeploymentStatus::DeploymentFailed);
    
    for record in failed_deployments {
        // Only process records that have received payment
        if let Some(last_status) = record.error_message.as_deref() {
            if !last_status.contains("Fee collection failed") {
                let deployment_id = record.deployment_id.clone();
                ic_cdk::spawn(async move {
                    let _ = process_refund(&deployment_id).await;
                });
            }
        }
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