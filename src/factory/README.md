# Factory Canister

HunterFi Factory Canister is responsible for the deployment and management of automated trading strategies on the Internet Computer. It uses ICRC2 token standard for secure and transparent payment processing.

## Table of Contents
- [Key Features](#key-features)
- [Supported Strategy Types](#supported-strategy-types)
- [Deployment Process](#deployment-process)
- [Main Interfaces](#main-interfaces)
- [Usage Example](#usage-example)
- [Security Features](#security-features)
- [Recent Improvements](#recent-improvements)

## Key Features

1. **ICRC2-Based Payment System**
   - Uses ICRC2 token standard for authorization and payment
   - No pre-deposit required, users only authorize fee collection
   - Full transparency of fee collection process
   - Automatic refunds for failed deployments

2. **Strategy Deployment**
   - Support for five strategy types (DCA, Value Averaging, Fixed Balance, Limit Order, Self Hedging)
   - Individual deployment via dedicated endpoints
   - Comprehensive deployment status tracking

3. **Admin Management**
   - Admin roles for governance functionality
   - Admin-controlled fee management
   - WASM module management for strategy code updates
   - Fund withdrawal capability for admin users

## Supported Strategy Types

- **Dollar Cost Averaging**: Periodic investment with fixed amount
- **Value Averaging**: Adjusts investment amount based on performance vs target
- **Fixed Balance**: Maintains balance through periodic rebalancing
- **Limit Order**: Executes trades at specified price points
- **Self-Hedging**: Creates balanced buying and selling operations

## Deployment Process

HunterFi implements a two-phase deployment process based on ICRC2 standard to address the non-atomic nature of ICP transactions:

### Phase One: Deployment Preparation

1. **Create Deployment Request**
   - User submits strategy type and configuration
   - System generates a unique deployment ID and returns fee information
   - Status is marked as `PendingPayment`

2. **User Payment Authorization**
   - User calls `icrc2_approve` through their wallet to authorize the Factory Canister to use a specified amount of ICP
   - User confirms deployment intent by submitting the deployment_id
   - System verifies the authorization amount is sufficient
   - Status is updated to `AuthorizationConfirmed`

### Phase Two: Deployment Execution

1. **Fee Collection and Canister Creation**
   - System calls `icrc2_transfer_from` to collect the fee
   - Creates new Canister and sets controller permissions
   - Installs the appropriate WASM module for the strategy type
   - Status progresses through `PaymentReceived` -> `CanisterCreated` -> `CodeInstalled`

2. **Initialization and Completion**
   - Initializes the strategy with user-provided configuration
   - Records strategy metadata
   - Status progresses to `Initialized` -> `Deployed`

3. **Error Handling**
   - If deployment fails, status is set to `DeploymentFailed`
   - Refund process is initiated, status updates to `Refunding` -> `Refunded`

## State Management

The system monitors deployment states through scheduled tasks, handling:
- Timed-out deployment requests
- Post-payment incomplete deployments
- Failed deployment refunds
- Refund retries

## Main Interfaces

### Deployment Request Interfaces
- `request_dca_strategy`: Deploy a Dollar Cost Averaging strategy
- `request_value_avg_strategy`: Deploy a Value Averaging strategy
- `request_fixed_balance_strategy`: Deploy a Fixed Balance strategy
- `request_limit_order_strategy`: Deploy a Limit Order strategy
- `request_self_hedging_strategy`: Deploy a Self-Hedging strategy

### Deployment Confirmation and Management
- `confirm_deployment`: Confirm authorization and execute deployment
- `get_deployment`: Retrieve deployment record
- `get_my_deployment_records`: Get user's deployment records
- `request_refund`: Request a refund

### Strategy Management
- `get_strategy`: Get strategy information
- `get_strategies_by_owner`: Get user's strategy list
- `get_all_strategies`: Get all strategies

### Admin Functions
- `set_deployment_fee`: Set deployment fee
- `install_strategy_wasm`: Install strategy WASM module
- `add_admin`: Add an admin
- `remove_admin`: Remove an admin
- `restart_timers`: Restart scheduled tasks
- `withdraw_icp`: Withdraw ICP from the canister

## Usage Example

### Deploying a DCA Strategy
```javascript
// 1. Create deployment request
const deploymentRequest = await factory.request_dca_strategy({
  exchange: { ICPSwap: null },
  base_token: { canister_id: Principal.fromText("..."), symbol: "ICP", decimals: 8 },
  quote_token: { canister_id: Principal.fromText("..."), symbol: "USDC", decimals: 6 },
  amount_per_execution: 10_000_000n, // 0.1 ICP
  interval_secs: 86400n, // Execute daily
  max_executions: [30n], // Execute 30 times
  slippage_tolerance: 0.5 // 0.5% slippage tolerance
});

// 2. Authorize Factory to use ICP
await icpLedger.icrc2_approve({
  spender: { owner: factoryCanisterId },
  amount: deploymentRequest.fee_amount,
  expires_at: [] // No expiration
});

// 3. Confirm deployment
await factory.confirm_deployment(deploymentRequest.deployment_id);

// 4. Query deployment status
const status = await factory.get_deployment(deploymentRequest.deployment_id);
```

## Security Features

1. **State Tracking**: Complete deployment state tracking for transparency
2. **Scheduled Monitoring**: Automatic handling of deployments stuck in intermediate states
3. **Refund Mechanism**: Automatic refund process for failed deployments
4. **Permission Control**: Strict admin permissions system
5. **Unique IDs**: Each deployment request has a unique ID to prevent duplicate processing

## Recent Improvements

1. **ICRC2 Payment Integration**
   - Implemented two-phase deployment process using ICRC2 standard
   - Eliminated need for pre-deposits, improving user experience
   - Enhanced security with explicit user authorization
   - Added comprehensive deployment state tracking

2. **Deployment Status System**
   - Implemented fine-grained deployment status tracking
   - Added automatic failure detection and refund processing
   - Created scheduled tasks for monitoring deployment states
   - Improved error handling with detailed status information

3. **Security Enhancements**
   - Added unique deployment IDs to prevent duplicate processing
   - Implemented strict verification of payment authorization
   - Enhanced refund mechanism with automatic retries
   - Improved admin permission system

4. **Overall Code Quality**
   - Enhanced error handling with detailed error messages
   - Improved documentation and type definitions
   - Optimized stable storage usage
   - Added comprehensive canister upgrade support