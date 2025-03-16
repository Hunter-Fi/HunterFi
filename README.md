# `HuntersFi`

## Factory Canister (factory)

The Factory Canister is the entry point for the entire quantitative trading system, developed using Rust. It is responsible for:

- **Managing strategy deployments**: Deploys new trading strategies.
- **Collecting deployment fees**: Charges a fee of 1 ICP per deployment.
- **Maintaining a registry**: Keeps a record of all deployed strategies.

**User Interactions:**

- **Deploy new trading strategies**: Users can deploy new strategies.
- **View owned strategies**: Users can check the list of strategies they have deployed.
- **Retrieve strategy details**: Detailed information about each strategy is available.

When a user deploys a new strategy, the Factory Canister creates a new Strategy Canister and installs the corresponding WASM code based on the strategy type chosen by the user.

---

## Strategy Common Library (strategy_common)

The Strategy Common Library is a shared library that provides common functionalities and type definitions for all strategy implementations, ensuring consistency and maintainability across the system. It consists of four main modules:

### 1. types

Defines all shared data structures and enumerated types for strategies, such as:
- `StrategyType`
- `StrategyStatus`
- `Transaction`

### 2. timer

Manages timed execution functions, allowing strategies to:
- Set up scheduled tasks.
- Cancel scheduled tasks.

### 3. cycles

Provides cycles management functionality, enabling strategies to:
- Check available cycles.
- Top up cycles to ensure continuous operation.

### 4. exchange

Defines exchange interfaces and includes mock implementations to offer:
- A unified method for interacting with exchanges.

---

## Dollar Cost Averaging Strategy (strategy_dca)

The DCA Strategy Canister implements the dollar-cost averaging method, a strategy that involves continuously investing a fixed amount at regular time intervals.

**Key Features:**

- **Periodic Trades**: Executes trades at fixed intervals to purchase a specific crypto asset.
- **Fixed Investment Amount**: Buys the asset with a predetermined amount of quote currency, regardless of market price fluctuations.
- **User Controls**: Allows users to start, pause, and manage strategy execution.
- **History Tracking**: Records the history of executed trades.

The core logic is to convert a fixed amount of quote currency to the base token without consideration for price fluctuations.

---

## Value Averaging Strategy (strategy_value_avg)

The Value Averaging Strategy Canister implements a value averaging strategy, which involves periodic investments based on a predetermined target growth path for the account's value.

**Key Features:**

- **Performance-Based Investment**: Determines the investment amount for each period based on the actual performance of the account.
- **Target Growth Path**: Ensures that the account value grows according to a pre-planned curve.
- **Dynamic Adjustments**: Increases investment if the portfolio value is below target or reduces investment (or sells assets) if above target.

---

## Fixed Balance Strategy (strategy_fixed_balance)

The Fixed Balance Strategy Canister maintains a constant account balance or total asset value by periodically adjusting fund allocations.

**Key Features:**

- **Regular Monitoring**: Routinely checks the account balance.
- **Automated Adjustments**: Executes buy or sell operations to adjust the balance to a preset target value.
- **Stability Focus**: Suitable for investors who want to maintain stability amidst market fluctuations.

---

## Limit Orders Strategy (strategy_limit_order)

The Limit Orders Strategy Canister implements a limit order strategy, enabling users to set specific prices for buying or selling assets.

**Key Features:**

- **Continuous Market Monitoring**: Monitors market prices to detect when they reach preset conditions.
- **Automated Execution**: Automatically executes trades once the conditions are met.
- **Multiple Conditions**: Supports setting multiple buying or selling conditions, each with a target price and quantity.

---

## Wash Trading Strategy (strategy_wash_trading)

The Wash Trading Strategy Canister implements a wash trading strategy, where the system acts as both buyer and seller to increase the trading volume of a specific asset.

**Key Features:**

- **Self-Trading Operations**: Executes simultaneous buying and selling operations.
- **Preset Frequencies and Amounts**: Operates at predetermined frequencies and trade sizes.
- **Use Cases**: Useful for testing market liquidity or analyzing trading volumes.
- **Configurable Parameters**: Users can configure trading frequency, quantity, and price range for automatic executions.

---

## Common Interface for Strategy Canisters

Each Strategy Canister implements a basic set of functionalities, including:

- **Initialization**
- **Start/Pause Execution**
- **Executing Strategy Logic**
- **Recording Execution History**
- **Managing Cycles**

Additionally, each canister incorporates unique trading logic tailored to its specific strategy type.

---


Welcome to your new `HuntersFi` project and to the Internet Computer development community. By default, creating a new project adds this README and some template files to your project directory. You can edit these template files to customize your project and to include your own code to speed up the development cycle.

To get started, you might want to explore the project directory structure and the default configuration file. Working with this project in your development environment will not affect any production deployment or identity tokens.

To learn more before you start working with `HuntersFi`, see the following documentation available online:

- [Quick Start](https://internetcomputer.org/docs/current/developer-docs/setup/deploy-locally)
- [SDK Developer Tools](https://internetcomputer.org/docs/current/developer-docs/setup/install)
- [Rust Canister Development Guide](https://internetcomputer.org/docs/current/developer-docs/backend/rust/)
- [ic-cdk](https://docs.rs/ic-cdk)
- [ic-cdk-macros](https://docs.rs/ic-cdk-macros)
- [Candid Introduction](https://internetcomputer.org/docs/current/developer-docs/backend/candid/)

If you want to start working on your project right away, you might want to try the following commands:

```bash
cd hunters_finance/
dfx help
dfx canister --help
```

## Running the project locally

If you want to test your project locally, you can use the following commands:

```bash
# Starts the replica, running in the background
dfx start --background

# Deploys your canisters to the replica and generates your candid interface
dfx deploy
```

Once the job completes, your application will be available at `http://localhost:4943?canisterId={asset_canister_id}`.

If you have made changes to your backend canister, you can generate a new candid interface with

```bash
npm run generate
```

at any time. This is recommended before starting the frontend development server, and will be run automatically any time you run `dfx deploy`.

If you are making frontend changes, you can start a development server with

```bash
npm start
```

Which will start a server at `http://localhost:8080`, proxying API requests to the replica at port 4943.

### Note on frontend environment variables

If you are hosting frontend code somewhere without using DFX, you may need to make one of the following adjustments to ensure your project does not fetch the root key in production:

- set`DFX_NETWORK` to `ic` if you are using Webpack
- use your own preferred method to replace `process.env.DFX_NETWORK` in the autogenerated declarations
  - Setting `canisters -> {asset_canister_id} -> declarations -> env_override to a string` in `dfx.json` will replace `process.env.DFX_NETWORK` with the string in the autogenerated declarations
- Write your own `createActor` constructor


