use ic_cdk_timers::TimerId;
use std::cell::RefCell;
use std::time::Duration;
use strategy_common::types::DeploymentStatus;

use crate::state::{
    get_deployment_records_by_status,
    archive_old_deployment_records, should_archive_records
};
use crate::payment::process_balance_refund;

// Constants for cleanup
const CLEANUP_INTERVAL_SECS: u64 = 86400; // 1 day
const RETRY_INTERVAL_SECS: u64 = 3600;    // 1 hour
const MAX_RETRY_COUNT: u32 = 5;           // Maximum number of retries for operations

// Store timer IDs
thread_local! {
    static CLEANUP_TIMER: RefCell<Option<TimerId>> = RefCell::new(None);
    static RETRY_COUNTER: RefCell<u32> = RefCell::new(0);
    static LAST_ERROR: RefCell<Option<String>> = RefCell::new(None);
}

/// Schedule timers for regular status processing and cleanup
pub fn schedule_timers() {
    // Reset retry counter
    RETRY_COUNTER.with(|counter| *counter.borrow_mut() = 0);
    
    // Schedule regular cleanup
    schedule_cleanup_timer();
}

/// Cancel all scheduled timers
pub fn cancel_timers() {
    cancel_cleanup_timer();
}

/// Schedule regular cleanup
fn schedule_cleanup_timer() {
    let timer_id = ic_cdk_timers::set_timer(
        Duration::from_secs(CLEANUP_INTERVAL_SECS),
        || {
            ic_cdk::spawn(async {
                // Archive old records if needed
                if should_archive_records() {
                    match archive_old_deployment_records() {
                        Ok(count) => {
                            if count > 0 {
                                ic_cdk::println!("Archived {} old deployment records", count);
                            }
                            // Reset retry counter on success
                            RETRY_COUNTER.with(|counter| *counter.borrow_mut() = 0);
                            LAST_ERROR.with(|error| *error.borrow_mut() = None);
                        },
                        Err(e) => {
                            ic_cdk::println!("Error archiving old records: {}", e);
                            
                            // Store error for logging
                            LAST_ERROR.with(|error| *error.borrow_mut() = Some(e));
                            
                            // Increment retry counter
                            let should_retry = RETRY_COUNTER.with(|counter| {
                                let mut counter_ref = counter.borrow_mut();
                                *counter_ref += 1;
                                *counter_ref <= MAX_RETRY_COUNT
                            });
                            
                            // If retry limit not reached, schedule retry sooner than normal
                            if should_retry {
                                let retry_timer = ic_cdk_timers::set_timer(
                                    Duration::from_secs(RETRY_INTERVAL_SECS),
                                    || {
                                        ic_cdk::spawn(async {
                                            if should_archive_records() {
                                                let _ = archive_old_deployment_records();
                                            }
                                        });
                                    }
                                );
                            }
                        }
                    }
                }
                
                // Process any failed deployments for refund
                match process_failed_deployments().await {
                    Ok(count) => {
                        if count > 0 {
                            ic_cdk::println!("Processed {} failed deployments for refund", count);
                        }
                    },
                    Err(e) => {
                        ic_cdk::println!("Error processing failed deployments: {}", e);
                    }
                }
                
                // Reschedule cleanup
                schedule_cleanup_timer();
            });
        },
    );
    
    CLEANUP_TIMER.with(|timer| {
        *timer.borrow_mut() = Some(timer_id);
    });
}

/// Cancel cleanup timer
fn cancel_cleanup_timer() {
    CLEANUP_TIMER.with(|timer| {
        if let Some(timer_id) = *timer.borrow() {
            ic_cdk_timers::clear_timer(timer_id);
            *timer.borrow_mut() = None;
        }
    });
}

/// Process all records in DeploymentFailed state to attempt refunds
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
        
        // Process refund by adding back to user's balance
        match process_balance_refund(record.owner, record.fee_amount, &record.deployment_id) {
            Ok(_) => {
                // Update deployment status
                match crate::state::update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::Refunded,
                    None,
                    None
                ) {
                    Ok(_) => {
                        processed_count += 1;
                        ic_cdk::println!("Successfully processed refund for {}", record.deployment_id);
                    },
                    Err(e) => {
                        ic_cdk::println!("Failed to update deployment status for {}: {}", record.deployment_id, e);
                        errors.push(format!("Status update error for {}: {}", record.deployment_id, e));
                    }
                }
            },
            Err(e) => {
                ic_cdk::println!("Failed to process refund for {}: {}", record.deployment_id, e);
                errors.push(format!("Refund error for {}: {}", record.deployment_id, e));
            }
        }
    }
    
    // Return result with error details if any
    if !errors.is_empty() && processed_count == 0 {
        // Only return error if nothing was processed
        return Err(format!("Failed to process any refunds. Errors: {}", errors.join(", ")));
    }
    
    Ok(processed_count)
}

/// Reset and restart all timers
pub fn reset_timers() {
    cancel_timers();
    schedule_timers();
}

/// Get status about the timer system
pub fn get_timer_status() -> String {
    let retry_count = RETRY_COUNTER.with(|counter| *counter.borrow());
    let last_error = LAST_ERROR.with(|error| error.borrow().clone());
    
    format!(
        "Timer status: Retry count: {}/{}, Last error: {}", 
        retry_count, 
        MAX_RETRY_COUNT,
        last_error.unwrap_or_else(|| "None".to_string())
    )
} 