use candid::{CandidType, Deserialize};
use ic_cdk::api::time;
use ic_cdk_timers::TimerId;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

thread_local! {
    // Store active timers by ID
    static TIMERS: RefCell<HashMap<String, TimerId>> = RefCell::new(HashMap::new());
    // Store last execution time for each timer
    static LAST_EXECUTION: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
}

/// Timer configuration
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct TimerConfig {
    pub id: String,
    pub interval_seconds: u64,
    pub enabled: bool,
}

/// Clear an existing timer by ID
pub fn clear_timer(timer_id: &str) {
    TIMERS.with(|timers| {
        if let Some(timer) = timers.borrow_mut().remove(timer_id) {
            ic_cdk_timers::clear_timer(timer);
        }
    });
}

/// Set up a periodic timer with the given interval and callback
pub fn set_timer<F>(config: TimerConfig, callback: F) 
where
    F: FnMut() + 'static,
{
    // Clear any existing timer with the same ID
    clear_timer(&config.id);
    
    if !config.enabled {
        return;
    }
    
    // Update last execution time to now
    let current_time = time();
    LAST_EXECUTION.with(|last_exec| {
        last_exec.borrow_mut().insert(config.id.clone(), current_time);
    });
    
    // Set up the periodic timer
    let interval = Duration::from_secs(config.interval_seconds);
    let callback = Rc::new(RefCell::new(callback));
    let timer_id = ic_cdk_timers::set_timer_interval(interval, move || {
        let mut callback = callback.borrow_mut();
        callback();
    });
    
    // Store the timer ID
    TIMERS.with(|timers| {
        timers.borrow_mut().insert(config.id, timer_id);
    });
}

/// Check if timer should be triggered (for manual execution)
pub fn should_trigger(timer_id: &str, interval_seconds: u64) -> bool {
    LAST_EXECUTION.with(|last_exec| {
        let current_time = time();
        let last_time = *last_exec.borrow().get(timer_id).unwrap_or(&0);
        
        // Calculate if enough time has passed
        if current_time - last_time >= interval_seconds * 1_000_000_000 {
            // Update last execution time
            last_exec.borrow_mut().insert(timer_id.to_string(), current_time);
            true
        } else {
            false
        }
    })
}

/// Update last execution time for a timer
pub fn update_last_execution(timer_id: &str) {
    let current_time = time();
    LAST_EXECUTION.with(|last_exec| {
        last_exec.borrow_mut().insert(timer_id.to_string(), current_time);
    });
}

/// Get time until next execution in seconds
pub fn time_until_next_execution(timer_id: &str, interval_seconds: u64) -> u64 {
    LAST_EXECUTION.with(|last_exec| {
        let current_time = time();
        let last_time = *last_exec.borrow().get(timer_id).unwrap_or(&0);
        let elapsed_ns = current_time - last_time;
        let interval_ns = interval_seconds * 1_000_000_000;
        
        if elapsed_ns >= interval_ns {
            0
        } else {
            (interval_ns - elapsed_ns) / 1_000_000_000
        }
    })
} 