use ic_cdk_timers::TimerId;
use std::cell::RefCell;
use std::time::Duration;
use strategy_common::types::DeploymentStatus;

use crate::state::{get_all_deployment_records, get_deployment_records_by_status};
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

thread_local! {
    // Timer ID for the status processing task
    static STATUS_PROCESSING_TIMER: RefCell<Option<TimerId>> = RefCell::new(None);
    
    // Timer ID for cleanup tasks
    static CLEANUP_TIMER: RefCell<Option<TimerId>> = RefCell::new(None);
}

/// Schedule timers for regular status processing and cleanup
pub fn schedule_timers() {
    schedule_status_processing();
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
                let _ = crate::state::update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    None,
                    Some("Payment timeout exceeded".to_string())
                );
            },
            
            // Handle AuthorizationConfirmed timeout
            DeploymentStatus::AuthorizationConfirmed if elapsed_seconds > AUTHORIZATION_TIMEOUT => {
                // Mark as failed and don't attempt to process payment
                let _ = crate::state::update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    None,
                    Some("Authorization timeout exceeded".to_string())
                );
            },
            
            // Handle PaymentReceived but stuck in this state
            DeploymentStatus::PaymentReceived if elapsed_seconds > PAYMENT_RECEIVED_TIMEOUT => {
                // Mark as failed and trigger refund
                let _ = crate::state::update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    None,
                    Some("Payment received but deployment not started in time".to_string())
                );
                
                // Process refund
                let _ = process_refund(&record.deployment_id).await;
            },
            
            // Handle CanisterCreated timeout
            DeploymentStatus::CanisterCreated if elapsed_seconds > CANISTER_CREATED_TIMEOUT => {
                // Mark as failed and trigger refund
                let _ = crate::state::update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    record.canister_id,
                    Some("Canister created but code not installed in time".to_string())
                );
                
                // Process refund
                let _ = process_refund(&record.deployment_id).await;
            },
            
            // Handle CodeInstalled timeout
            DeploymentStatus::CodeInstalled if elapsed_seconds > CODE_INSTALLED_TIMEOUT => {
                // Mark as failed and trigger refund
                let _ = crate::state::update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    record.canister_id,
                    Some("Code installed but not initialized in time".to_string())
                );
                
                // Process refund
                let _ = process_refund(&record.deployment_id).await;
            },
            
            // Handle Initialized timeout
            DeploymentStatus::Initialized if elapsed_seconds > INITIALIZED_TIMEOUT => {
                // Mark as failed and trigger refund
                let _ = crate::state::update_deployment_status(
                    &record.deployment_id,
                    DeploymentStatus::DeploymentFailed,
                    record.canister_id,
                    Some("Initialized but not deployed in time".to_string())
                );
                
                // Process refund
                let _ = process_refund(&record.deployment_id).await;
            },
            
            // Retry refunds for DeploymentFailed records
            DeploymentStatus::DeploymentFailed => {
                // Process refund
                let _ = process_refund(&record.deployment_id).await;
            },
            
            // Retry Refunding if stuck
            DeploymentStatus::Refunding if elapsed_seconds > REFUNDING_TIMEOUT => {
                // Retry refund
                let _ = process_refund(&record.deployment_id).await;
            },
            
            _ => {}
        }
    }
}

/// Process all records in DeploymentFailed state to attempt refunds
pub async fn process_failed_deployments() {
    let failed_deployments = get_deployment_records_by_status(DeploymentStatus::DeploymentFailed);
    
    for record in failed_deployments {
        // Skip records that failed before payment
        if let Some(msg) = &record.error_message {
            if msg.contains("Fee collection failed") || 
               msg.contains("Payment timeout exceeded") ||
               msg.contains("Authorization timeout exceeded") {
                continue;
            }
        }
        
        // Process refund
        let _ = process_refund(&record.deployment_id).await;
    }
}

/// Process all records in Refunding state to continue refund attempts
pub async fn process_refunding_deployments() {
    let refunding_deployments = get_deployment_records_by_status(DeploymentStatus::Refunding);
    
    for record in refunding_deployments {
        // Process refund
        let _ = process_refund(&record.deployment_id).await;
    }
}

/// Cleanup old completed records (optional, based on system requirements)
async fn cleanup_old_records() {
    // This could archive or delete very old records that are already completed
    // (Deployed or Refunded) and are beyond a certain age
}

/// Reset and restart all timers
pub fn reset_timers() {
    cancel_timers();
    schedule_timers();
} 