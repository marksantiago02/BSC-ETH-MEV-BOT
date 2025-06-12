# BSC/ETH MEV Bot

A high-performance MEV (Maximal Extractable Value) bot engineered for Ethereum and Binance Smart Chain networks. This sophisticated trading system captures value through advanced arbitrage detection, sandwich attacks, and transaction backrunning strategies.

## Overview

This MEV bot implements cutting-edge algorithms to identify and execute profitable opportunities across multiple DEXs and protocols. Built with Rust for maximum performance, it processes millions of calculations per second to find optimal arbitrage cycles and execute complex trading strategies.

### Key Capabilities

- **Multi-Strategy Execution**: Combines sandwich attacks with arbitrage in single transactions, with fallback mechanisms
- **Flash Loan Integration**: Minimizes capital requirements through flash loans and flash swaps
- **Real-time Analysis**: Processes mempool transactions with microsecond precision
- **Advanced Path Finding**: Graph-based algorithms discover optimal multi-hop arbitrage routes

## Features

### MEV Strategies
- **Arbitrage Detection**: Graph-based shortest path algorithms for maximum profitability
- **Sandwich Attacks**: Front-run and back-run large transactions for guaranteed profits  
- **Transaction Backrunning**: Capitalize on state changes from pending transactions
- **Bundle Optimization**: Next-block transaction bundling for MEV extraction

### Builder Integrations
- **48.club** - High-performance block building
- **Bloxroute** - Private mempool access and bundle submission
- **Blackrazor** - Specialized MEV infrastructure
- **TxBoost** - Transaction acceleration services
- **ArbOnly** - Arbitrage-focused builder
- *Additional builders in development*

### Protocol Support
- **Uniswap V2/V3** - Complete AMM integration with liquidity tracking
- **DoDo** - Proactive Market Maker protocol support
- **1inch Limit Orders** - Off-chain order book integration
- **0x Protocol** - Decentralized exchange infrastructure

### Technical Architecture
- **Real-time Mempool Monitoring** - Multi-source transaction stream processing
- **Graph-based Path Finding** - Optimized algorithms for arbitrage route discovery
- **EVM Simulation Engine** - Accurate profit calculation and risk assessment
- **Multi-threaded Processing** - Concurrent transaction analysis and execution
- **State Management** - Advanced caching and synchronization mechanisms

## Installation & Setup

### Prerequisites
- **Rust Toolchain** (2021 edition or later)
- **Node Access** - BSC/ETH node with IPC, HTTP, and WebSocket endpoints
- **Private Key** - For transaction signing and execution
- **Discord Webhook** (optional) - For monitoring and notifications

### Environment Configuration

Create a `.env` file in the project root:

```bash
# Node Connections
IPC_NODE_URL=path/to/node.ipc
HTTP_NODE_URL=http://localhost:8545
WS_NODE_URL=ws://localhost:8546

# Data Storage
POOLS_DATA_DIR=./data/pools
DB_PATH_DIR=./data/db

# Wallet Configuration
WALLET_SIGNER=your_private_key_here

# Strategy Toggles
ARBONLY_ENABLED=true
FAST_MEMPOOL_ENABLED=true
MERKLE_ENABLED=true
PUBLIC_MEMPOOL_ENABLED=true
NEXTBLOCK_ENABLED=true
SANDWICH_ENABLED=true
ARBITRAGE_ENABLED=true

# Optional: Discord Notifications
DISCORD_WEBHOOK_URL=your_discord_webhook_url
```

### Build and Run

```bash
cargo build --release
```

```bash
cargo run --release
```

## Architecture

### Core Components

#### Pool Management (`src/pools.rs`)
- Multi-protocol pool abstraction (V2, V3, DoDo, 1inch, 0x)
- Real-time reserve tracking and rate calculations
- Efficient state synchronization with on-chain data

#### Graph System (`src/graph.rs`)
- Directed graph representation of token pairs
- Advanced pathfinding algorithms for arbitrage discovery
- Dynamic edge weight updates based on liquidity changes

#### Search Engine (`src/search.rs`)
- Sandwich opportunity detection and sizing
- Optimal trade amount calculations
- Multi-hop arbitrage route evaluation

#### Transaction Flow (`src/flow.rs`)
- Mempool monitoring and filtering
- Private transaction pool integration
- Bundle construction and submission

#### Builder Integration (`src/builders.rs`)
- Multi-builder transaction submission
- Backrun and arbitrage execution logic
- Gas optimization and profit maximization

### Execution Workflow

1. **Initialization**: Load historical pool data and construct token relationship graph
2. **Monitoring**: Connect to multiple mempool sources for comprehensive transaction visibility
3. **Analysis**: Real-time opportunity detection using advanced algorithms
4. **Simulation**: EVM-based profit calculation and risk assessment
5. **Execution**: Automated bundle construction and submission to optimal builders
6. **Monitoring**: Performance tracking and Discord notifications

## Development Roadmap

### Phase 1: Core Infrastructure ✅
- [x] Multi-protocol pool support
- [x] Graph-based arbitrage detection
- [x] Basic MEV strategy implementation

### Phase 2: Advanced Features 🚧
- [ ] Flash loan integration for capital efficiency
- [ ] Relayer optimization and reliability improvements
- [ ] Comprehensive backtesting framework
- [ ] Smart contract development completion

### Phase 3: Optimization 📋
- [ ] Advanced searcher algorithms
- [ ] Profit calculation refinements
- [ ] Performance benchmarking and tuning
- [ ] Additional builder integrations

## Performance Metrics

- **Calculation Speed**: Billions of operations per second in Rust
- **Latency**: Sub-millisecond opportunity detection
- **Accuracy**: Advanced filtering for high-precision analysis
- **Scalability**: Multi-threaded architecture for concurrent processing

## Risk Management

- **Simulation-First**: All transactions simulated before execution
- **Slippage Protection**: Dynamic slippage calculations
- **Gas Optimization**: Intelligent gas pricing strategies
- **Fallback Mechanisms**: Graceful degradation on strategy failures

## Monitoring & Analytics

- **Real-time Dashboards**: Performance and profit tracking
- **Discord Integration**: Instant notifications for significant events
- **Historical Analysis**: Comprehensive backtesting capabilities
- **Risk Metrics**: Continuous monitoring of exposure and performance

---

## Contributing

This project follows standard Rust development practices. Please ensure all code passes `cargo clippy` and `cargo test` before submitting pull requests.

---

## 📜 License

This project is licensed under the [MIT License](./LICENSE).

---

## 📞 Contact Information

- **Email:** marksantiago0929@gmail.com  
- **Telegram:** [@marksantiago02](https://t.me/marksantiago02)  
- **Discord:** @marksantiago02  
- **Twitter:** [@MarkSantiago02](https://twitter.com/MarkSantiago02)  
- **Instagram:** [@marksantiago_0929](https://www.instagram.com/marksantiago_0929/)  

---

## Disclaimer

This software is for educational and research purposes. Users are responsible for compliance with applicable laws and regulations. Trading cryptocurrencies involves substantial risk of loss.
