use async_trait::async_trait;
use candid::{CandidType, Deserialize, Principal, Nat, Int};
use serde::Serialize;
use std::collections::HashMap;
use ic_cdk::api::call::{CallResult, RejectionCode};
use std::convert::TryFrom;
use ic_ledger_types::{AccountIdentifier, AccountBalanceArgs, Tokens, DEFAULT_SUBACCOUNT};

use crate::error::*;
use crate::types::*;
use crate::traits::*;
use crate::utils;

/// Connector for the ICPSwap exchange
pub struct ICPSwapConnector {
    config: ExchangeConfig,
    factory_canister_id: Principal,  // ICPSwap Factory Canister ID
}

/// ICPSwap specific Token type
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ICPSwapToken {
    pub address: String,
    pub standard: String,
}

/// ICPSwap arguments for getting pool information
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ICPSwapGetPoolArgs {
    pub fee: Nat,
    pub token0: ICPSwapToken,
    pub token1: ICPSwapToken,
}

/// ICPSwap pool information structure
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ICPSwapPoolData {
    pub fee: Nat,
    pub key: String,
    pub tickSpacing: Int,
    pub token0: ICPSwapToken,
    pub token1: ICPSwapToken,
    pub canisterId: Principal,
}

/// ICPSwap pool information result type
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ICPSwapPoolResult {
    ok(ICPSwapPoolData),
    err(ICPSwapError),
}

/// ICPSwap error types
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ICPSwapError {
    CommonError,
    InternalError(String),
    UnsupportedToken(String),
    InsufficientFunds,
}

/// ICPSwap quote arguments
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ICPSwapQuoteArgs {
    pub zeroForOne: bool,
    pub amountIn: String, // Using String as ICPSwap expects it
    pub amountOutMinimum: String, // Using String as ICPSwap expects it
}

/// ICPSwap quote result type
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ICPSwapQuoteResult {
    ok(Nat), // Changed from String to Nat based on documentation
    err(ICPSwapError),
}

/// ICPSwap deposit arguments
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ICPSwapDepositArgs {
    pub fee: u64,
    pub token: String, // Token address as String
    pub amount: String, // Amount as String
}

/// Generic ICPSwap result type
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ICPSwapResult {
    ok(String), // Result often returns amount as String
    err(ICPSwapError),
}

/// ICPSwap swap arguments
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ICPSwapSwapArgs {
    pub zeroForOne: bool,
    pub amountIn: String, // Using String as ICPSwap expects it
    pub amountOutMinimum: String, // Using String as ICPSwap expects it
}

/// ICPSwap balance result structure
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ICPSwapBalance {
    pub balance0: String, // Balances as String
    pub balance1: String,
}

/// ICPSwap balance query result type
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ICPSwapBalanceResult {
    ok(ICPSwapBalance),
    err(ICPSwapError),
}

/// ICPSwap withdraw arguments
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ICPSwapWithdrawArgs {
    pub fee: u64,
    pub token: String, // Token address as String
    pub amount: String, // Amount as String
}

impl ICPSwapConnector {
    /// Creates a new instance of the ICPSwap connector
    pub fn new(config: ExchangeConfig) -> Self {
        Self {
            factory_canister_id: config.canister_id.clone(),
            config,
        }
    }

    /// Converts internal TokenInfo to ICPSwapToken representation
    fn token_to_icpswap_token(&self, token: &TokenInfo) -> ICPSwapToken {
        ICPSwapToken {
            address: token.canister_id.to_string(),
            standard: match token.standard {
                TokenStandard::ICRC1 => "ICRC1".to_string(),
                TokenStandard::ICRC2 => "ICRC2".to_string(),
                TokenStandard::DIP20 => "DIP20".to_string(),
                TokenStandard::EXT => "EXT".to_string(),
                TokenStandard::ICP => "ICP".to_string(), // Assuming ICP has a specific representation if needed
            },
        }
    }

    /// Checks token order and returns the correct zeroForOne value
    fn is_zero_for_one(&self, base: &TokenInfo, quote: &TokenInfo) -> bool {
        // ICPSwap requires token0's canister_id to be lexicographically smaller than token1's
        base.canister_id.to_string() < quote.canister_id.to_string()
    }

    /// Queries for SwapPool information from the ICPSwap factory
    async fn get_pool_canister(&self, base: &TokenInfo, quote: &TokenInfo) -> ExchangeResult<ICPSwapPoolData> {
        // Sort token0 and token1 lexicographically by canister ID string
        let (token0, token1) = if self.is_zero_for_one(base, quote) {
            (base, quote)
        } else {
            (quote, base)
        };

        let args = ICPSwapGetPoolArgs {
            fee: Nat::from(3000_u64),
            token0: self.token_to_icpswap_token(token0),
            token1: self.token_to_icpswap_token(token1),
        };

        ic_cdk::println!("Calling getPool with args: {:?}", args); // Debug log
        // Use 'call' for both query and update. IC determines mode based on target method.
        let result: CallResult<(ICPSwapPoolResult,)> = ic_cdk::api::call::call(
            self.factory_canister_id,
            "getPool",
            (args,),
        ).await;
        ic_cdk::println!("getPool call result: {:?}", result); // Debug log

        match result {
            Ok((pool_result,)) => match pool_result {
                ICPSwapPoolResult::ok(pool_data) => Ok(pool_data),
                ICPSwapPoolResult::err(err) => {
                    ic_cdk::println!("getPool call returned error: {:?}", err); // Debug log
                    Err(self.map_icpswap_error(err))
                },
            },
            Err((code, msg)) => {
                ic_cdk::println!("getPool call failed: {:?} - {}", code, msg); // Debug log
                Err(ExchangeError::CanisterCallError(format!("Failed to call getPool: {:?} - {}", code, msg)))
            },
        }
    }

    /// Maps ICPSwapError to ExchangeError
    fn map_icpswap_error(&self, err: ICPSwapError) -> ExchangeError {
        match err {
            ICPSwapError::CommonError => ExchangeError::Unknown("ICPSwap common error".to_string()),
            ICPSwapError::InternalError(msg) => ExchangeError::InternalError(format!("ICPSwap internal error: {}", msg)),
            ICPSwapError::UnsupportedToken(token) => ExchangeError::UnsupportedToken(format!("ICPSwap unsupported token: {}", token)),
            ICPSwapError::InsufficientFunds => ExchangeError::InsufficientFunds,
        }
    }

    /// Calls the quote method on the ICPSwap pool canister
    async fn call_quote(&self, pool_id: &Principal, args: ICPSwapQuoteArgs) -> ExchangeResult<Nat> { // Return Nat
        ic_cdk::println!("Calling quote on pool {} with args: {:?}", pool_id, args); // Debug log
        // Use 'call' for both query and update. IC determines mode based on target method.
        let result: CallResult<(ICPSwapQuoteResult,)> = ic_cdk::api::call::call(
            *pool_id,
            "quote",
            (args,),
        ).await;
         ic_cdk::println!("quote call result: {:?}", result); // Debug log

        match result {
            Ok((quote_result,)) => match quote_result {
                ICPSwapQuoteResult::ok(amount_nat) => Ok(amount_nat), // Return Nat directly
                ICPSwapQuoteResult::err(err) => {
                     ic_cdk::println!("quote call returned error: {:?}", err); // Debug log
                     Err(self.map_icpswap_error(err))
                },
            },
            Err((code, msg)) => {
                 ic_cdk::println!("quote call failed: {:?} - {}", code, msg); // Debug log
                 Err(ExchangeError::CanisterCallError(format!("Failed to call quote: {:?} - {}", code, msg)))
            },
        }
    }

    /// Calls the swap method on the ICPSwap pool canister
    async fn call_swap(&self, pool_id: &Principal, args: ICPSwapSwapArgs) -> ExchangeResult<String> {
        ic_cdk::println!("Calling swap on pool {} with args: {:?}", pool_id, args); // Debug log
        let result: CallResult<(ICPSwapResult,)> = ic_cdk::api::call::call(
            *pool_id,
            "swap",
            (args,),
        ).await;
        ic_cdk::println!("swap result: {:?}", result); // Debug log

        match result {
            Ok((swap_result,)) => match swap_result {
                ICPSwapResult::ok(amount_str) => Ok(amount_str),
                ICPSwapResult::err(err) => {
                    ic_cdk::println!("swap returned error: {:?}", err); // Debug log
                    Err(self.map_icpswap_error(err))
                },
            },
            Err((code, msg)) => {
                 ic_cdk::println!("swap call failed: {:?} - {}", code, msg); // Debug log
                 Err(ExchangeError::CanisterCallError(format!("Failed to call swap: {:?} - {}", code, msg)))
            },
        }
    }

    /// Calls the deposit method on the ICPSwap pool canister
    async fn call_deposit(&self, pool_id: &Principal, args: ICPSwapDepositArgs) -> ExchangeResult<String> {
        ic_cdk::println!("Calling deposit on pool {} with args: {:?}", pool_id, args); // Debug log
        let result: CallResult<(ICPSwapResult,)> = ic_cdk::api::call::call(
            *pool_id,
            "deposit",
            (args,),
        ).await;
        ic_cdk::println!("deposit result: {:?}", result); // Debug log

        match result {
            Ok((deposit_result,)) => match deposit_result {
                ICPSwapResult::ok(amount_str) => Ok(amount_str),
                ICPSwapResult::err(err) => {
                    ic_cdk::println!("deposit returned error: {:?}", err); // Debug log
                    Err(self.map_icpswap_error(err))
                },
            },
            Err((code, msg)) => {
                ic_cdk::println!("deposit call failed: {:?} - {}", code, msg); // Debug log
                Err(ExchangeError::CanisterCallError(format!("Failed to call deposit: {:?} - {}", code, msg)))
            },
        }
    }

    /// Calls the withdraw method on the ICPSwap pool canister
    async fn call_withdraw(&self, pool_id: &Principal, args: ICPSwapWithdrawArgs) -> ExchangeResult<String> {
        ic_cdk::println!("Calling withdraw on pool {} with args: {:?}", pool_id, args); // Debug log
        let result: CallResult<(ICPSwapResult,)> = ic_cdk::api::call::call(
            *pool_id,
            "withdraw",
            (args,),
        ).await;
        ic_cdk::println!("withdraw result: {:?}", result); // Debug log

        match result {
            Ok((withdraw_result,)) => match withdraw_result {
                ICPSwapResult::ok(amount_str) => Ok(amount_str),
                ICPSwapResult::err(err) => {
                     ic_cdk::println!("withdraw returned error: {:?}", err); // Debug log
                    Err(self.map_icpswap_error(err))
                },
            },
            Err((code, msg)) => {
                 ic_cdk::println!("withdraw call failed: {:?} - {}", code, msg); // Debug log
                Err(ExchangeError::CanisterCallError(format!("Failed to call withdraw: {:?} - {}", code, msg)))
            },
        }
    }
    
    /// Query user unused balance
    async fn call_get_user_unused_balance(&self, pool_id: &Principal, user: &Principal) -> ExchangeResult<(u128, u128)> {
        let result: CallResult<(ICPSwapBalanceResult,)> = ic_cdk::api::call::call(
            *pool_id,
            "getUserUnusedBalance",
            (user,),
        ).await;
        
        match result {
            Ok((result,)) => match result {
                ICPSwapBalanceResult::ok(balance) => Ok((balance.balance0.parse::<u128>().unwrap(), balance.balance1.parse::<u128>().unwrap())),
                ICPSwapBalanceResult::err(err) => Err(self.map_icpswap_error(err)),
            },
            Err((code, msg)) => Err(ExchangeError::CanisterCallError(format!("Failed to call getUserUnusedBalance: {:?} - {}", code, msg))),
        }
    }
    
    /// Execute ICRC1 token transfer to the SwapPool subaccount
    async fn transfer_token_to_pool_subaccount(&self, token: &TokenInfo, pool_id: &Principal, amount: u128, fee: u64) -> ExchangeResult<()> {
        // Execute transfer based on token standard
        match token.standard {
            TokenStandard::ICRC1 | TokenStandard::ICRC2 => {
                // Generate the subaccount for the caller within the SwapPool
                let caller = ic_cdk::caller();
                let subaccount = utils::principal_to_subaccount(&caller);
                
                // Create transfer arguments
                let transfer_args = self.create_icrc1_transfer_args(token, pool_id, &subaccount, amount, fee);
                
                // Call transfer
                let result: CallResult<(candid::Nat,)> = ic_cdk::api::call::call(
                    token.canister_id,
                    "icrc1_transfer",
                    (transfer_args,),
                ).await;
                
                match result {
                    Ok(_) => Ok(()),
                    Err((code, msg)) => Err(ExchangeError::TokenTransferFailed(format!("ICRC1 transfer failed: {:?} - {}", code, msg))),
                }
            },
            _ => Err(ExchangeError::NotImplemented),
        }
    }
    
    /// Create ICRC1 transfer arguments
    fn create_icrc1_transfer_args(&self, token: &TokenInfo, pool_id: &Principal, subaccount: &[u8], amount: u128, fee: u64) -> serde_bytes::ByteBuf {
        #[derive(CandidType, Serialize)]
        struct Account {
            owner: Principal, 
            subaccount: Option<serde_bytes::ByteBuf>,
        }
        
        #[derive(CandidType, Serialize)]
        struct TransferArgs {
            from_subaccount: Option<serde_bytes::ByteBuf>,
            to: Account,
            amount: candid::Nat,
            fee: Option<candid::Nat>,
            memo: Option<serde_bytes::ByteBuf>,
            created_at_time: Option<u64>,
        }
        
        // Create the destination account for the transfer
        let to_account = Account {
            owner: *pool_id,
            subaccount: Some(serde_bytes::ByteBuf::from(subaccount.to_vec())),
        };
        
        // Create transfer arguments
        let transfer_args = TransferArgs {
            from_subaccount: None,
            to: to_account,
            amount: candid::Nat::from(amount),
            fee: Some(candid::Nat::from(fee)),
            memo: None,
            created_at_time: None,
        };
        
        // Serialize arguments
        // Note: Simplified handling here, needs correct serialization in actual application
        serde_bytes::ByteBuf::from(vec![]) // TODO: Implement proper serialization
    }
    
    /// Execute the ICPSwap trade process based on trade parameters
    async fn execute_icpswap_trade(&self, params: &TradeParams) -> ExchangeResult<TradeResult> {
        // 1. Validate trade parameters
        utils::validate_trade_params(params)?;
        
        // 2. Get Pool Canister ID
        let pool_data = self.get_pool_canister(&params.pair.base_token, &params.pair.quote_token).await?;
        
        // 3. Get quote
        let quote_result = self.get_quote_internal(&pool_data.canisterId, params).await?;
        
        // 4. Determine input and output tokens
        let (input_token, output_token) = match params.direction {
            TradeDirection::Buy => (&params.pair.quote_token, &params.pair.base_token),
            TradeDirection::Sell => (&params.pair.base_token, &params.pair.quote_token),
        };
        
        // 5. Determine zero_for_one value and input amount
        let zero_for_one = self.is_zero_for_one(input_token, output_token);
        let amount_in = params.amount.to_string();
        
        // 6. Calculate minimum output amount (considering slippage)
        let amount_out_minimum = (quote_result.output_amount as f64 * (1.0 - params.slippage_tolerance / 100.0)) as u128;
        let amount_out_minimum_str = amount_out_minimum.to_string();
        
        // 7. First, transfer tokens to the corresponding Pool subaccount
        let fee = pool_data.fee.0.bits();
        
        self.transfer_token_to_pool_subaccount(input_token, &&pool_data.canisterId, params.amount, fee).await?;
        
        // 8. Deposit tokens into the Pool via the deposit method
        let deposit_args = ICPSwapDepositArgs {
            fee,
            token: input_token.canister_id.to_string(),
            amount: params.amount.to_string(),
        };
        
        let deposit_result = self.call_deposit(&&pool_data.canisterId, deposit_args).await?;
        
        // 9. Execute the swap
        let swap_args = ICPSwapSwapArgs {
            zeroForOne: zero_for_one,
            amountIn: amount_in,
            amountOutMinimum: amount_out_minimum_str,
        };
        
        let swap_result = self.call_swap(&&pool_data.canisterId, swap_args).await?;
        
        // 10. Withdraw output tokens
        let withdraw_fee = match output_token.standard {
            TokenStandard::ICRC1 => 10000, // ICP fee
            TokenStandard::ICRC2 => 10,    // ckBTC fee
            _ => return Err(ExchangeError::NotImplemented),
        };
        
        let withdraw_args = ICPSwapWithdrawArgs {
            fee: withdraw_fee,
            token: output_token.canister_id.to_string(),
            amount: swap_result,
        };
        
        let withdraw_result = self.call_withdraw(&&pool_data.canisterId, withdraw_args).await?;
        
        // 11. Build the trade result
        let trade_result = TradeResult {
            input_amount: params.amount,
            output_amount: withdraw_result.parse::<u128>().unwrap(), // Assuming withdraw_result is a String representing u128
            fee_amount: (params.amount as f64 * 0.003) as u128, // 0.3% fee
            price: withdraw_result.parse::<f64>().unwrap() / params.amount as f64, // Assuming withdraw_result is a String representing f64 or parsable
            timestamp: utils::current_timestamp_secs(),
            transaction_id: Some(format!("icpswap_{}_{}", &pool_data.canisterId.to_string(), utils::current_timestamp_nanos())),
        };
        
        Ok(trade_result)
    }
    
    /// Internal method to get a quote
    async fn get_quote_internal(&self, pool_id: &Principal, params: &TradeParams) -> ExchangeResult<QuoteResult> {
        // Determine input and output tokens
        let (input_token, output_token) = match params.direction {
            TradeDirection::Buy => (&params.pair.quote_token, &params.pair.base_token),
            TradeDirection::Sell => (&params.pair.base_token, &params.pair.quote_token),
        };
        let zero_for_one = self.is_zero_for_one(input_token, output_token);
        
        // Create quote arguments
        let quote_args = ICPSwapQuoteArgs {
            zeroForOne: zero_for_one,
            amountIn: params.amount.to_string(),
            amountOutMinimum: "0".to_string(), 
        };

        // Call the quote method - expecting Nat now
        let quote_amount_nat = self.call_quote(pool_id, quote_args).await?;
        
        // Convert Nat to u128 via accessing tuple field 0 for BigUint, then cloning and using try_into()
        let quote_amount_u128: u128 = match quote_amount_nat.0.clone().try_into() { 
            Ok(val) => val,
            // Updated error message to show the BigUint value and the error
            Err(e) => return Err(ExchangeError::InternalError(format!("Failed to convert quote BigUint {:?} to u128: {}", quote_amount_nat.0, e))), 
        };

        // Calculate fee (0.3%)
        let fee_amount = (params.amount as f64 * 0.003) as u128;
        
        // Calculate price - handle potential division by zero
        let price = if params.amount > 0 {
            quote_amount_u128 as f64 / params.amount as f64 
        } else {
            0.0
        };

        // Construct quote result
        let quote_result = QuoteResult {
            input_amount: params.amount,
            output_amount: quote_amount_u128,
            price,
            fee_amount,
            price_impact: 0.0, // Price impact not directly available from ICPSwap API, can be improved later
        };

        Ok(quote_result)
    }
}

#[async_trait]
impl Exchange for ICPSwapConnector {
    /// Get the type of the exchange
    fn get_exchange_type(&self) -> ExchangeType {
        ExchangeType::ICPSwap
    }
    
    /// Get the status of the exchange
    async fn get_status(&self) -> ExchangeResult<ExchangeStatus> {
        // Simplified handling here, actual implementation could call ICPSwap API for detailed status
        Ok(ExchangeStatus {
            exchange_type: ExchangeType::ICPSwap,
            is_available: true,
            supported_tokens: vec![],
            supported_pairs: vec![],
            last_updated: utils::current_timestamp_secs(),
        })
    }
    
    /// Query token balance
    async fn get_token_balance(&self, token: &TokenInfo, owner: &Principal) -> ExchangeResult<u128> {
        // Need to call different interfaces based on token standard
        match token.standard {
            TokenStandard::ICRC1 | TokenStandard::ICRC2 => {
                #[derive(CandidType, Serialize)]
                struct Account {
                    owner: Principal,
                    subaccount: Option<serde_bytes::ByteBuf>,
                }
                
                let account = Account {
                    owner: *owner,
                    subaccount: None,
                };
                
                let result: CallResult<(candid::Nat,)> = ic_cdk::api::call::call(
                    token.canister_id,
                    "icrc1_balance_of",
                    (account,),
                ).await;
                
                match result {
                    Ok((balance,)) => Ok(u128::try_from(balance.0).unwrap_or(0)), // Use unwrap_or for safety
                    Err((code, msg)) => Err(ExchangeError::CanisterCallError(format!("Failed to query ICRC balance: {:?} - {}", code, msg))),
                }
            },
            TokenStandard::ICP => {
                // ICP Ledger Canister ID (Mainnet)
                let ledger_canister_id = Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai")
                    .expect("Failed to parse ICP ledger canister ID");

                // Calculate account identifier from principal
                let account_identifier = AccountIdentifier::new(&owner, &DEFAULT_SUBACCOUNT);

                // Prepare arguments for account_balance
                let args = AccountBalanceArgs {
                    account: account_identifier,
                };

                // Call account_balance on the ICP ledger
                let result: CallResult<(Tokens,)> = ic_cdk::api::call::call(
                    ledger_canister_id,
                    "account_balance",
                    (args,),
                ).await;

                match result {
                    Ok((tokens,)) => Ok(tokens.e8s() as u128), // Balance is in e8s (u64), cast to u128
                    Err((code, msg)) => Err(ExchangeError::CanisterCallError(format!("Failed to query ICP balance: {:?} - {}", code, msg))),
                }
            }
            _ => Err(ExchangeError::NotImplemented),
        }
    }
    
    /// Check if a trading pair is supported
    async fn is_pair_supported(&self, base: &TokenInfo, quote: &TokenInfo) -> ExchangeResult<bool> {
        // Try to get pool info; if successful, the pair is supported
        match self.get_pool_canister(base, quote).await {
            Ok(_) => Ok(true),
            Err(ExchangeError::PoolNotFound) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl Trading for ICPSwapConnector {
    /// Get a trading quote
    async fn get_quote(&self, params: &TradeParams) -> ExchangeResult<QuoteResult> {
        // Get Pool Canister ID
        let pool_data = self.get_pool_canister(&params.pair.base_token, &params.pair.quote_token).await?;
        
        // Call internal get quote method
        self.get_quote_internal(&pool_data.canisterId, params).await
    }
    
    /// Execute a trade
    async fn execute_trade(&self, params: &TradeParams) -> ExchangeResult<TradeResult> {
        self.execute_icpswap_trade(params).await
    }
    
    /// Execute multiple trades in a batch
    async fn execute_batch_trade(&self, params: &BatchTradeParams) -> ExchangeResult<BatchTradeResult> {
        let mut results = Vec::new();
        let mut all_succeeded = true;
        
        for trade_param in &params.trades {
            let result = self.execute_trade(trade_param).await;
            
            match &result {
                Ok(_) => {},
                Err(_) => {
                    all_succeeded = false;
                    if params.require_all_success {
                        return Err(ExchangeError::TransactionFailed("Batch trade failed, one or more trades errored".to_string()));
                    }
                }
            }
            
            results.push(result.map_err(|e| e.to_string()));
        }
        
        Ok(BatchTradeResult {
            results,
            all_succeeded,
            timestamp: utils::current_timestamp_secs(),
        })
    }
    
    /// Get trading history
    async fn get_trade_history(&self, user: &Principal, limit: usize, offset: usize) -> ExchangeResult<Vec<TradeHistory>> {
        // ICPSwap currently does not provide an API to get trade history
        // Returning an empty vector here; actual implementation might record and query history elsewhere
        Ok(Vec::new())
    }
}

#[async_trait]
impl TokenOperations for ICPSwapConnector {
    /// Deposit tokens into the exchange
    async fn deposit_token(&self, token: &TokenInfo, amount: u128) -> ExchangeResult<u128> {
        let deposit_args = ICPSwapDepositArgs {
            fee: 3000,
            token: token.canister_id.to_string(),
            amount: amount.to_string(),
        };

        let deposit_result = self.call_deposit(&token.canister_id, deposit_args).await?.parse::<u128>().unwrap();
        Ok(deposit_result)
    }
    
    /// Withdraw tokens from the exchange
    async fn withdraw_token(&self, token: &TokenInfo, amount: u128) -> ExchangeResult<u128> {
        Err(ExchangeError::NotImplemented)
    }
    
    /// Get the user's unused token balance (e.g., balance not in orders or pools)
    async fn get_unused_balance(&self, token: &TokenInfo, user: &Principal) -> ExchangeResult<u128> {
        // Placeholder implementation, needs refinement in actual application
        Err(ExchangeError::NotImplemented)
    }
    
    /// Query the user's total balance within the exchange
    async fn get_exchange_balance(&self, token: &TokenInfo, user: &Principal) -> ExchangeResult<u128> {
        // Placeholder implementation, needs refinement in actual application
        Err(ExchangeError::NotImplemented)
    }
}

// Helper function to convert standard string to TokenStandard enum
fn string_to_token_standard(standard: &str) -> ExchangeResult<TokenStandard> {
    match standard {
        "ICRC1" => Ok(TokenStandard::ICRC1),
        "ICRC2" => Ok(TokenStandard::ICRC2),
        "DIP20" => Ok(TokenStandard::DIP20),
        "EXT" => Ok(TokenStandard::EXT),
        "ICP" => Ok(TokenStandard::ICP),
        _ => Err(ExchangeError::UnsupportedToken(format!("Unknown standard string: {}", standard))),
    }
}

#[async_trait]
impl LiquidityPool for ICPSwapConnector {
    /// Get information about a liquidity pool
    async fn get_pool_info(&self, base: &TokenInfo, quote: &TokenInfo) -> ExchangeResult<PoolInfo> {
        // 1. Get Pool Canister ID and basic data from Factory
        let pool_data = self.get_pool_canister(base, quote).await?;

        // 2. Parse token addresses
        let token0_principal = Principal::from_text(&pool_data.token0.address)
            .map_err(|e| ExchangeError::InternalError(format!("Failed to parse token0 principal: {}", e)))?;
        let token1_principal = Principal::from_text(&pool_data.token1.address)
            .map_err(|e| ExchangeError::InternalError(format!("Failed to parse token1 principal: {}", e)))?;

        // 3. Convert token standards
        let token0_standard_enum = string_to_token_standard(&pool_data.token0.standard)?;
        let token1_standard_enum = string_to_token_standard(&pool_data.token1.standard)?;

        // 4. Determine which input token corresponds to token0/token1 based on sorting
        //    (Assuming ICPSwapPoolData.token0 is the lexicographically smaller one)
        let (actual_token0_info, actual_token1_info) =
            // pool_data.token0 is the base token
            (
                TokenInfo { // Constructing based on pool_data and input base token
                    canister_id: token0_principal,
                    symbol: base.symbol.clone(),
                    decimals: base.decimals,
                    standard: token0_standard_enum,
                },
                TokenInfo { // Constructing based on pool_data and input quote token
                    canister_id: token1_principal,
                    symbol: quote.symbol.clone(),
                    decimals: quote.decimals,
                    standard: token1_standard_enum,
                }
            );
        
        // 5. Convert fee from Nat to u64 (assuming PoolInfo.fee is u64)
        let fee_u64: u64 = match pool_data.fee.0.clone().try_into() {
            Ok(val) => val,
            Err(e) => return Err(ExchangeError::InternalError(format!("Failed to convert pool fee Nat {:?} to u64: {}", pool_data.fee.0, e))),
        };

        // 6. Construct PoolInfo - Liquidity and reserves require calling the pool canister itself
        //    For now, setting them to 0 as placeholders.
        Ok(PoolInfo {
            pool_id: pool_data.canisterId,
            token0: actual_token0_info, // Use the determined token info
            token1: actual_token1_info, // Use the determined token info
            fee: fee_u64,
            total_liquidity: 0, // Placeholder - requires call to pool canister
            token0_reserves: 0, // Placeholder - requires call to pool canister
            token1_reserves: 0, // Placeholder - requires call to pool canister
        })
    }
    
    /// Add liquidity to a pool
    async fn add_liquidity(&self, params: &LiquidityParams) -> ExchangeResult<LiquidityResult> {
        // Placeholder implementation, needs refinement in actual application
        Err(ExchangeError::NotImplemented)
    }
    
    /// Remove liquidity from a pool
    async fn remove_liquidity(&self, pool_id: &Principal, liquidity_amount: u128, min_token0: u128, min_token1: u128) -> ExchangeResult<LiquidityResult> {
        // Placeholder implementation, needs refinement in actual application
        Err(ExchangeError::NotImplemented)
    }
    
    /// Get a user's liquidity in a specific pool
    async fn get_user_liquidity(&self, pool_id: &Principal, user: &Principal) -> ExchangeResult<u128> {
        // Placeholder implementation, needs refinement in actual application
        Err(ExchangeError::NotImplemented)
    }
} 