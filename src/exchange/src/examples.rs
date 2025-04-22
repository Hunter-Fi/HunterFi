use candid::Principal;
use ic_cdk;

use crate::error::*;
use crate::types::*;
use crate::factory::ExchangeFactory;
use crate::traits::*;

/// Gets ICP token information.
fn get_icp_token() -> TokenInfo {
    TokenInfo {
        canister_id: Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai").unwrap_or_else(|_| Principal::anonymous()),
        symbol: "ICP".to_string(),
        decimals: 8,
        standard: TokenStandard::ICP,
    }
}

/// Gets ckBTC token information.
fn get_ckbtc_token() -> TokenInfo {
    TokenInfo {
        canister_id: Principal::from_text("mxzaz-hqaaa-aaaar-qaada-cai").unwrap_or_else(|_| Principal::anonymous()),
        symbol: "ckBTC".to_string(),
        decimals: 8,
        standard: TokenStandard::ICRC1,
    }
}

/// Example 1: Get trade quote.
pub async fn example_get_quote() -> ExchangeResult<()> {
    // Create exchange factory
    let factory = ExchangeFactory::new();
    
    // Create ICPSwap connector
    let icpswap = factory.create_icpswap()?;
    
    // Create trading pair information
    let trading_pair = TradingPair {
        base_token: get_icp_token(),
        quote_token: get_ckbtc_token(),
        exchange: ExchangeType::ICPSwap,
    };
    
    // Set trade parameters
    let params = TradeParams {
        pair: trading_pair,
        direction: TradeDirection::Sell,
        amount: 100_000_000, // 1 ICP
        slippage_tolerance: 0.5, // 0.5%
        deadline_secs: None,
    };
    
    // Get quote
    let quote_result = icpswap.get_quote(&params).await?;
    
    // Print quote result
    ic_cdk::println!(
        "Quote Result: {} ICP -> {} ckBTC, Price: {}, Fee: {}",
        quote_result.input_amount as f64 / 100_000_000.0, // Convert to ICP unit
        quote_result.output_amount as f64 / 100_000_000.0, // Convert to ckBTC unit
        quote_result.price,
        quote_result.fee_amount as f64 / 100_000_000.0,
    );
    
    Ok(())
}

/// Example 2: Execute trade.
pub async fn example_execute_trade() -> ExchangeResult<()> {
    // Create exchange factory
    let factory = ExchangeFactory::new();
    
    // Create connector based on exchange type
    let exchange = factory.create_exchange(&ExchangeType::ICPSwap)?;
    
    // Create trading pair information
    let trading_pair = TradingPair {
        base_token: get_icp_token(),
        quote_token: get_ckbtc_token(),
        exchange: ExchangeType::ICPSwap,
    };
    
    // Set trade parameters
    let params = TradeParams {
        pair: trading_pair,
        direction: TradeDirection::Sell,
        amount: 50_000_000, // 0.5 ICP
        slippage_tolerance: 0.5, // 0.5%
        deadline_secs: Some(crate::utils::current_timestamp_secs() + 300), // 5 minutes timeout
    };
    
    // Execute trade
    let trade_result = exchange.execute_trade(&params).await?;
    
    // Print trade result
    ic_cdk::println!(
        "Trade Result: {} ICP -> {} ckBTC, Price: {}, Fee: {}, TxID: {}",
        trade_result.input_amount as f64 / 100_000_000.0,
        trade_result.output_amount as f64 / 100_000_000.0,
        trade_result.price,
        trade_result.fee_amount as f64 / 100_000_000.0,
        trade_result.transaction_id.unwrap_or_default(),
    );
    
    Ok(())
}

/// Example 3: Execute batch trade.
pub async fn example_execute_batch_trade() -> ExchangeResult<()> {
    // Create exchange factory
    let factory = ExchangeFactory::new();
    
    // Create ICPSwap connector
    let exchange = factory.create_icpswap()?;
    
    // Create trading pair information
    let icp_ckbtc_pair = TradingPair {
        base_token: get_icp_token(),
        quote_token: get_ckbtc_token(),
        exchange: ExchangeType::ICPSwap,
    };
    
    // Set two trade parameters
    let trade1 = TradeParams {
        pair: icp_ckbtc_pair.clone(),
        direction: TradeDirection::Sell,
        amount: 30_000_000, // 0.3 ICP
        slippage_tolerance: 0.5, // 0.5%
        deadline_secs: None,
    };
    
    let trade2 = TradeParams {
        pair: icp_ckbtc_pair,
        direction: TradeDirection::Sell,
        amount: 20_000_000, // 0.2 ICP
        slippage_tolerance: 0.5, // 0.5%
        deadline_secs: None,
    };
    
    // Create batch trade parameters
    let batch_params = BatchTradeParams {
        trades: vec![trade1, trade2],
        require_all_success: true,
    };
    
    // Execute batch trade
    let batch_result = exchange.execute_batch_trade(&batch_params).await?;
    
    // Print batch trade result
    ic_cdk::println!(
        "Batch Trade Result: All succeeded: {}, Timestamp: {}",
        batch_result.all_succeeded,
        batch_result.timestamp,
    );
    
    for (i, result) in batch_result.results.iter().enumerate() {
        match result {
            Ok(trade) => {
                ic_cdk::println!(
                    "Trade {}: {} ICP -> {} ckBTC, Price: {}",
                    i + 1,
                    trade.input_amount as f64 / 100_000_000.0,
                    trade.output_amount as f64 / 100_000_000.0,
                    trade.price,
                );
            },
            Err(err) => {
                ic_cdk::println!("Trade {} failed: {}", i + 1, err);
            }
        }
    }
    
    Ok(())
}

/// Example 4: Use different exchanges to perform the same operation.
pub async fn example_cross_exchange() -> ExchangeResult<()> {
    // Create exchange factory
    let factory = ExchangeFactory::new();
    
    // Get supported exchanges
    let supported_exchanges = factory.get_supported_exchanges();
    
    // Iterate through all supported exchanges
    for exchange_type in supported_exchanges {
        match factory.create_exchange(&exchange_type) {
            Ok(exchange) => {
                // Create trading pair information
                let trading_pair = TradingPair {
                    base_token: get_icp_token(),
                    quote_token: get_ckbtc_token(),
                    exchange: exchange_type.clone(),
                };
                
                // Query exchange status
                let status = exchange.get_status().await;
                
                match status {
                    Ok(status) => {
                        ic_cdk::println!(
                            "Exchange {:?} is {}. Last updated: {}",
                            exchange_type,
                            if status.is_available { "available" } else { "unavailable" },
                            status.last_updated,
                        );
                        
                        // If the exchange is available, try to get a quote
                        if status.is_available {
                            // Set trade parameters
                            let params = TradeParams {
                                pair: trading_pair,
                                direction: TradeDirection::Sell,
                                amount: 100_000_000, // 1 ICP
                                slippage_tolerance: 0.5, // 0.5%
                                deadline_secs: None,
                            };
                            
                            match exchange.get_quote(&params).await {
                                Ok(quote) => {
                                    ic_cdk::println!(
                                        "Quote from {:?}: {} ICP -> {} ckBTC, Price: {}",
                                        exchange_type,
                                        quote.input_amount as f64 / 100_000_000.0,
                                        quote.output_amount as f64 / 100_000_000.0,
                                        quote.price,
                                    );
                                },
                                Err(err) => {
                                    ic_cdk::println!("Failed to get quote from {:?}: {:?}", exchange_type, err);
                                }
                            }
                        }
                    },
                    Err(err) => {
                        ic_cdk::println!("Failed to get status for {:?}: {:?}", exchange_type, err);
                    }
                }
            },
            Err(err) => {
                ic_cdk::println!("Failed to create exchange {:?}: {:?}", exchange_type, err);
            }
        }
    }
    
    Ok(())
}

/// Example 5: Best price path finder.
pub async fn example_best_price_finder() -> ExchangeResult<()> {
    // Create exchange factory
    let factory = ExchangeFactory::new();
    
    // Get all supported exchanges
    let supported_exchanges = factory.get_supported_exchanges();
    
    // Set token to query
    let icp_token = get_icp_token();
    let ckbtc_token = get_ckbtc_token();
    
    // Trade amount
    let trade_amount = 100_000_000; // 1 ICP
    
    // Store best quote
    let mut best_quote: Option<(ExchangeType, QuoteResult)> = None;
    
    // Iterate through all exchanges to find best quote
    for exchange_type in supported_exchanges {
        match factory.create_exchange(&exchange_type) {
            Ok(exchange) => {
                // Create trading pair information
                let trading_pair = TradingPair {
                    base_token: icp_token.clone(),
                    quote_token: ckbtc_token.clone(),
                    exchange: exchange_type.clone(),
                };
                
                // Set trade parameters
                let params = TradeParams {
                    pair: trading_pair,
                    direction: TradeDirection::Sell,
                    amount: trade_amount,
                    slippage_tolerance: 0.5, // 0.5%
                    deadline_secs: None,
                };
                
                // Get quote
                match exchange.get_quote(&params).await {
                    Ok(quote) => {
                        // Update best quote
                        match &best_quote {
                            Some((_, best)) if quote.output_amount > best.output_amount => {
                                best_quote = Some((exchange_type, quote));
                            },
                            None => {
                                best_quote = Some((exchange_type, quote));
                            },
                            _ => {}
                        }
                    },
                    Err(_) => {
                        // Ignore error and continue querying next exchange
                        continue;
                    }
                }
            },
            Err(_) => {
                // Ignore error and continue querying next exchange
                continue;
            }
        }
    }
    
    // Output best quote
    match best_quote {
        Some((exchange_type, quote)) => {
            ic_cdk::println!(
                "Best price found on {:?}: {} ICP -> {} ckBTC, Price: {}",
                exchange_type,
                quote.input_amount as f64 / 100_000_000.0,
                quote.output_amount as f64 / 100_000_000.0,
                quote.price,
            );
            
            // Use best quote to execute trade
            let exchange = factory.create_exchange(&exchange_type)?;
            
            let best_params = TradeParams {
                pair: TradingPair {
                    base_token: icp_token,
                    quote_token: ckbtc_token,
                    exchange: exchange_type,
                },
                direction: TradeDirection::Sell,
                amount: trade_amount,
                slippage_tolerance: 0.5,
                deadline_secs: Some(crate::utils::current_timestamp_secs() + 300), // 5 minutes timeout
            };
            
            // Execute trade
            let trade_result = exchange.execute_trade(&best_params).await?;
            
            // Print trade result
            ic_cdk::println!(
                "Trade executed: {} ICP -> {} ckBTC, Price: {}, TxID: {}",
                trade_result.input_amount as f64 / 100_000_000.0,
                trade_result.output_amount as f64 / 100_000_000.0,
                trade_result.price,
                trade_result.transaction_id.unwrap_or_default(),
            );
        },
        None => {
            ic_cdk::println!("No available quote found.");
        }
    }
    
    Ok(())
}