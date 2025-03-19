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
| InfinitySwap | 🔍 Planned | - | coming soon |
| ICDex | 🔍 Planned | - | coming soon |

Legend:
- ✅ Complete: Fully integrated and tested
- 🔄 In Progress: Work underway, partially implemented
- 🔍 Planned: On roadmap but implementation not yet started

## 🏗️ System Architecture

HunterFi employs a modular design, primarily consisting of the following components:

### Core Components

#### Factory Canister (factory)

As the platform entry point, it is responsible for:
- **Strategy Deployment Management**: Creates new strategy canisters
- **Deployment Fee Collection**: Charges 1 ICP per deployment as a platform fee
- **Strategy Registry Maintenance**: Records and indexes all deployed strategies

#### Strategy Common Library (strategy_common)

A core library providing shared functionality and type definitions for all strategies, containing four main modules:
- **types**: Defines shared data structures and enumerated types
- **timer**: Manages timed execution functions
- **cycles**: Provides cycles management functionality
- **exchange**: Defines exchange interfaces

#### Strategy Canisters

Each strategy is implemented as an independent canister with common features including:
- **Initialization and Configuration**
- **Start/Pause Execution**
- **Strategy Logic Execution**
- **Transaction History Recording**
- **Cycles Management**

### Strategy Types

#### Dollar Cost Averaging Strategy (strategy_dca)

Implements the dollar-cost averaging method, periodically investing a fixed amount.
- **Periodic Trades**: Executes at fixed time intervals
- **Fixed Investment Amount**: Uses a preset amount to purchase assets
- **User Controls**: Allows starting, pausing, and managing execution

#### Value Averaging Strategy (strategy_value_avg)

Periodic investment based on the target growth curve of account value.
- **Performance-Based Investment**: Determines investment amount based on actual account performance
- **Target Growth Curve**: Ensures account value grows according to a predetermined trajectory
- **Dynamic Adjustments**: Increases investment when below target, decreases when above target

#### Fixed Balance Strategy (strategy_fixed_balance)

Maintains a constant account balance through periodic fund allocation adjustments.
- **Regular Monitoring**: Periodically checks account balance
- **Automated Adjustments**: Executes buy or sell operations to adjust to target value
- **Stability-Oriented**: Suitable for investors seeking stability amid market fluctuations

#### Limit Order Strategy (strategy_limit_order)

Implements limit order functionality, allowing users to set specific prices for buying or selling assets.
- **Continuous Market Monitoring**: Monitors prices reaching preset conditions
- **Automated Execution**: Automatically trades when conditions are met
- **Multiple Condition Support**: Supports setting multiple buy/sell conditions

#### Self-Hedging Strategy (strategy_self_hedging)

System acts as both buyer and seller to increase trading volume for a specific asset.
- **Self-Trading Operations**: Executes synchronized buying and selling operations
- **Preset Frequencies and Amounts**: Operates at predetermined frequency and trade sizes
- **Configurable Parameters**: Users can configure trading frequency, quantity, and price range

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


