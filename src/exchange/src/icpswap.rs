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
    pub fee: candid::Nat,       // Keep as Nat
    pub token: String, 
    pub amount: candid::Nat,    // Keep as Nat
}

/// ICPSwap depositFrom arguments
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct ICPSwapDepositFromArgs {
    pub fee: candid::Nat,       // Keep as Nat
    pub token: String,
    pub amount: candid::Nat,    // Keep as Nat
}

/// Generic ICPSwap result type
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum ICPSwapResult {
    ok(candid::Nat), // Changed from String to Nat based on documentation and decoding error
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
    pub balance0: candid::Nat, // Keep as Nat
    pub balance1: candid::Nat, // Keep as Nat
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
    pub fee: candid::Nat,    // Changed from u64 to Nat
    pub token: String,      // Token address as String
    pub amount: candid::Nat, // Changed from String to Nat
}

/// Define a type matching the return value of ckBTC icrc2_approve
#[derive(CandidType, Deserialize, Debug)]
enum ICRCApproveResult {
    ok(candid::Nat),
    err(String),
}

/// ICPSwap ICRC1 transfer result type
#[derive(CandidType, Deserialize, Debug)]
enum ICRC1TransferResult {
    Ok(candid::Nat),
    Err(TransferError),
}

/// ICPSwap ICRC1 transfer error type
#[derive(CandidType, Deserialize, Debug)]
enum TransferError {
    BadFee { expected_fee: candid::Nat },
    BadBurn { min_burn_amount: candid::Nat },
    InsufficientFunds { balance: candid::Nat },
    TooOld,
    CreatedInFuture { ledger_time: u64 },
    Duplicate { duplicate_of: candid::Nat },
    TemporarilyUnavailable,
    GenericError { error_code: candid::Nat, message: String },
}

/// ICP Ledger transfer result type for Candid decoding
#[derive(CandidType, Deserialize, Debug)]
enum ICPTransferResult {
    Ok(u64), // Corresponds to ic_ledger_types::BlockIndex
    Err(ICPTransferError),
}

/// ICP Ledger transfer error type for Candid decoding
/// Mirrors ic_ledger_types::TransferError structure
#[derive(CandidType, Deserialize, Debug)]
enum ICPTransferError {
    BadFee { expected_fee: Tokens },
    InsufficientFunds { balance: Tokens },
    TxTooOld { allowed_window_nanos: u64 },
    TxCreatedInFuture,
    TxDuplicate { duplicate_of: u64 }, // Corresponds to ic_ledger_types::BlockIndex
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
    async fn call_swap(&self, pool_id: &Principal, args: ICPSwapSwapArgs) -> ExchangeResult<candid::Nat> { // Changed return type to Nat
        ic_cdk::println!("Calling swap on pool {} with args: {:?}", pool_id, args); // Debug log
        let result: CallResult<(ICPSwapResult,)> = ic_cdk::api::call::call(
            *pool_id,
            "swap",
            (args,),
        ).await;
        ic_cdk::println!("swap result: {:?}", result); // Debug log

        match result {
            Ok((swap_result,)) => match swap_result {
                ICPSwapResult::ok(amount_nat) => Ok(amount_nat), // Return Nat
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
    async fn call_deposit(&self, pool_id: &Principal, args: ICPSwapDepositArgs) -> ExchangeResult<candid::Nat> { // Changed return type to Nat
        ic_cdk::println!("Calling deposit on pool {} with args: {:?}", pool_id, args); // Debug log
        let result: CallResult<(ICPSwapResult,)> = ic_cdk::api::call::call(
            *pool_id,
            "deposit",
            (args,),
        ).await;
        ic_cdk::println!("deposit result: {:?}", result); // Debug log

        match result {
            Ok((deposit_result,)) => match deposit_result {
                ICPSwapResult::ok(amount_nat) => Ok(amount_nat), // Return Nat
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

    /// Calls the depositFrom method on the ICPSwap pool canister
    async fn call_deposit_from(&self, pool_id: &Principal, args: ICPSwapDepositFromArgs) -> ExchangeResult<candid::Nat> { // Changed return type to Nat
        ic_cdk::println!("Calling depositFrom on pool {} with args: {:?}", pool_id, args);
        let result: CallResult<(ICPSwapResult,)> = ic_cdk::api::call::call(
            *pool_id,
            "depositFrom",
            (args,),
        ).await;
        ic_cdk::println!("depositFrom result: {:?}", result);

        match result {
            Ok((deposit_result,)) => match deposit_result {
                ICPSwapResult::ok(amount_nat) => Ok(amount_nat), // Return Nat
                ICPSwapResult::err(err) => {
                    ic_cdk::println!("depositFrom returned error: {:?}", err);
                    Err(self.map_icpswap_error(err))
                },
            },
            Err((code, msg)) => {
                ic_cdk::println!("depositFrom call failed: {:?} - {}", code, msg);
                Err(ExchangeError::CanisterCallError(format!("Failed to call depositFrom: {:?} - {}", code, msg)))
            },
        }
    }

    /// Calls the withdraw method on the ICPSwap pool canister
    async fn call_withdraw(&self, pool_id: &Principal, args: ICPSwapWithdrawArgs) -> ExchangeResult<candid::Nat> { // Changed return type to Nat
        ic_cdk::println!("Calling withdraw on pool {} with args: {:?}", pool_id, args); // Debug log
        let result: CallResult<(ICPSwapResult,)> = ic_cdk::api::call::call(
            *pool_id,
            "withdraw",
            (args,),
        ).await;
        ic_cdk::println!("withdraw result: {:?}", result); // Debug log

        match result {
            Ok((withdraw_result,)) => match withdraw_result {
                ICPSwapResult::ok(amount_nat) => Ok(amount_nat), // Return Nat
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
                ICPSwapBalanceResult::ok(balance) => {
                    // Convert Nat to u128 safely
                    let balance0_u128 = u128::try_from(balance.balance0.0.clone())
                        .map_err(|e| ExchangeError::InternalError(format!("Failed to convert balance0 Nat {:?} to u128: {}", balance.balance0.0, e)))?;
                    let balance1_u128 = u128::try_from(balance.balance1.0.clone())
                        .map_err(|e| ExchangeError::InternalError(format!("Failed to convert balance1 Nat {:?} to u128: {}", balance.balance1.0, e)))?;
                    Ok((balance0_u128, balance1_u128))
                },
                ICPSwapBalanceResult::err(err) => Err(self.map_icpswap_error(err)),
            },
            Err((code, msg)) => Err(ExchangeError::CanisterCallError(format!("Failed to call getUserUnusedBalance: {:?} - {}", code, msg))),
        }
    }
    
    /// Execute ICRC1 token transfer to the SwapPool subaccount
    async fn transfer_token_to_pool_subaccount(&self, token: &TokenInfo, pool_id: &Principal, amount: u128, fee: u64) -> ExchangeResult<()> {
        ic_cdk::println!("Transferring token {} to pool {} subaccount", token.canister_id, pool_id);
        let caller = ic_cdk::caller();

        // Execute transfer based on token standard
        match token.standard {
            TokenStandard::ICRC1 | TokenStandard::ICRC2 => {
                // Generate subaccount for the caller
                let subaccount = utils::principal_to_subaccount(&caller);
                let pool_subaccount = utils::principal_to_subaccount(pool_id);

                // Define ICRC Account structure
                #[derive(CandidType, Deserialize)]
                struct Account {
                    owner: Principal,
                    subaccount: Option<Vec<u8>>,
                }

                // Define ICRC Transfer arguments
                #[derive(CandidType)]
                struct TransferArgs {
                    from_subaccount: Option<Vec<u8>>,
                    to: Account,
                    amount: candid::Nat,
                    fee: Option<candid::Nat>,
                    memo: Option<Vec<u8>>,
                    created_at_time: Option<u64>,
                }

                // Create the destination account
                let to_account = Account {
                    owner: *pool_id,
                    subaccount: Some(pool_subaccount.to_vec()),
                };

                // Create transfer arguments
                let transfer_args = TransferArgs {
                    from_subaccount: Some(subaccount.to_vec()),
                    to: to_account,
                    amount: candid::Nat::from(amount),
                    fee: Some(candid::Nat::from(fee)), // Assuming fee is required, adjust if optional
                    memo: None,
                    created_at_time: None,
                };

                // Call the transfer
                let call_result: CallResult<(ICRC1TransferResult,)> = ic_cdk::api::call::call(
                    token.canister_id,
                    "icrc1_transfer",
                    (transfer_args,),
                ).await;
                
                match call_result {
                    Ok((transfer_result,)) => match transfer_result {
                        ICRC1TransferResult::Ok(block_index) => {
                            ic_cdk::println!("ICRC transfer successful, block index: {}", block_index);
                            Ok(())
                        },
                        ICRC1TransferResult::Err(err) => {
                            let error_msg = match &err {
                                TransferError::BadFee { expected_fee } => 
                                    format!("Bad fee, expected: {}", expected_fee),
                                TransferError::BadBurn { min_burn_amount } => 
                                    format!("Bad burn, minimum: {}", min_burn_amount),
                                TransferError::InsufficientFunds { balance } => 
                                    format!("Insufficient funds, balance: {}", balance),
                                TransferError::TooOld => 
                                    "Transaction too old".to_string(),
                                TransferError::CreatedInFuture { ledger_time } => 
                                    format!("Transaction created in future, ledger time: {}", ledger_time),
                                TransferError::Duplicate { duplicate_of } => 
                                    format!("Duplicate transaction of: {}", duplicate_of),
                                TransferError::TemporarilyUnavailable => 
                                    "Ledger temporarily unavailable".to_string(),
                                TransferError::GenericError { error_code, message } => 
                                    format!("Generic error {}: {}", error_code, message),
                            };
                            ic_cdk::println!("ICRC transfer error: {}", error_msg);
                            Err(ExchangeError::TokenTransferFailed(format!("ICRC transfer failed: {}", error_msg)))
                        }
                    },
                    Err((code, msg)) => {
                        ic_cdk::println!("ICRC transfer call failed: {:?} - {}", code, msg);
                        Err(ExchangeError::TokenTransferFailed(
                            format!("ICRC transfer failed: {:?} - {}", code, msg)
                        ))
                    },
                }
            },
            TokenStandard::ICP => {
                // Special handling logic for ICP, as it uses ic-ledger-types
                ic_cdk::println!("Handling ICP transfer");
                let caller_subaccount_bytes = utils::principal_to_subaccount(&caller);
                // Convert to subaccount format
                let mut from_subaccount_array = [0u8; 32];
                for (i, byte) in caller_subaccount_bytes.iter().enumerate().take(32) {
                    from_subaccount_array[i] = *byte;
                }
                let from_subaccount = ic_ledger_types::Subaccount(from_subaccount_array);

                let pool_subaccount_bytes = utils::principal_to_subaccount(pool_id);
                let mut pool_subaccount_array = [0u8; 32];
                 for (i, byte) in pool_subaccount_bytes.iter().enumerate().take(32) {
                    pool_subaccount_array[i] = *byte;
                }
                let pool_subaccount = ic_ledger_types::Subaccount(pool_subaccount_array);

                // Build the target account identifier
                let to_account_identifier = AccountIdentifier::new(pool_id, &pool_subaccount);

                // Prepare ICP transfer arguments
                let transfer_args = ic_ledger_types::TransferArgs {
                    memo: ic_ledger_types::Memo(0), // Use a default memo or make it configurable
                    amount: Tokens::from_e8s(amount as u64), // Assuming amount fits in u64 E8s
                    fee: Tokens::from_e8s(10_000), // Standard ICP transfer fee
                    from_subaccount: Some(from_subaccount), // Keep this Some(from_subaccount) as per original logic before erroneous edit
                    to: to_account_identifier,
                    created_at_time: None,
                };

                // Call the ICP ledger's transfer method
                let ledger_canister_id = Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai")
                     .expect("Failed to parse ICP ledger canister ID"); // Use hardcoded ICP ledger ID as before
                let call_result: CallResult<(ICPTransferResult,)> = ic_cdk::api::call::call(
                    ledger_canister_id, // Use hardcoded ICP ledger ID
                    "transfer",
                    (transfer_args,),
                ).await;
                
                match call_result {
                    Ok((transfer_result,)) => match transfer_result {
                        ICPTransferResult::Ok(block_index) => {
                            ic_cdk::println!("ICP transfer successful, block index: {}", block_index);
                            Ok(())
                        },
                        ICPTransferResult::Err(err) => {
                            let error_msg = match err {
                                ICPTransferError::BadFee { expected_fee } => 
                                    format!("Bad fee, expected: {} e8s", expected_fee.e8s()),
                                ICPTransferError::InsufficientFunds { balance } => 
                                    format!("Insufficient funds, balance: {} e8s", balance.e8s()),
                                ICPTransferError::TxTooOld { allowed_window_nanos } => 
                                    format!("Transaction too old, allowed window: {}ns", allowed_window_nanos),
                                ICPTransferError::TxCreatedInFuture => 
                                    "Transaction created in the future".to_string(),
                                ICPTransferError::TxDuplicate { duplicate_of } => 
                                    format!("Duplicate transaction of block index: {}", duplicate_of),
                            };
                            ic_cdk::println!("ICP transfer failed: {}", error_msg);
                            Err(ExchangeError::TokenTransferFailed(
                                format!("ICP transfer failed: {}", error_msg)
                            ))
                        }
                    },
                    Err((code, msg)) => {
                        ic_cdk::println!("ICP transfer call failed: {:?} - {}", code, msg);
                        Err(ExchangeError::TokenTransferFailed(
                            format!("ICP transfer call failed: {:?} - {}", code, msg)
                        ))
                    },
                }
            },
            TokenStandard::DIP20 | TokenStandard::EXT => {
                // DIP20/EXT doesn't need this step, as they use Workflow 2, transferring directly from the user via depositFrom
                ic_cdk::println!("DIP20/EXT tokens use Workflow 2, skipping transfer_token_to_pool_subaccount");
                Ok(())
            },
        }
    }
    
    /// Execute trade based on token standard
    async fn execute_icpswap_trade(&self, params: &TradeParams) -> ExchangeResult<TradeResult> {
        // 1. Validate trade parameters
        utils::validate_trade_params(params)?;
        
        // 2. Get pool information
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
        let amount_in_u128 = params.amount;
        let amount_in_nat = candid::Nat::from(amount_in_u128);
        let amount_in_str = amount_in_u128.to_string();
        
        // 6. Calculate minimum output amount (considering slippage)
        let amount_out_minimum = (quote_result.output_amount as f64 * (1.0 - params.slippage_tolerance / 100.0)) as u128;
        let amount_out_minimum_str = amount_out_minimum.to_string();
        
        // 7. Get pool_fee (u64) and calculate input_token_fee (Nat) beforehand
        let pool_fee_u64 = u64::try_from(pool_data.fee.0.clone())
            .map_err(|e| ExchangeError::InternalError(format!("Failed to convert pool fee Nat {:?} to u64: {}", pool_data.fee.0, e)))?;
        let input_token_fee = match input_token.standard {
            TokenStandard::ICRC1 | TokenStandard::ICP => 10000_u64,
            TokenStandard::ICRC2 => 10_u64, 
            _ => 0_u64, 
        };
        let input_token_fee_nat = candid::Nat::from(input_token_fee);

        // 8. Define caller and swap_result
        let caller = ic_cdk::caller(); 
        let mut swap_result = candid::Nat::from(0u64); // Ensure this initialization is correct

        // 9. Choose different trade flows based on token standard
        match input_token.standard {
            // --- Workflow 1 (ICRC1 & ICP) --- 
            TokenStandard::ICRC1  => {
                ic_cdk::println!("Executing Workflow 1 for {:?}", input_token.standard);
                
                // Step 2: Transfer token to SwapPool's subaccount (using u64 fee)
                ic_cdk::println!("Transferring token to pool subaccount");
                self.transfer_token_to_pool_subaccount(input_token, &pool_data.canisterId, amount_in_u128, input_token_fee).await?;
                
                // Step 3: Call deposit method (using Nat fee)
                let deposit_args = ICPSwapDepositArgs {
                    fee: input_token_fee_nat.clone(), // Use input token's standard fee (Nat)
                    token: input_token.canister_id.to_string(),
                    amount: amount_in_nat.clone(),
                };
                
                ic_cdk::println!("Depositing token to pool with token fee: {}", input_token_fee);
                let deposit_result = match self.call_deposit(&pool_data.canisterId, deposit_args).await {
                    Ok(result) => result,
                    Err(e) => {
                        ic_cdk::println!("Deposit failed: {:?}", e);
                        // Potentially call handle_deposit_failure here if needed, considering async context
                        return Err(e);
                    }
                };
                ic_cdk::println!("Deposit result: {}", deposit_result);

                // Step 4: Execute swap
                let swap_args = ICPSwapSwapArgs {
                    zeroForOne: zero_for_one,
                    amountIn: amount_in_str, 
                    amountOutMinimum: amount_out_minimum_str,
                };
                ic_cdk::println!("Executing swap");
                swap_result = self.call_swap(&pool_data.canisterId, swap_args).await
                   .map_err(|e| {
                        ic_cdk::println!("Swap failed directly: {:?}", e);
                        // Potentially call handle_swap_failure here
                        e 
                    })?; 
                ic_cdk::println!("Swap result: {}", swap_result);
            },
            // --- Workflow 2 (ICRC2, DIP20, EXT) --- 
            TokenStandard::DIP20 | TokenStandard::EXT | TokenStandard::ICRC2 | TokenStandard::ICP => {
                ic_cdk::println!("Executing Workflow 2 for {:?}", input_token.standard);
                
                // Step 2: NO APPROVE CALL HERE. User MUST approve the pool BEFORE calling execute_trade.
                self.approve_token(input_token, &pool_data.canisterId, amount_in_u128 * 100).await?;
                ic_cdk::println!("Skipping internal approve. Assuming user pre-approved pool {:?}", pool_data.canisterId);

                // Step 3: Call depositFrom method (using Nat fee)
                let deposit_args = ICPSwapDepositFromArgs {
                    fee: input_token_fee_nat.clone(), // Use input token's standard fee (Nat)
                    token: input_token.canister_id.to_string(),
                    amount: amount_in_nat.clone(),
                };
                
                ic_cdk::println!("Calling depositFrom with token fee: {}", input_token_fee);
                let deposit_result = self.call_deposit_from(&pool_data.canisterId, deposit_args).await
                    .map_err(|e| {
                        ic_cdk::println!("DepositFrom failed directly: {:?}", e);
                        // Potentially call handle_deposit_failure here
                        e 
                    })?; 
                ic_cdk::println!("DepositFrom result: {}", deposit_result);
                
                 // Step 4: Execute swap
                 let swap_args = ICPSwapSwapArgs {
                     zeroForOne: zero_for_one,
                     amountIn: amount_in_str,
                     amountOutMinimum: amount_out_minimum_str,
                 };
                ic_cdk::println!("Executing swap");
                 swap_result = self.call_swap(&pool_data.canisterId, swap_args).await
                    .map_err(|e| {
                         ic_cdk::println!("Swap failed directly: {:?}", e);
                         // Potentially call handle_swap_failure here
                         e
                     })?; 
                ic_cdk::println!("Swap result: {}", swap_result);
            },
        }
        
        // 10. Step 5: Withdraw output token (Common for all workflows)
        let withdraw_fee_u64 = match output_token.standard {
            TokenStandard::ICRC1 | TokenStandard::ICP => 10000_u64,
            TokenStandard::ICRC2 => 10_u64, 
            TokenStandard::DIP20 => 0_u64, 
            TokenStandard::EXT => 0_u64,
        };
        let withdraw_fee_nat = candid::Nat::from(withdraw_fee_u64); // Convert fee to Nat
        let withdraw_args = ICPSwapWithdrawArgs {
            fee: withdraw_fee_nat,                       // Pass Nat fee
            token: output_token.canister_id.to_string(),
            amount: swap_result.clone(),                 // Pass Nat amount directly (use swap_result)
        };
        ic_cdk::println!("Withdrawing output token with args: {:?}", withdraw_args);
        let withdraw_result_nat = match self.call_withdraw(&pool_data.canisterId, withdraw_args).await {
            Ok(result) => result, // Result is Nat
            Err(e) => {
                ic_cdk::println!("Withdraw failed: {:?}", e);
                // Potentially call handle_withdraw_failure here
                return Err(e);
            }
        };
        
        // 11. Build trade result - Use swap_result (Nat) which represents the amount before withdrawal fee
        let final_output_amount_u128 = u128::try_from(swap_result.0.clone()).map_err(|e| { // Use swap_result for final amount
            ExchangeError::InternalError(format!("Failed to convert final swap result Nat {:?} to u128: {}", swap_result.0, e))
        })?;
        let trade_result = TradeResult {
            input_amount: params.amount,
            output_amount: final_output_amount_u128, // Use the amount calculated from swap_result
            fee_amount: (params.amount as f64 * (pool_fee_u64 as f64 / 1_000_000.0)) as u128, 
            price: if params.amount > 0 { final_output_amount_u128 as f64 / params.amount as f64 } else { 0.0 },
            timestamp: utils::current_timestamp_secs(),
            transaction_id: Some(format!("icpswap_{}_{}", pool_data.canisterId.to_string(), utils::current_timestamp_nanos())),
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

    /// Approve token
    async fn approve_token(&self, token: &TokenInfo, spender: &Principal, amount: u128) -> ExchangeResult<()> {
        match token.standard {
            TokenStandard::DIP20 => {
                // DIP20 approve
                #[derive(CandidType, Serialize, Debug)]
                struct ApproveArgs {
                    spender: Principal,
                    amount: candid::Nat,
                }
                
                // Define DIP20 approve return value type
                #[derive(CandidType, Deserialize, Debug)]
                enum DIP20ApproveResult {
                    ok(()),
                    err(String),
                }

                let args = ApproveArgs {
                    spender: *spender,
                    amount: candid::Nat::from(amount),
                };
                
                ic_cdk::println!("Calling DIP20 approve with args: {:?}", &args);
                let result: CallResult<(DIP20ApproveResult,)> = ic_cdk::api::call::call(
                    token.canister_id,
                    "approve",
                    (args,),
                ).await;
                
                match result {
                    Ok((approve_result,)) => match approve_result {
                        DIP20ApproveResult::ok(()) => {
                            ic_cdk::println!("DIP20 approve successful");
                            Ok(())
                        },
                        DIP20ApproveResult::err(e) => {
                            ic_cdk::println!("DIP20 approve returned error: {}", e);
                            Err(ExchangeError::TokenApprovalFailed(e))
                        },
                    },
                    Err((code, msg)) => {
                        ic_cdk::println!("DIP20 approve call failed: {:?} - {}", code, msg);
                        Err(ExchangeError::CanisterCallError(
                            format!("DIP20 approve failed: {:?} - {}", code, msg)
                        ))
                    },
                }
            },
            TokenStandard::EXT => {
                // EXT approve
                #[derive(CandidType, Serialize, Debug)]
                struct ApproveArgs {
                    subaccount: Option<Vec<u8>>,
                    spender: Principal,
                    allowance: candid::Nat,
                }
                
                // Define EXT approve return value type
                #[derive(CandidType, Deserialize, Debug)]
                enum EXTApproveResult {
                    ok(()),
                    err(String),
                }

                let args = ApproveArgs {
                    subaccount: None,
                    spender: *spender,
                    allowance: candid::Nat::from(amount),
                };
                
                ic_cdk::println!("Calling EXT approve with args: {:?}", &args);
                let result: CallResult<(EXTApproveResult,)> = ic_cdk::api::call::call(
                    token.canister_id,
                    "approve",
                    (args,),
                ).await;
                
                match result {
                    Ok((approve_result,)) => match approve_result {
                        EXTApproveResult::ok(()) => {
                            ic_cdk::println!("EXT approve successful");
                            Ok(())
                        },
                        EXTApproveResult::err(e) => {
                            ic_cdk::println!("EXT approve returned error: {}", e);
                            Err(ExchangeError::TokenApprovalFailed(e))
                        },
                    },
                    Err((code, msg)) => {
                        ic_cdk::println!("EXT approve call failed: {:?} - {}", code, msg);
                        Err(ExchangeError::CanisterCallError(
                            format!("EXT approve failed: {:?} - {}", code, msg)
                        ))
                    },
                }
            },
            TokenStandard::ICRC2 | TokenStandard::ICP => {
                // Define ICRC Account struct for spender field
                #[derive(CandidType, Serialize, Deserialize, Clone, Debug)] // Add Deserialize and Clone
                struct Account {
                    owner: Principal,
                    subaccount: Option<Vec<u8>>,
                }

                // Define correct ICRC2 Approve arguments struct
                #[derive(CandidType, Serialize, Debug)]
                struct CorrectICRC2ApproveArgs {
                    spender: Account, // Use Account struct
                    amount: candid::Nat,
                }

                // Keep full return value and error type definitions
                #[derive(CandidType, Deserialize, Debug)]
                enum ICRCApproveResult {
                    Ok(candid::Nat),
                    Err(ApproveError),
                }
                
                #[derive(CandidType, Deserialize, Debug)]
                enum ApproveError {
                    BadFee { expected_fee: candid::Nat },
                    InsufficientFunds { balance: candid::Nat },
                    AllowanceChanged { current_allowance: candid::Nat },
                    Expired { ledger_time: u64 },
                    TooOld,
                    CreatedInFuture { ledger_time: u64 },
                    Duplicate { duplicate_of: candid::Nat },
                    TemporarilyUnavailable,
                    GenericError { error_code: candid::Nat, message: String },
                }
                
                // Create arguments instance, spender is an Account record
                let args = CorrectICRC2ApproveArgs {
                    spender: Account {
                        owner: *spender, // Passed spender principal as owner
                        subaccount: None, // Set subaccount to None
                    },
                    amount: candid::Nat::from(amount),
                };
                
                ic_cdk::println!("Calling icrc2_approve with correct spender Account: {:?}", &args);
                let result: CallResult<(ICRCApproveResult,)> = ic_cdk::api::call::call(
                    token.canister_id,
                    "icrc2_approve",
                    (args,),
                ).await;
                
                match result {
                    Ok((approve_result,)) => match approve_result {
                        ICRCApproveResult::Ok(_) => {
                            ic_cdk::println!("ICRC2 approve successful");
                            Ok(())
                        },
                        ICRCApproveResult::Err(e) => {
                            let error_msg = match &e {
                                ApproveError::BadFee { expected_fee } => 
                                    format!("Bad fee, expected: {}", expected_fee),
                                ApproveError::InsufficientFunds { balance } => 
                                    format!("Insufficient funds, balance: {}", balance),
                                ApproveError::AllowanceChanged { current_allowance } => 
                                    format!("Allowance changed, current: {}", current_allowance),
                                ApproveError::Expired { ledger_time } => 
                                    format!("Expired, ledger time: {}", ledger_time),
                                ApproveError::TooOld => 
                                    "Transaction too old".to_string(),
                                ApproveError::CreatedInFuture { ledger_time } => 
                                    format!("Created in future, ledger time: {}", ledger_time),
                                ApproveError::Duplicate { duplicate_of } => 
                                    format!("Duplicate of: {}", duplicate_of),
                                ApproveError::TemporarilyUnavailable => 
                                    "Temporarily unavailable".to_string(),
                                ApproveError::GenericError { error_code, message } => 
                                    format!("Generic error {}: {}", error_code, message),
                            };
                            ic_cdk::println!("ICRC2 approve returned error: {}", error_msg);
                            Err(ExchangeError::TokenApprovalFailed(error_msg))
                        },
                    },
                    Err((code, msg)) => {
                        ic_cdk::println!("ICRC2 approve call failed: {:?} - {}", code, msg);
                        Err(ExchangeError::CanisterCallError(
                            format!("ICRC2 approve failed: {:?} - {}", code, msg)
                        ))
                    },
                }
            },
            _ => Err(ExchangeError::UnsupportedToken(format!("Token standard {:?} does not support approve", token.standard))),
        }
    }

    /// Handle deposit failure
    async fn handle_deposit_failure(&self, pool_id: &Principal, user: &Principal, token: &TokenInfo) -> ExchangeResult<()> {
        ic_cdk::println!("Handling deposit failure for user {} in pool {}", user, pool_id);
        
        // Try to get the user's unused balance in the pool
        let balances = self.call_get_user_unused_balance(pool_id, user).await?;
        ic_cdk::println!("User unused balance: token0={}, token1={}", balances.0, balances.1);
        
        // Balance handling strategy can be implemented based on business requirements
        // For example, one could try calling deposit again, or withdraw tokens from the pool
        
        Ok(())
    }

    /// Handle swap failure
    async fn handle_swap_failure(&self, pool_id: &Principal, user: &Principal, params: &TradeParams) -> ExchangeResult<()> {
        ic_cdk::println!("Handling swap failure for user {} in pool {}", user, pool_id);
        
        // Get user's unused balance in the pool
        let balances = self.call_get_user_unused_balance(pool_id, user).await?;
        ic_cdk::println!("User unused balance: token0={}, token1={}", balances.0, balances.1);
        
        // Determine if tokens need to be withdrawn
        // Based on business requirements, decide whether to automatically withdraw tokens or let the user handle it manually
        
        Ok(())
    }

    /// Handle withdraw failure
    async fn handle_withdraw_failure(&self, pool_id: &Principal, user: &Principal, token: &TokenInfo) -> ExchangeResult<()> {
        ic_cdk::println!("Handling withdraw failure for user {} in pool {}", user, pool_id);
        
        // Get user's unused balance in the pool
        let balances = self.call_get_user_unused_balance(pool_id, user).await?;
        ic_cdk::println!("User unused balance: token0={}, token1={}", balances.0, balances.1);
        
        // Check transaction logs to determine if the transfer was successful
        // Note: Since we haven't implemented getTransferLogs, this is just a framework example
        
        // If the transaction is confirmed failed, record the error and notify administrators
        ic_cdk::println!("Withdraw failed, transfer logs need to be verified manually");
        
        Ok(())
    }

    /// Add a method to check the current balance
    async fn check_token_balance(&self, token: &TokenInfo) -> ExchangeResult<String> {
        let canister_id = ic_cdk::id();
        let balance = self.get_token_balance(token, &canister_id).await?;
        Ok(format!("Current balance: {}", balance))
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

    async fn execute_call_trade(
        &self, params: &TradeParams
    ) -> ExchangeResult<TradeResult> {
        // 2. Get pool information
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
        let amount_in_u128 = params.amount;
        let amount_in_nat = candid::Nat::from(amount_in_u128);
        let amount_in_str = amount_in_u128.to_string();

        // 6. Calculate minimum output amount (considering slippage)
        let amount_out_minimum = (quote_result.output_amount as f64 * (1.0 - params.slippage_tolerance / 100.0)) as u128;
        let amount_out_minimum_str = amount_out_minimum.to_string();
        // Step 4: Execute swap
        let swap_args = ICPSwapSwapArgs {
            zeroForOne: zero_for_one,
            amountIn: amount_in_str,
            amountOutMinimum: amount_out_minimum_str,
        };
        ic_cdk::println!("Executing swap");
        let swap_result = self.call_swap(&pool_data.canisterId, swap_args).await
            .map_err(|e| {
                ic_cdk::println!("Swap failed directly: {:?}", e);
                // Potentially call handle_swap_failure here
                e
            })?;
        ic_cdk::println!("Swap result: {}", swap_result);
        let final_output_amount_u128 = u128::try_from(swap_result.0.clone()).map_err(|e| { // Use swap_result for final amount
            ExchangeError::InternalError(format!("Failed to convert final swap result Nat {:?} to u128: {}", swap_result.0, e))
        })?;
        let pool_fee_u64 = u64::try_from(pool_data.fee.0.clone())
            .map_err(|e| ExchangeError::InternalError(format!("Failed to convert pool fee Nat {:?} to u64: {}", pool_data.fee.0, e)))?;
        let trade_result = TradeResult {
            input_amount: params.amount,
            output_amount: final_output_amount_u128, // Use the amount calculated from swap_result
            fee_amount: (params.amount as f64 * (pool_fee_u64 as f64 / 1_000_000.0)) as u128,
            price: if params.amount > 0 { final_output_amount_u128 as f64 / params.amount as f64 } else { 0.0 },
            timestamp: utils::current_timestamp_secs(),
            transaction_id: Some(format!("icpswap_{}_{}", pool_data.canisterId.to_string(), utils::current_timestamp_nanos())),
        };

        Ok(trade_result)
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
    async fn approve_token(&self,token: &TokenInfo, spender: &Principal, amount: u128) -> ExchangeResult<()> {
        self.approve_token(token, spender, amount).await
    }
    /// Deposit tokens into the exchange
    async fn deposit_token(&self, params: &TradeParams,token: &TokenInfo, amount: u128) -> ExchangeResult<u128> {
        match token.standard {
            TokenStandard::ICRC1 => {
                Err(ExchangeError::InternalError("Unsupported type: ICRC1".to_string()))
            }
            TokenStandard::ICRC2| TokenStandard::ICP | TokenStandard::EXT|TokenStandard::DIP20=> {
                let input_token_fee = match token.standard {
                    TokenStandard::ICRC1 | TokenStandard::ICP => 10000_u64,
                    TokenStandard::ICRC2 => 1_000_000,
                    _ => 0_u64,
                };
                let input_token_fee_nat = candid::Nat::from(input_token_fee);
                let pool_data = self.get_pool_canister(&params.pair.base_token, &params.pair.quote_token).await?;
                // Step 2: NO APPROVE CALL HERE. User MUST approve the pool BEFORE calling execute_trade.
                self.approve_token(token, &pool_data.canisterId, amount * 100).await?;
                ic_cdk::println!("Skipping internal approve. Assuming user pre-approved pool {:?}", pool_data.canisterId);
                let amount_nat = candid::Nat::from(amount);
                // Step 3: Call depositFrom method (using Nat fee)
                let deposit_args = ICPSwapDepositFromArgs {
                    fee: input_token_fee_nat.clone(), // Use input token's standard fee (Nat)
                    token: token.canister_id.to_string(),
                    amount: amount_nat.clone(),
                };

                ic_cdk::println!("Calling depositFrom with token fee: {}", input_token_fee_nat);
                let deposit_result = self.call_deposit_from(&pool_data.canisterId, deposit_args).await
                    .map_err(|e| {
                        ic_cdk::println!("DepositFrom failed directly: {:?}", e);
                        // Potentially call handle_deposit_failure here
                        e
                    })?;
                ic_cdk::println!("DepositFrom result: {}", deposit_result);
                let deposit_result_u128 = u128::try_from(deposit_result.0.clone())
                    .map_err(|e| ExchangeError::InternalError(format!("Failed to convert withdraw result Nat {:?} to u128: {}", deposit_result.0, e)))?;

                Ok(deposit_result_u128)
            }
        }
        
    }
    
    /// Withdraw tokens from the exchange
    async fn withdraw_token(&self, params: &TradeParams,token: &TokenInfo, amount: u128) -> ExchangeResult<u128> { 
        let pool_data = self.get_pool_canister(&params.pair.base_token, &params.pair.quote_token).await?;
        let withdraw_fee_u64 = match token.standard {
            TokenStandard::ICRC1 | TokenStandard::ICP => 10000_u64,
            TokenStandard::ICRC2 => 1_000_000,
            TokenStandard::DIP20 => 0_u64,
            TokenStandard::EXT => 0_u64,
        };
        let withdraw_fee_nat = candid::Nat::from(withdraw_fee_u64); // Convert fee to Nat
        let withdraw_args = ICPSwapWithdrawArgs {
            fee: withdraw_fee_nat,                       // Pass Nat fee
            token: token.canister_id.to_string(),
            amount: Nat::from(amount.clone()),                 // Pass Nat amount directly (use swap_result)
        };
        ic_cdk::println!("Withdrawing output token with args: {:?}", withdraw_args);
        let withdraw_result_nat = match self.call_withdraw(&pool_data.canisterId, withdraw_args).await {
            Ok(result) => result, // Result is Nat
            Err(e) => {
                ic_cdk::println!("Withdraw failed: {:?}", e);
                // Potentially call handle_withdraw_failure here
                return Err(e);
            }
        };

        // Convert the Nat result to u128 and return it
        let withdrawn_amount_u128 = u128::try_from(withdraw_result_nat.0.clone())
            .map_err(|e| ExchangeError::InternalError(format!("Failed to convert withdraw result Nat {:?} to u128: {}", withdraw_result_nat.0, e)))?;

        Ok(withdrawn_amount_u128) // Return the successfully withdrawn amount as u128
    }
    
    /// Query the user's unused token balance (e.g., balance not in orders or pools)
    async fn get_unused_balance(&self, params: &TradeParams, user: &Principal) -> ExchangeResult<(u128,u128,String)> { // Added underscores
        // Get pool information
        let pool_data = self.get_pool_canister(&params.pair.base_token, &params.pair.quote_token).await?;
        let(balance0_u128, balance1_u128)= self.call_get_user_unused_balance(&pool_data.canisterId, user).await?;
        Ok((balance0_u128,balance1_u128,pool_data.token0.address))
    }
    
    /// Query the user's total balance within the exchange
    async fn get_exchange_balance(&self, _token: &TokenInfo, _user: &Principal) -> ExchangeResult<(u128,u128)> { // Added underscores
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