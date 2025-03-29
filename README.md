# HunterFi - ICP Quantitative Trading Platform

<div align="center">
  <img src="favicon.svg" alt="HunterFi Logo" width="200"/>
  <p>
    <strong>Decentralized Quantitative Trading Platform Based on Internet Computer</strong>
  </p>
  <p>
    <a href="https://internetcomputer.org/"><img src="https://img.shields.io/badge/Platform-Internet%20Computer-blue" alt="Platform" /></a>
    <a href="https://internetcomputer.org/docs/current/developer-docs/backend/rust/"><img src="https://img.shields.io/badge/Backend-Rust-orange" alt="Rust" /></a>
    <a href="https://github.com/dfinity/candid"><img src="https://img.shields.io/badge/IDL-Candid-yellow" alt="Candid" /></a>
    <a href="https://reactjs.org/"><img src="https://img.shields.io/badge/Frontend-React-blue" alt="React" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-green" alt="License" /></a>
  </p>
</div>

## 📖 Overview

HunterFi is a decentralized quantitative trading platform built on the Internet Computer Protocol (ICP), allowing users to create, deploy, and manage various automated trading strategies. The platform leverages ICP's trustless computing capabilities to provide enhanced security, transparency, and decentralization.

### 🌟 Key Features

- **Decentralized Deployment**: All strategies are deployed as independent canisters on ICP
- **Diverse Strategies**: Supports multiple strategies including Dollar-Cost Averaging (DCA), Value Averaging, Fixed Balance, Limit Orders, and more
- **Exchange Integration**: Supports decentralized exchanges like ICPSwap, KongSwap, and more
- **Security & Reliability**: Open-source strategy code, self-custody of funds, no need for asset custody
- **Customizable**: Users can adjust strategy parameters according to their specific needs
- **Real-time Monitoring**: Provides visualization of strategy performance and historical transaction data

## 💱 DEX Integration Status

The following table shows the current integration status for various decentralized exchanges:

| Exchange | Status | Features Supported | Notes |
|----------|--------|-------------------|-------|
| ICPSwap | ✅ Complete | Swaps, Liquidity Pools, Price Feeds | Full integration with all trading pairs |
| KongSwap | 🔄 In Progress | Basic Swaps | Core functionality working, advanced features coming soon |
| Sonic | 🔄 In Progress | Price Feeds | Coming soon |
| ICDex | 🔍 Planned | - | Coming soon |

Legend:
- ✅ Complete: Fully integrated and tested
- 🔄 In Progress: Work underway, partially implemented
- 🔍 Planned: On roadmap but implementation not yet started

## 🏗️ System Architecture

### Architecture Diagram
```
┌───────────┐     ┌─────────────────┐
│  Factory  │◄────┤ User/Frontend   │
└─────┬─────┘     └─────────────────┘
      │
      │ creates/manages
      ▼
┌─────────────────────────────────────┐
│                                     │
│         Strategy Canisters          │
│                                     │
├─────────┬─────────┬─────────┬───────┤
│   DCA   │ Value   │ Fixed   │ Limit │
│         │ Avg     │ Balance │ Order │
└────┬────┴────┬────┴────┬────┴───┬───┘
     │         │         │        │
     │         │         │        │
     ▼         ▼         ▼        ▼
┌────────────────────────────────────┐
│        Exchange Interface          │
├────────────┬───────────────────────┤
│  ICPSwap   │       KongSwap        │
└────────────┴───────────────────────┘
      │                │
      ▼                ▼
┌────────────┐   ┌───────────┐
│  ICPSwap   │   │  KongSwap │
│  Canister  │   │  Canister │
└────────────┘   └───────────┘
```

HunterFi employs a modular design, primarily consisting of the following components:

### Core Components

#### Factory Canister (factory)

The Factory Canister is the central component of HunterFi that manages:
- User account balances and deposit/withdrawal processing
- Transaction history tracking and account management
- Strategy canister deployment and lifecycle
- WASM module installation for different strategy types
- Administrative functions and fee management

# HunterFi Factory Canister

The Factory Canister is the core component of the HunterFi platform, responsible for user account management, strategy deployment, and canister lifecycle management.

## Key Features

1. **Account Management System**
   - User balance tracking with stable memory
   - Secure deposit and withdrawal functionality
   - Complete transaction history with event logging
   - Principal-based account identification

2. **Strategy Deployment Engine**
   - Dynamic strategy creation and configuration
   - Automatic fee collection from user balances
   - Multi-phase deployment state management
   - Automatic refund handling for failed deployments

3. **Canister Management**
   - Dynamic canister creation and configuration
   - WASM module installation and version control
   - Strategy canister lifecycle monitoring
   - Inter-canister communication handling

4. **Administrative Capabilities**
   - Role-based permission system
   - Fee configuration management
   - Emergency failsafe mechanisms
   - System monitoring and maintenance features

## Stable Storage Architecture

The Factory Canister implements a robust stable storage architecture using `ic-stable-structures`:

```
┌─────────────────────────────────────────────────┐
│                 Stable Storage                   │
├─────────────┬──────────────┬───────────────────┤
│ UserAccounts│ Transactions │ DeploymentRecords │
├─────────────┼──────────────┼───────────────────┤
│ Strategies  │ AdminRoles   │ WasmModules       │
└─────────────┴──────────────┴───────────────────┘
```

- **Memory Optimization**: Efficient storage allocation with minimal memory overhead
- **Upgrade Safety**: Protected state during canister upgrades
- **Data Integrity**: Transaction-like safety for critical operations
- **Scalability**: Designed to handle thousands of user accounts and strategies

## Deployment Process

HunterFi implements a deposit-based deployment model with comprehensive state tracking:

### Account Balance System

1. **User Deposit Flow**
   ```
   User Request -> Ledger Transaction -> Balance Update -> Transaction Record
   ```
   - Minimum deposit: 0.01 ICP (1,000,000 e8s)
   - Maximum deposit: 1,000 ICP (100,000,000,000 e8s)
   - Transaction ID generation ensures idempotency

2. **Account Balance Management**
   - Real-time balance tracking with optimistic locking
   - Transaction history with pagination support
   - Balance reservation during pending deployments

### Strategy Deployment Flow

1. **Request and Payment Phase**
   ```
   Strategy Request -> Balance Verification -> Fee Deduction -> Status: PaymentReceived
   ```
   - Atomic fee deduction with rollback capability
   - Configurable fee structure based on strategy type
   - Detailed deployment record creation

2. **Deployment Execution Phase**
   ```
   Canister Creation -> WASM Installation -> Initialization -> Status: Deployed
   ```
   - Step-by-step state tracking with detailed logging
   - Automatic retry mechanism for transient failures
   - Controlled error propagation and handling

3. **Error Recovery**
   ```
   Failure Detection -> Automatic Refund -> Status: Refunded
   ```
   - Comprehensive error classification and handling
   - Full refund processing with transaction recording
   - Detailed failure reporting for troubleshooting

## State Management

The system employs a sophisticated state management system:

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ PendingState│────►│ ActiveState │────►│ FinalState  │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │
       ▼                   ▼                   ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ Validation  │     │ Processing  │     │ Completion  │
│ Functions   │     │ Functions   │     │ Functions   │
└─────────────┘     └─────────────┘     └─────────────┘
```

- **Timers**: Scheduled tasks run every 5 minutes
- **State Reconciliation**: Automatic recovery of stuck deployments
- **Timeout Handling**: Deployments older than 24 hours are automatically refunded

## Interface Specification

### Account Management
```rust
// Deposit ICP to your account
#[update]
deposit_icp(amount: u64) -> Result<u64, String>;

// Withdraw ICP from your account
#[update]
withdraw_user_icp(amount: u64) -> Result<u64, String>;

// Get account balance and details
#[query]
get_account_info() -> UserAccount;

// Get transaction history
#[query]
get_transaction_history() -> Vec<TransactionRecord>;
```

### Strategy Deployment
```rust
// Request DCA strategy deployment
#[update]
request_dca_strategy(config: DCAConfig) -> Result<DeploymentRequest, String>;

// Request Value Average strategy deployment
#[update]
request_value_avg_strategy(config: ValueAvgConfig) -> Result<DeploymentRequest, String>;

// Request Fixed Balance strategy deployment
#[update]
request_fixed_balance_strategy(config: FixedBalanceConfig) -> Result<DeploymentRequest, String>;

// Request Limit Order strategy deployment
#[update]
request_limit_order_strategy(config: LimitOrderConfig) -> Result<DeploymentRequest, String>;

// Request Self-Hedging strategy deployment
#[update]
request_self_hedging_strategy(config: SelfHedgingConfig) -> Result<DeploymentRequest, String>;

// Get deployment status
#[query]
get_deployment(deployment_id: String) -> Option<DeploymentRecord>;
```

### Strategy Management
```rust
// Get strategy information
#[query]
get_strategy(canister_id: Principal) -> Option<StrategyMetadata>;

// Get user's strategy list
#[query]
get_strategies_by_owner(owner: Principal) -> Vec<StrategyMetadata>;

// Get all strategies (admin only)
#[query]
get_all_strategies() -> Vec<StrategyMetadata>;

// Get strategy count
#[query]
get_strategy_count() -> u64;
```

### Admin Functions
```rust
// Set deployment fee
#[update(guard = "is_admin")]
set_deployment_fee(fee_e8s: u64) -> Result<(), String>;

// Get current deployment fee
#[query]
get_deployment_fee() -> u64;

// Add/remove admins
#[update(guard = "is_admin")]
add_admin(principal: Principal) -> Result<(), String>;
#[update(guard = "is_admin")]
remove_admin(principal: Principal) -> Result<(), String>;

// Restart system timers
#[update(guard = "is_admin")]
reset_system_timers() -> Result<(), String>;

// Admin withdrawal
#[update(guard = "is_admin")]
withdraw_icp(recipient: Principal, amount_e8s: u64) -> Result<(), String>;

// Adjust user balance
#[update(guard = "is_admin")]
adjust_balance(user: Principal, amount: u64, reason: String) -> Result<(), String>;
```

## Usage Examples

### 1. Account Management

```javascript
// Deposit ICP to account
const amount = 100_000_000n; // 1 ICP
const depositResult = await factory.deposit_icp(amount);
if ("Ok" in depositResult) {
  console.log(`Deposit successful, new balance: ${depositResult.Ok} e8s`);
} else {
  console.error(`Deposit failed: ${depositResult.Err}`);
}

// Withdraw ICP (must have sufficient balance)
const withdrawAmount = 50_000_000n; // 0.5 ICP
const withdrawResult = await factory.withdraw_user_icp(withdrawAmount);
if ("Ok" in withdrawResult) {
  console.log(`Withdrawal successful, new balance: ${withdrawResult.Ok}`);
} else {
  console.error(`Withdrawal failed: ${withdrawResult.Err}`);
}

// Get account information
const account = await factory.get_account_info();
console.log(`Account balance: ${account.balance} e8s`);
console.log(`Total deposited: ${account.total_deposited} e8s`);
console.log(`Total consumed: ${account.total_consumed} e8s`);

// Get transaction history
const transactions = await factory.get_transaction_history();
transactions.forEach(tx => {
  console.log(`Transaction ${tx.transaction_id}: ${tx.transaction_type} - ${tx.amount} e8s`);
});
```

### 2. Strategy Deployment

```javascript
// Deploy a DCA strategy
const dcaConfig = {
  exchange: { ICPSwap: null },
  base_token: { 
    canister_id: Principal.fromText("ryjl3-tyaaa-aaaaa-aaaba-cai"), 
    symbol: "ICP", 
    decimals: 8 
  },
  quote_token: { 
    canister_id: Principal.fromText("mxzaz-hqaaa-aaaar-qaada-cai"), 
    symbol: "USDC", 
    decimals: 6 
  },
  amount_per_execution: 10_000_000n, // 0.1 ICP per execution
  interval_secs: 86400n,             // Execute daily
  max_executions: 30n,               // Execute 30 times total
  slippage_tolerance: 0.5            // 0.5% slippage tolerance
};

const deploymentResult = await factory.request_dca_strategy(dcaConfig);
if ("Ok" in deploymentResult) {
  const deploymentRequest = deploymentResult.Ok;
  console.log(`Deployment requested: ${deploymentRequest.deployment_id}`);
  console.log(`Fee deducted: ${deploymentRequest.fee_amount} e8s`);
  
  // Check deployment status after a few seconds
  setTimeout(async () => {
    const statusResult = await factory.get_deployment(deploymentRequest.deployment_id);
    if (statusResult) {
      console.log(`Current status: ${statusResult.status}`);
      if (statusResult.status === "Deployed") {
        console.log(`Strategy canister: ${statusResult.canister_id}`);
      }
    }
  }, 5000);
} else {
  console.error(`Deployment failed: ${deploymentResult.Err}`);
}
```

## Error Handling System

HunterFi implements comprehensive error handling with detailed error types:

```rust
// Transaction Types
pub enum TransactionType {
    Deposit,
    DeploymentFee,
    Refund,
    AdminAdjustment,
    Withdrawal,
    Transfer,
}

// Deployment Status
pub enum DeploymentStatus {
    PendingPayment,
    AuthorizationConfirmed,
    PaymentReceived,
    CanisterCreated,
    CodeInstalled,
    Initialized,
    Deployed,
    DeploymentCancelled,
    DeploymentFailed,
    Refunding,
    Refunded,
}
```

### Recovery Mechanisms

1. **Automatic Retry System**
   - Failed WASM installations retry up to 3 times
   - Failed canister creations retry with exponential backoff
   - Failed refunds are retried on the next timer tick

2. **Manual Recovery Options**
   - Admin interface for manual deployment intervention
   - Stuck deployment resolution through admin console
   - Emergency fund recovery mechanisms

## Security Considerations

The Factory Canister incorporates multiple security mechanisms:

1. **Guard-based Access Control**
   - Role-based permission system for admin functions
   - Principal-based account access restrictions
   - Granular permission model for different operations

2. **Transaction Security**
   - Atomic operations with rollback capability
   - Idempotent transaction handling
   - Complete audit trail of all financial operations

3. **Input Validation**
   - Comprehensive parameter validation
   - Type-safe interfaces with Candid
   - Boundary checking for all numeric inputs

4. **Deployment Protection**
   - Fee verification before deployment
   - Resource allocation limits
   - Deployment timeout monitoring

## Project Structure

- `src/factory`: The main factory canister that manages strategy deployment and user accounts
- `src/strategy_common`: Common utilities and types shared across canisters
- `src/strategies/`: Individual strategy implementations (DCA, ValueAveraging, etc.)
- `src/exchange/`: Exchange connectors and adapters

