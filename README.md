
# BSC/ETH MEV Bot
Hades is a sophisticated MEV (Maximal Extractable Value) bot designed for the Binance Smart Chain (BSC/ETH). It focuses on capturing value through various strategies including arbitrage, sandwiching, and backrunning.

## What is missing 
Legend arbitrage bot will search shortest and the most profitable arbitrage cycle. It means millions^3 or more. In worse case, Billion is a second in Rust. We must calculate that value, optimized and optimized value. To success, can't be more, can't be less. So, need high and a lot of filter works, more accurate analysis

We will do Sandwich + Arbitrage in one transaction, if sandwich fails, then just arb. As for the arbitrage, we don't need much initial funds because we can use Flashloan or Flashswap.

1. Add flash loan integration to the code
2. Make sure relayer works properly
3. Backtesting and fine-tune the bot
4. Finish the smart contract development
5. Optimize the searcher, and calculate profit 


## Features
Multiple MEV Strategies:
- Arbitrage detection and execution
- Sandwich attacks
- Backrunning transactions
- Next block transaction bundling

Builder Integrations:
- 48.club
- Bloxroute
- Blackrazor
- TxBoost
- ArbOnly specialized builder
- Need more()

Pool Support:
- Uniswap V2/V3 compatible pools
- DoDo pools
- 1inch limit orders
- 0x protocol orders

Advanced Architecture:
- Real-time mempool monitoring
- Graph-based arbitrage path finding (Improving...)
- EVM simulation for profit calculation (important)
- Multi-threaded transaction processing (YES!!)

## Setup
Prerequisites
- Rust toolchain (2021 edition)
- Access to a BSC node (IPC, HTTP, and WebSocket endpoints)
- Private key for transaction signing
- Discord webhooks for notifications (optional)

## Environment Variables
Create a .env file with the following variables:

```
IPC_NODE_URL=path/to/bsc.ipc / eth
HTTP_NODE_URL=<local bsc/eth node>
WS_NODE_URL=<local bsc/eth node>
POOLS_DATA_DIR=./data/pools
DB_PATH_DIR=./data/db
WALLET_SIGNER=your_private_key


# Feature flags
ARBONLY_ENABLED=true
FAST_MEMPOOL_ENABLED=true
MERKLE_ENABLED=true
PUBLIC_MEMPOOL_ENABLED=true
NEXTBLOCK_ENABLED=true
SANDWICH_ENABLED=true
ARBITRAGE_ENABLED=true
```

## Architecture

### Core Components

Pool Management (src/pools.rs):
Handles different pool types (V2, V3, DoDo, 1inch, 0x)
Tracks reserves and calculates rates
Graph System (src/graph.rs):
Builds a directed graph of token pairs
Finds profitable arbitrage paths
Search Algorithms (src/search.rs):
- Implements sandwich attack detection
- Calculates optimal trade amounts

Transaction Flow (src/flow.rs):
- Monitors mempool for opportunities
- Connects to Bloxroute and Merkle for private transactions

Bundle Builders (src/builders.rs):
- Constructs and submits transaction bundles to various builders
- Handles backrunning and arbitrage execution (IMPORTANT)

### Execution Flow
The bot initializes by loading pool data and building the token graph
It connects to mempool sources to monitor transactions
When opportunities are detected, it simulates the transactions to calculate profit
If profitable, it constructs and submits bundles to appropriate builders

The bot will:
- Connect to the BSC/ETH node
- Load pool data and build the token graph
- Start monitoring for MEV opportunities
- Send notifications to Discord when events occur
- Execute profitable transactions automatically
