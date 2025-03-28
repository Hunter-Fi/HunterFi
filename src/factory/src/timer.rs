use ic_cdk_timers::TimerId;
use std::cell::RefCell;
use std::time::Duration;
use strategy_common::DeploymentStatus;

use crate::state::{
    get_deployment_records_by_status,
    archive_old_deployment_records, should_archive_records,
    RETENTION_PERIOD_NS, COMPLETED_RECORD_RETENTION_DAYS,
    MAX_COMPLETED_RECORDS, ARCHIVING_THRESHOLD_PERCENT,
    get_all_deployment_records, process_balance_refund
};

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

/// A task that can be retried on failure
struct RetryableTask<F, G>
where
    F: Fn() -> Result<usize, String> + Clone + 'static,
    G: Fn() + Clone + 'static,
{
    task: F,
    on_success: G,
    max_retries: u32,
    retry_interval: Duration,
}

impl<F, G> RetryableTask<F, G>
where
    F: Fn() -> Result<usize, String> + Clone + 'static,
    G: Fn() + Clone + 'static,
{
    pub fn new(task: F, on_success: G) -> Self {
        RetryableTask {
            task,
            on_success,
            max_retries: MAX_RETRY_COUNT,
            retry_interval: Duration::from_secs(RETRY_INTERVAL_SECS),
        }
    }
    
    pub fn execute(self) {
        // Create owned values for the closure
        let task = self.task;
        let on_success = self.on_success;
        let max_retries = self.max_retries;
        let retry_interval = self.retry_interval;
        
        // Start with retry count at 0
        RETRY_COUNTER.with(|counter| *counter.borrow_mut() = 0);
        
        ic_cdk::spawn(async move {
            execute_with_retry(task, on_success, max_retries, retry_interval, 0).await;
        });
    }
}

/// Separate function to handle the task execution and retries
async fn execute_with_retry<F, G>(
    task: F,
    on_success: G,
    max_retries: u32,
    retry_interval: Duration,
    current_retry: u32
)
where
    F: Fn() -> Result<usize, String> + Clone + 'static,
    G: Fn() + Clone + 'static
{
    match task() {
        Ok(count) => {
            if count > 0 {
                ic_cdk::println!("Task successfully processed {} items", count);
            }
            // Reset retry counter and clear error
            RETRY_COUNTER.with(|counter| *counter.borrow_mut() = 0);
            LAST_ERROR.with(|error| *error.borrow_mut() = None);
            
            // Call success callback
            on_success();
        },
        Err(e) => {
            ic_cdk::println!("Task execution error: {}", e);
            
            // Store error for logging
            LAST_ERROR.with(|error| *error.borrow_mut() = Some(e.clone()));
            
            // Increment retry counter
            let next_retry = current_retry + 1;
            RETRY_COUNTER.with(|counter| *counter.borrow_mut() = next_retry);
            
            // If retry limit not reached, schedule retry sooner than normal
            if next_retry <= max_retries {
                let task_clone = task.clone();
                let on_success_clone = on_success.clone();
                
                ic_cdk_timers::set_timer(retry_interval, move || {
                    ic_cdk::spawn(async move {
                        execute_with_retry(
                            task_clone, 
                            on_success_clone, 
                            max_retries, 
                            retry_interval, 
                            next_retry
                        ).await;
                    });
                });
            }
        }
    }
}

// Combined task handler for regular operations
struct TimerTaskManager {
    max_retries: u32,
    retry_interval: Duration,
    cleanup_interval: Duration,
}

impl TimerTaskManager {
    fn new() -> Self {
        Self {
            max_retries: MAX_RETRY_COUNT,
            retry_interval: Duration::from_secs(RETRY_INTERVAL_SECS),
            cleanup_interval: Duration::from_secs(CLEANUP_INTERVAL_SECS),
        }
    }
    
    /// Execute a task with retry capability
    async fn execute_with_retry<F>(&self, task: F, task_name: &str) -> Result<usize, String>
    where
        F: Fn() -> Result<usize, String> + Clone + 'static
    {
        let mut retry_count = 0;
        let task_name = task_name.to_string();
        
        loop {
            match task() {
                Ok(count) => {
                    if count > 0 {
                        ic_cdk::println!("Task '{}' successfully processed {} items", task_name, count);
                    }
                    
                    // Update retry counter and error status
                    RETRY_COUNTER.with(|counter| *counter.borrow_mut() = 0);
                    LAST_ERROR.with(|error| *error.borrow_mut() = None);
                    
                    return Ok(count);
                },
                Err(e) => {
                    ic_cdk::println!("Task '{}' execution error: {}", task_name, e);
                    LAST_ERROR.with(|error| *error.borrow_mut() = Some(e.clone()));
                    
                    retry_count += 1;
                    if retry_count <= self.max_retries {
                        // Wait before retry
                        ic_cdk::println!("Retrying task '{}' (attempt {}/{})", 
                            task_name, retry_count, self.max_retries);
                        
                        // Sleep for the retry interval
                        let sleep_ns = self.retry_interval.as_nanos() as u64;
                        ic_cdk::println!("Sleeping for {} seconds before retry", sleep_ns / 1_000_000_000);
                        
                        // Update retry counter
                        RETRY_COUNTER.with(|counter| *counter.borrow_mut() = retry_count);
                        
                        // In IC environment, we can't sleep in the main thread
                        // Instead, let's continue with the next iteration in the future
                        // Use a timer for delay instead
                        return Err(format!("Retry needed for task {}", task_name));
                    } else {
                        ic_cdk::println!("Task '{}' failed after {} retries", task_name, retry_count - 1);
                        return Err(format!("Task failed after {} retries: {}", self.max_retries, e));
                    }
                }
            }
        }
    }
    
    /// Schedule regular cleanup tasks
    fn schedule_cleanup(&self) {
        ic_cdk::println!("Scheduling cleanup timer (interval: {} seconds)", self.cleanup_interval.as_secs());
        
        let timer_id = ic_cdk_timers::set_timer(
            self.cleanup_interval,
            || {
                ic_cdk::spawn(async {
                    // Execute cleanup tasks
                    let _ = execute_cleanup_tasks().await;
                    
                    // Reschedule for next time
                    let manager = TimerTaskManager::new();
                    manager.schedule_cleanup();
                });
            },
        );
        
        CLEANUP_TIMER.with(|timer| {
            *timer.borrow_mut() = Some(timer_id);
        });
    }
}

/// Execute all cleanup tasks in sequence
async fn execute_cleanup_tasks() -> Result<(), String> {
    let manager = TimerTaskManager::new();
    
    // 1. Archive old records if needed
    if should_archive_records() {
        if let Err(e) = manager.execute_with_retry(
            || archive_old_deployment_records(),
            "record_archiving"
        ).await {
            ic_cdk::println!("Record archiving failed: {}", e);
        }
    }
    
    // 2. Process failed deployments
    if let Err(e) = process_failed_deployments().await {
        ic_cdk::println!("Failed deployment processing error: {}", e);
    }
    
    Ok(())
}

/// Schedule timers for regular status processing and cleanup
pub fn schedule_timers() {
    // Reset retry counter
    RETRY_COUNTER.with(|counter| *counter.borrow_mut() = 0);
    
    // Create task manager and schedule cleanup
    let manager = TimerTaskManager::new();
    manager.schedule_cleanup();
}

/// Cancel all scheduled timers
pub fn cancel_timers() {
    // Safely cancel the timer to avoid BorrowMutError
    let timer_id_opt = CLEANUP_TIMER.with(|timer| {
        let timer_id = *timer.borrow();
        if timer_id.is_some() {
            *timer.borrow_mut() = None;
        }
        timer_id
    });
    
    // If timer ID exists, clear it
    if let Some(timer_id) = timer_id_opt {
        ic_cdk_timers::clear_timer(timer_id);
        ic_cdk::println!("Canceled cleanup timer");
    }
}

/// Process all records in DeploymentFailed state to attempt refunds
pub async fn process_failed_deployments() -> Result<usize, String> {
    let failed_records = get_deployment_records_by_status(DeploymentStatus::DeploymentFailed);
    let mut processed_count = 0;
    
    for record in failed_records {
        // Only process records that haven't been refunded already
        if record.status != DeploymentStatus::Refunded {
            // Process refund
            if let Err(e) = process_balance_refund(
                record.owner, 
                record.fee_amount, 
                &record.deployment_id
            ).await {
                ic_cdk::println!("Failed to process refund for deployment {}: {}", 
                    record.deployment_id, e);
                continue;
            }
            
            processed_count += 1;
        }
    }
    
    Ok(processed_count)
}

/// Reset timers - Cancel current and schedule new
pub fn reset_timers() {
    cancel_timers();
    schedule_timers();
}

/// Get current timer status
pub fn get_timer_status() -> String {
    let retry_count = RETRY_COUNTER.with(|counter| *counter.borrow());
    let last_error = LAST_ERROR.with(|error| error.borrow().clone());
    let timer_active = CLEANUP_TIMER.with(|timer| timer.borrow().is_some());
    
    format!(
        "Timer status: active={}, retry_count={}, last_error={}", 
        timer_active,
        retry_count,
        last_error.unwrap_or_else(|| "none".to_string())
    )
}

/// Archive old records - run once per day
async fn archive_old_records() -> Result<(), String> {
    if should_archive_records() {
        match archive_old_deployment_records() {
            Ok(count) => {
                if count > 0 {
                    ic_cdk::println!("Successfully archived {} old records", count);
                }
                Ok(())
            },
            Err(e) => Err(format!("Failed to archive old records: {}", e))
        }
    } else {
        // Nothing to archive
        Ok(())
    }
}

/// Restart the cleanup timer
fn restart_cleanup_timer() {
    let manager = TimerTaskManager::new();
    manager.schedule_cleanup();
} 