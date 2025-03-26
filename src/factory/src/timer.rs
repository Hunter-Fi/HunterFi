use ic_cdk_timers::TimerId;
use std::cell::RefCell;
use std::time::Duration;
use strategy_common::types::DeploymentStatus;

use crate::state::{
    get_all_deployment_records, get_deployment_records_by_status, 
    RefundStatus, update_deployment_status, update_refund_status
};
use crate::payment::process_refund;

// Constants for timeout durations (in seconds)
const PENDING_PAYMENT_TIMEOUT: u64 = 24 * 60 * 60; // 24 hours
const AUTHORIZATION_TIMEOUT: u64 = 6 * 60 * 60;    // 6 hours
const PAYMENT_RECEIVED_TIMEOUT: u64 = 3 * 60 * 60; // 3 hours
const CANISTER_CREATED_TIMEOUT: u64 = 1 * 60 * 60; // 1 hour
const CODE_INSTALLED_TIMEOUT: u64 = 1 * 60 * 60;   // 1 hour
const INITIALIZED_TIMEOUT: u64 = 30 * 60;          // 30 minutes
const REFUNDING_TIMEOUT: u64 = 30 * 60;            // 30 minutes
const CLEANUP_INTERVAL: u64 = 12 * 60 * 60;        // 12 hours

// Period for processing tasks (in milliseconds)
const PROCESSING_INTERVAL_MS: u64 = 15 * 60 * 1000; // 15 minutes
const REFUND_PROCESSING_INTERVAL_MS: u64 = 5 * 60 * 1000; // 5 minutes

thread_local! {
    // Timer ID for the status processing task
    static STATUS_PROCESSING_TIMER: RefCell<Option<TimerId>> = RefCell::new(None);
    
    // Timer ID for refund processing task
    static REFUND_PROCESSING_TIMER: RefCell<Option<TimerId>> = RefCell::new(None);
    
    // Timer ID for cleanup tasks
    static CLEANUP_TIMER: RefCell<Option<TimerId>> = RefCell::new(None);
}

/// Schedule timers for regular status processing and cleanup
pub fn schedule_timers() {
    schedule_status_processing();
    schedule_refund_processing();
    schedule_cleanup();
}

/// Cancel all scheduled timers
pub fn cancel_timers() {
    STATUS_PROCESSING_TIMER.with(|timer| {
        if let Some(timer_id) = *timer.borrow() {
            ic_cdk_timers::clear_timer(timer_id);
            *timer.borrow_mut() = None;
        }
    });
    
    REFUND_PROCESSING_TIMER.with(|timer| {
        if let Some(timer_id) = *timer.borrow() {
            ic_cdk_timers::clear_timer(timer_id);
            *timer.borrow_mut() = None;
        }
    });
    
    CLEANUP_TIMER.with(|timer| {
        if let Some(timer_id) = *timer.borrow() {
            ic_cdk_timers::clear_timer(timer_id);
            *timer.borrow_mut() = None;
        }
    });
}

/// Schedule the timer for processing deployment statuses
fn schedule_status_processing() {
    // Clear existing timer if any
    STATUS_PROCESSING_TIMER.with(|timer| {
        if let Some(timer_id) = *timer.borrow() {
            ic_cdk_timers::clear_timer(timer_id);
        }
        
        // Schedule new timer
        let new_timer = ic_cdk_timers::set_timer_interval(
            Duration::from_millis(PROCESSING_INTERVAL_MS),
            || {
                ic_cdk::spawn(process_deployment_statuses());
            },
        );
        
        *timer.borrow_mut() = Some(new_timer);
    });
}

/// Schedule the timer for processing refunds separately
fn schedule_refund_processing() {
    // Clear existing timer if any
    REFUND_PROCESSING_TIMER.with(|timer| {
        if let Some(timer_id) = *timer.borrow() {
            ic_cdk_timers::clear_timer(timer_id);
        }
        
        // Schedule new timer
        let new_timer = ic_cdk_timers::set_timer_interval(
            Duration::from_millis(REFUND_PROCESSING_INTERVAL_MS),
            || {
                ic_cdk::spawn(process_refunds());
            },
        );
        
        *timer.borrow_mut() = Some(new_timer);
    });
}

/// Schedule the timer for cleaning up old records
fn schedule_cleanup() {
    // Clear existing timer if any
    CLEANUP_TIMER.with(|timer| {
        if let Some(timer_id) = *timer.borrow() {
            ic_cdk_timers::clear_timer(timer_id);
        }
        
        // Schedule new timer
        let cleanup_interval = Duration::from_secs(CLEANUP_INTERVAL);
        let new_timer = ic_cdk_timers::set_timer_interval(
            cleanup_interval,
            || {
                ic_cdk::spawn(cleanup_old_records());
            },
        );
        
        *timer.borrow_mut() = Some(new_timer);
    });
}

/// Process deployment records based on their status and timeout
pub async fn process_deployment_statuses() {
    // Get all deployment records
    let records = get_all_deployment_records();
    let current_time = ic_cdk::api::time();
    
    for record in records {
        // Skip already completed statuses
        match record.status {
            DeploymentStatus::Deployed | 
            DeploymentStatus::Refunded => continue,
            _ => {}
        }
        
        let elapsed_seconds = (current_time - record.last_updated) / 1_000_000_000;
        
        match record.status {
            // Handle PendingPayment timeout
            DeploymentStatus::PendingPayment if elapsed_seconds > PENDING_PAYMENT_TIMEOUT => {
                // Mark as failed without refund (no payment was made)
                let _ = update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    None,
                    Some("Payment timeout exceeded".to_string())
                );
            },
            
            // Handle AuthorizationConfirmed timeout
            DeploymentStatus::AuthorizationConfirmed if elapsed_seconds > AUTHORIZATION_TIMEOUT => {
                // Mark as failed and don't attempt to process payment
                let _ = update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    None,
                    Some("Authorization timeout exceeded".to_string())
                );
            },
            
            // Handle PaymentReceived but stuck in this state
            DeploymentStatus::PaymentReceived if elapsed_seconds > PAYMENT_RECEIVED_TIMEOUT => {
                // Mark as failed and trigger refund
                let _ = update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    None,
                    Some("Payment received but deployment not started in time".to_string())
                );
                
                // Mark for refund but don't process immediately
                let _ = update_refund_status(
                    &record.deployment_id,
                    RefundStatus::NotStarted
                );
                
                // Refund will be processed by the refund timer
            },
            
            // Handle other intermediate states
            DeploymentStatus::CanisterCreated if elapsed_seconds > CANISTER_CREATED_TIMEOUT => {
                ic_cdk::println!("CanisterCreated timeout exceeded for deployment {}", record.deployment_id);
                // Continue with process instead of failing - this could be handled by deployment process
                // todo install code
            },
            
            DeploymentStatus::CodeInstalled if elapsed_seconds > CODE_INSTALLED_TIMEOUT => {
                ic_cdk::println!("CodeInstalled timeout exceeded for deployment {}", record.deployment_id);
                // Continue with process instead of failing - this could be handled by deployment process
                // todo Initializ
            },
            
            DeploymentStatus::Initialized if elapsed_seconds > INITIALIZED_TIMEOUT => {
                ic_cdk::println!("Initialized timeout exceeded for deployment {}", record.deployment_id);
                // Continue with process instead of failing - this could be handled by deployment process
                //todo Deployed
            },
            
            _ => {}
        }
    }
}

/// Process all refunds (consolidated function for all refund processing)
pub async fn process_refunds() {
    // Process deployments that are marked for refund but not started yet
    process_pending_refunds().await;
    
    // Process deployments that are in the refunding state
    process_refunding_deployments().await;
    
    // Process deployments that failed but haven't been marked for refund
    process_failed_deployments().await;
}

/// Process deployments that are marked for refund but not started yet
async fn process_pending_refunds() {
    // This targets records with NotStarted refund status
    let refunding_records = get_all_deployment_records()
        .into_iter()
        .filter(|record| {
            record.status == DeploymentStatus::DeploymentFailed &&
            record.refund_status.as_ref() == Some(&RefundStatus::NotStarted)
        })
        .collect::<Vec<_>>();
    
    for record in refunding_records {
        // Process at most one refund at a time to avoid overwhelming the system
        let deployment_id = record.deployment_id.clone();
        
        match process_refund(&deployment_id).await {
            Ok(_) => {
                ic_cdk::println!("Successfully processed refund for {}", deployment_id);
            }
            Err(e) => {
                ic_cdk::println!("Failed to process refund for {}: {}", deployment_id, e);
                // Will be retried by the timer
            }
        }
        
        // Only process one per interval to avoid system overload
        break;
    }
}

/// Process all records in DeploymentFailed state to attempt refunds
pub async fn process_failed_deployments() {
    // This targets records that are in failed state but have no refund status yet
    let failed_deployments = get_deployment_records_by_status(DeploymentStatus::DeploymentFailed)
        .into_iter()
        .filter(|record| record.refund_status.is_none())
        .collect::<Vec<_>>();
    
    for record in failed_deployments {
        // Skip records that failed before payment
        if let Some(msg) = &record.error_message {
            if msg.contains("Fee collection failed") || 
               msg.contains("Payment timeout exceeded") ||
               msg.contains("Authorization timeout exceeded") {
                // Mark these as not requiring refund
                let _ = update_refund_status(
                    &record.deployment_id,
                    RefundStatus::Failed { reason: "Payment not collected".to_string() }
                );
                continue;
            }
        }
        
        // Mark for refund but don't process immediately
        let deployment_id = record.deployment_id.clone();
        let _ = update_refund_status(
            &deployment_id,
            RefundStatus::NotStarted
        );
        
        // Process at most one record per interval
        break;
    }
}

/// Process all records in Refunding state to continue refund attempts
pub async fn process_refunding_deployments() {
    let refunding_deployments = get_deployment_records_by_status(DeploymentStatus::Refunding)
        .into_iter()
        .filter(|record| {
            // Only process records that are in progress and haven't exceeded max attempts
            if let Some(RefundStatus::InProgress { attempts }) = &record.refund_status {
                *attempts < crate::state::MAX_REFUND_ATTEMPTS
            } else {
                // Also include records in refunding state without proper refund status
                record.refund_status.is_none()
            }
        })
        .collect::<Vec<_>>();
    
    if !refunding_deployments.is_empty() {
        // Process at most one refund at a time
        let record = &refunding_deployments[0];
        let deployment_id = record.deployment_id.clone();
        
        match process_refund(&deployment_id).await {
            Ok(_) => {
                ic_cdk::println!("Successfully processed refund for {}", deployment_id);
            }
            Err(e) => {
                ic_cdk::println!("Failed to process refund for {}: {}", deployment_id, e);
                // Will be retried by the timer
            }
        }
    }
}

/// Cleanup old completed records (optional, based on system requirements)
async fn cleanup_old_records() {
    // This could archive or delete very old records that are already completed
    // (Deployed or Refunded) and are beyond a certain age
    
    // For now, we're just logging
    let completed_count = get_all_deployment_records()
        .into_iter()
        .filter(|record| 
            record.status == DeploymentStatus::Deployed || 
            record.status == DeploymentStatus::Refunded
        )
        .count();
    
    ic_cdk::println!("Cleanup check: Found {} completed records", completed_count);
}

/// Reset and restart all timers
pub fn reset_timers() {
    cancel_timers();
    schedule_timers();
} 