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
| Sonic | 🔄 In Progress | Price Feeds | coming soon |
| ICDex | 🔍 Planned | - | coming soon |

Legend:
- ✅ Complete: Fully integrated and tested
- 🔄 In Progress: Work underway, partially implemented
- 🔍 Planned: On roadmap but implementation not yet started

## 🏗️ System Architecture

# Architecture Diagram
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

HunterFi is a decentralized finance strategy platform that enables users to deploy and manage automated trading strategies on the Internet Computer. The Factory Canister is responsible for strategy creation, deployment, and management.

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
- `request_dca_strategy`: Request to deploy a Dollar Cost Averaging strategy
- `request_value_avg_strategy`: Request to deploy a Value Averaging strategy
- `request_fixed_balance_strategy`: Request to deploy a Fixed Balance strategy
- `request_limit_order_strategy`: Request to deploy a Limit Order strategy
- `request_self_hedging_strategy`: Request to deploy a Self-Hedging strategy

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

### 1. Deploying a DCA Strategy
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

## Strategy Types

### Dollar Cost Averaging Strategy (strategy_dca)
- Implements the dollar-cost averaging method, periodically investing a fixed amount
- Executes trades at fixed time intervals
- Uses a preset amount to purchase assets
- Allows starting, pausing, and managing execution

### Value Averaging Strategy (strategy_value_avg)
- Adjusts investment based on performance relative to a target growth curve
- Determines investment amount based on actual account performance
- Maintains account value growth according to a predetermined trajectory
- Dynamically increases investment when below target, decreases when above target

### Fixed Balance Strategy (strategy_fixed_balance)
- Maintains a constant account balance through periodic rebalancing
- Regularly monitors account balance
- Executes buy or sell operations to adjust to target value
- Suitable for investors seeking stability amid market fluctuations

### Limit Order Strategy (strategy_limit_order)
- Implements limit order functionality for specific price points
- Continuously monitors market prices
- Automatically executes trades when conditions are met
- Supports setting multiple buy/sell conditions

### Self-Hedging Strategy (strategy_self_hedging)
- Creates balanced buying and selling operations to increase trading volume
- Executes synchronized buying and selling transactions
- Operates at predetermined frequency and trade sizes
- Allows configuration of trading frequency, quantity, and price range

## 🔧 Technology Stack

### Backend
- **Rust**: Primary development language for implementing canister logic
- **Candid**: Interface Definition Language (IDL) for defining canister interfaces
- **ic-cdk**: Internet Computer Development Kit
- **ic-stable-structures**: Persistent storage management library

### Frontend
- **React**: User interface framework
- **TypeScript**: Type-safe JavaScript superset
- **Ant Design**: UI component library
- **dfx**: DFINITY Canister SDK
- **Internet Identity**: Authentication

## 🚀 Quick Start

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) >= 1.60.0
- [DFX](https://internetcomputer.org/docs/current/developer-docs/setup/install) >= 0.14.0
- [Node.js](https://nodejs.org/) >= 16.x (required for frontend development)

### Local Development

1. **Clone Repository and Install Dependencies**

```bash
git clone https://github.com/yourusername/hunterfi.git
cd hunterfi
npm install  # Install frontend dependencies
```

2. **Start Local Internet Computer Network**

```bash
dfx start --background --clean
```

3. **Deploy Canisters**

```bash
dfx deploy
```

4. **Initialize Factory Canister**

```bash
# Get Factory canister ID
FACTORY_ID=$(dfx canister id factory)

# Deploy DCA strategy WASM
dfx canister call factory install_strategy_wasm '(record { strategy_type = variant { DollarCostAveraging }; wasm_module = blob })'
```

5. **Start Frontend Development Server**

```bash
npm start
```

The application will be available at `http://localhost:8080`, with API requests proxied to the replica at port 4943.

## 📝 Usage Guide

### Deploying a New Strategy

1. Connect with Internet Identity to log in to the platform
2. Navigate to the "Deploy Strategy" page
3. Select strategy type (DCA, Value Averaging, etc.)
4. Configure strategy parameters:
   - Trading pair
   - Exchange
   - Investment amount
   - Execution frequency
   - Maximum slippage
5. Confirm and deploy (1 ICP deployment fee will be charged)

### Managing Strategies

1. View all deployed strategies on the "My Strategies" page
2. Click on strategy cards to view details
3. Use control options:
   - Start/Pause strategy
   - Execute manually once
   - View historical transaction records
   - Modify strategy parameters

## 🔒 Security Considerations

- **Fund Security**: User funds are always under user control, the platform does not hold user assets
- **Code Audit**: All strategy code is open-source and transparent, fully auditable
- **Error Handling**: The system is designed with comprehensive error handling mechanisms to ensure stable execution
- **Slippage Protection**: Trade execution includes slippage protection mechanisms to prevent losses from price volatility

## 🌐 Mainnet Deployment

Deploying to ICP mainnet is similar to local deployment, but requires:

1. Set mainnet identity: `dfx identity use <your_identity>`
2. Ensure sufficient cycles for deployment
3. Deploy to mainnet: `dfx deploy --network ic`

## 🛠️ Developer Resources

- [Internet Computer Documentation](https://internetcomputer.org/docs/)
- [Rust Canister Development Guide](https://internetcomputer.org/docs/current/developer-docs/backend/rust/)
- [ic-cdk Documentation](https://docs.rs/ic-cdk)
- [ic-cdk-macros Documentation](https://docs.rs/ic-cdk-macros)
- [Candid Introduction](https://internetcomputer.org/docs/current/developer-docs/backend/candid/)

## 🤝 Contribution Guidelines

Code contributions, issue reporting, and improvement suggestions are welcome. Please follow these steps:

1. Fork this repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Commit changes: `git commit -m 'Add some amazing feature'`
4. Push to the branch: `git push origin feature/amazing-feature`
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details


