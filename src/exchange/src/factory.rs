use candid::Principal;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::*;
use crate::types::*;
use crate::traits::*;
use crate::icpswap::ICPSwapConnector;
use crate::kongswap::KongSwapConnector;

/// Exchange factory, used to create exchange connector instances
pub struct ExchangeFactory {
    exchange_configs: HashMap<ExchangeType, ExchangeConfig>,
}

impl ExchangeFactory {
    /// Creates a new instance of the exchange factory
    pub fn new() -> Self {
        let mut factory = Self {
            exchange_configs: HashMap::new(),
        };
        
        // Add default configurations
        factory.add_default_configs();
        
        factory
    }
    
    /// Adds default exchange configurations
    fn add_default_configs(&mut self) {
        // ICPSwap default configuration
        self.exchange_configs.insert(
            ExchangeType::ICPSwap,
            ExchangeConfig {
                exchange_type: ExchangeType::ICPSwap,
                // ICPSwap Factory Canister ID - Please replace with the actual ID
                canister_id: Principal::from_text("xmiu5-jqaaa-aaaag-qbz7q-cai").unwrap_or_else(|_| Principal::anonymous()),
                default_slippage: 0.5, // 0.5%
                max_slippage: 5.0,     // 5%
                timeout_secs: 60,       // 60 seconds timeout
                retry_count: 3,         // Retry up to 3 times
            }
        );
        
        // KongSwap default configuration
        self.exchange_configs.insert(
            ExchangeType::KongSwap,
            ExchangeConfig {
                exchange_type: ExchangeType::KongSwap,
                // KongSwap Factory Canister ID - Please replace with the actual ID
                canister_id: Principal::anonymous(), // Temporarily use anonymous Principal
                default_slippage: 0.5, // 0.5%
                max_slippage: 5.0,     // 5%
                timeout_secs: 60,       // 60 seconds timeout
                retry_count: 3,         // Retry up to 3 times
            }
        );
    }
    
    /// Updates the exchange configuration
    pub fn update_config(&mut self, config: ExchangeConfig) {
        self.exchange_configs.insert(config.exchange_type.clone(), config);
    }
    
    /// Gets the exchange configuration
    pub fn get_config(&self, exchange_type: &ExchangeType) -> ExchangeResult<&ExchangeConfig> {
        self.exchange_configs.get(exchange_type)
            .ok_or_else(|| ExchangeError::InvalidParameters(format!("Unsupported exchange type: {:?}", exchange_type)))
    }
    
    /// Creates an ICPSwap connector
    pub fn create_icpswap(&self) -> ExchangeResult<ICPSwapConnector> {
        let config = self.get_config(&ExchangeType::ICPSwap)?;
        Ok(ICPSwapConnector::new(config.clone()))
    }
    
    /// Creates a KongSwap connector
    pub fn create_kongswap(&self) -> ExchangeResult<KongSwapConnector> {
        let config = self.get_config(&ExchangeType::KongSwap)?;
        Ok(KongSwapConnector::new(config.clone()))
    }
    
    /// Creates the corresponding connector based on the exchange type
    pub fn create_exchange(&self, exchange_type: &ExchangeType) -> ExchangeResult<Box<dyn Trading>> {
        match exchange_type {
            ExchangeType::ICPSwap => {
                let connector = self.create_icpswap()?;
                Ok(Box::new(connector) as Box<dyn Trading>)
            },
            ExchangeType::KongSwap => {
                let connector = self.create_kongswap()?;
                Ok(Box::new(connector) as Box<dyn Trading>)
            },
            _ => Err(ExchangeError::NotImplemented),
        }
    }
    
    /// Creates the corresponding connector based on the trading pair
    pub fn create_exchange_for_pair(&self, pair: &TradingPair) -> ExchangeResult<Box<dyn Trading>> {
        self.create_exchange(&pair.exchange)
    }
    
    /// Gets all supported exchange types
    pub fn get_supported_exchanges(&self) -> Vec<ExchangeType> {
        self.exchange_configs.keys().cloned().collect()
    }
} 