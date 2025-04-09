use alloy::{
    eips::eip2718::Encodable2718,
    network::{EthereumWallet, TransactionBuilder},
    primitives::utils::parse_ether,
    rpc::types::Transaction,
    signers::k256::pkcs8::der::DateTime,
    sol_types::SolCall,
};
use reth::primitives::format_ether;
use reth_db::{cursor::DbCursorRO, tables, Database, DatabaseEnv};
use revm::{
    db::State,
    inspector_handle_register,
    primitives::{AccessList, AccountInfo, ExecutionResult, ResultAndState, TransactTo},
    Evm,
};
use revm_inspectors::access_list::AccessListInspector;
use std::time::SystemTime;
use tokio::sync::broadcast::Sender as Bsender;
use tokio::time::Instant;

use crate::*;

pub type BundleSender = Option<(Address, TransactionRequest, Vec<Arc<String>>, u128)>;
pub type QouteSearchResult =
    Result<(Vec<U256>, U256, u64, Vec<u8>, AccessList), Box<dyn std::error::Error>>;

// invoked if ARBITRAGE_ENABLED == true
pub fn quote_search(
    swaps: &[DirectedGraphEdge],
    transactions: &[Arc<Transaction>],
    db: Arc<DatabaseEnv>,
) -> QouteSearchResult {
    let now = Instant::now();

    // Set up the DB
    let (db_latest_block, _) = db
        .clone()
        .tx()?
        .new_cursor::<tables::BlockBodyIndices>()?
        .last()?
        .ok_or("Failed to get last block DB")?;
    let db = LatestState.read().unwrap().clone();
    let mut cache_db = State::builder().with_database_ref(db).build();
    cache_db.insert_account(
        SEARCHER_ADDRESS,
        AccountInfo {
            balance: parse_ether("100")?,
            nonce: 0,
            code_hash: SEARCHER_HASH,
            code: Some(SEARCHER_BYTE_CODE.clone()),
        },
    );

    // Set up the EVM
    let mut evm = Evm::builder()
        .with_db(cache_db)
        .with_spec_id(revm::primitives::SpecId::CANCUN)
        .modify_cfg_env(|c| {
            c.chain_id = CHAIN_ID;
            c.disable_base_fee = true;
            c.disable_balance_check = true;
        })
        .modify_block_env(|b| {
            b.basefee = U256::ZERO;
            b.number = U256::from(db_latest_block);
        })
        .build();

    // Set up the targets
    for transaction in transactions {
        let victim = evm.tx_mut();
        victim.transact_to = TransactTo::Call(transaction.to().unwrap());
        victim.caller = transaction.from();
        victim.value = transaction.value();
        victim.data = transaction.input().clone();

        match evm.transact_commit()? {
            ExecutionResult::Success { .. } => {}
            _ => return Err("Setting target into Env failed!".into()),
        }
    }

    // Set up the Searcher
    {
        let tx = evm.tx_mut();
        tx.caller = WALLET_SIGNER.address();
        tx.transact_to = TransactTo::Call(SEARCHER_ADDRESS);
        tx.value = U256::ZERO;
        tx.data = Searcher::searchAmountsOutCall {
            min_size: parse_ether("0.000001").unwrap(),
            max_size: parse_ether("1.60").unwrap(),
            iterations: U256::from(30),
            accuracy: U256::from(10000000000000u128),
            swaps: swaps.iter().map(|x| x.to_swap()).collect_vec(),
        }
        .abi_encode()
        .into();
    }

    let ResultAndState { result, .. } = evm.transact()?;

    let values = match result {
        ExecutionResult::Success { output, .. } => {
            Searcher::searchAmountsOutCall::abi_decode_returns(output.data(), false)?._0
        }
        _ => return Err("No Arb Found".into()),
    };

    if let (Some(first), Some(last)) = (values.first(), values.last()) {
        let _profit = match last.checked_sub(*first) {
            Some(U256::ZERO) | None => return Err("No Profit Found".into()),
            Some(num) => num,
        };
    } else {
        return Err("No Profit".into());
    };

    // Final confirmation
    let hswaps = swaps
        .iter()
        .enumerate()
        .map(|(i, x)| x.to_swap_mev(values[i * 2], values[i * 2 + 1]))
        .collect_vec();

    // Update the EVM
    let mut evm = evm
        .modify()
        .modify_tx_env(|tx| {
            tx.transact_to = TransactTo::Call(MEV_ADDRESS);
            tx.data = Rustitrage::swapCall {
                miner: address!("4848489f0b2bedd788c696e2d79b6b69d7484848"),
                finalBalance: U256::ZERO,
                swaps: hswaps.clone(),
            }
            .abi_encode()
            .into();
        })
        .reset_handler_with_external_context(AccessListInspector::new(
            Default::default(),
            WALLET_SIGNER.address(),
            MEV_ADDRESS,
            Vec::default(),
        ))
        .append_handler_register(inspector_handle_register)
        .build();

    // Execute the transaction with the real contract
    if let ResultAndState {
        result: ExecutionResult::Success { gas_used, .. },
        state,
    } = evm.transact()?
    {
        // Check the profit through the state
        if let Some(storage) = state
            .get(&WETH_ADDRESS)
            .unwrap_or_else(|| {
                error!("WETH Searcher Balances not found");
                panic!("WETH Searcher Balances not found");
            })
            .storage
            .get(&U256::from_str(MEV_SLOT_WETH)?)
        {
            // Calculate the profit based on the state
            let profit = storage
                .present_value()
                .checked_sub(storage.original_value() + U256::from(1))
                .ok_or("No HUff profit ending")?;

            trace!(time=?now.elapsed(), profit=?format_ether(profit), ?values, ?swaps, "Search Took");
            // NextBlocks.lock().unwrap().insert(profit, hswaps.clone());
            let calldata = Rustitrage::swapCall {
                miner: address!("4848489f0b2bedd788c696e2d79b6b69d7484848"),
                finalBalance: storage.present_value(),
                swaps: hswaps,
            }
            .abi_encode();
            return Ok((
                values,
                profit,
                gas_used,
                calldata,
                evm.context.external.into_access_list(),
            ));
        };
    }

    error!("Huff Profit failed");
    Err("Huff Profit failed".into())
}

// invoked if ARBITRAGE_ENABLED == true
pub fn searcher_handler(
    swaps: Vec<DirectedGraphEdge>,
    transactions: Vec<Arc<Transaction>>,
    db: Arc<DatabaseEnv>,
    raw: Vec<Arc<String>>,
    bundler: Bsender<BundleSender>,
    tracker: Address,
) {
    match quote_search(&swaps[..], &transactions[..], db) {
        Ok((values, profit, gas_used, calldata, access_list)) => {
            let nonce = loop {
                if let Ok(Some(number)) = NONCE_MANAGER.read().as_deref() {
                    break *number;
                }
            };

            let mut gas_price = 1_000_000_000;

            let bribe = match profit.checked_sub(U256::from(gas_used * gas_price)) {
                Some(U256::ZERO) | None => {
                    gas_price = 1;
                    match profit.checked_sub(U256::from(gas_used * gas_price)) {
                        Some(U256::ZERO) | None => return,
                        Some(value) => value * U256::from(90) / U256::from(100),
                    }
                }
                Some(value) => value * U256::from(90) / U256::from(100),
            };

            warn!(
                ?tracker,
                profit=?format_ether(profit),
                ?gas_used,
                ?values,
                ?raw,
                "Potential ARB"
            );
            let tx = TransactionRequest::default()
                .with_from(WALLET_SIGNER.address())
                .with_to(MEV_ADDRESS)
                .with_chain_id(CHAIN_ID)
                .with_nonce(nonce)
                .with_gas_limit((gas_used * 10 / 5).into())
                .with_input(calldata)
                .with_access_list(access_list)
                .with_value(bribe)
                .with_gas_price(gas_price.into());

            bundler.send(Some((tracker, tx, raw, profit.to()))).unwrap();
        }
        Err(_error) => {}
    }
}

// invoked if SANDWICH_ENABLED == true
pub async fn sandwich_bundler(
    tracker: Address,
    frontrun: SandwichTransaction,
    backrun: SandwichTransaction,
    profit: U256,
    mut raw: Vec<Arc<String>>,
    sender: Bsender<BundleSender>,
) {
    let nonce = loop {
        if let Ok(Some(number)) = NONCE_MANAGER.read().as_deref() {
            break *number;
        }
    };

    let mut gas_price = 1_000_000_000;

    let bribe = match profit.checked_sub(U256::from((frontrun.0 + backrun.0) * gas_price)) {
        Some(U256::ZERO) | None => {
            gas_price = 1;
            match profit.checked_sub(U256::from((frontrun.0 + backrun.0) * gas_price)) {
                Some(U256::ZERO) | None => return,
                Some(value) => value * U256::from(25) / U256::from(100),
            }
        }
        Some(value) => value * U256::from(95) / U256::from(100),
    };

    let frontrun_tx = TransactionRequest::default()
        .with_from(WALLET_SIGNER.address())
        .with_to(MEV_ADDRESS)
        .with_chain_id(CHAIN_ID)
        .with_nonce(nonce)
        .with_gas_limit((frontrun.0 * 10 / 5).into())
        .with_access_list(frontrun.1)
        .with_input(frontrun.2)
        .with_gas_price(gas_price.into());

    let backrun_tx = TransactionRequest::default()
        .with_from(WALLET_SIGNER.address())
        .with_to(MEV_ADDRESS)
        .with_chain_id(CHAIN_ID)
        .with_nonce(nonce + 1)
        .with_value(bribe)
        .with_gas_limit((backrun.0 * 10 / 5).into())
        .with_access_list(backrun.1)
        .with_input(backrun.2)
        .with_gas_price(gas_price.into());

    // Sign the front run
    let wallet = EthereumWallet::from(WALLET_SIGNER.clone());
    let tx_envelope = frontrun_tx.build(&wallet).await.unwrap();
    raw.insert(0, tx_envelope.encoded_2718().encode_hex().into());

    sender
        .send(Some((tracker, backrun_tx, raw, profit.to())))
        .unwrap();
}

pub type SandwichTransaction = (u64, AccessList, Vec<u8>);
pub type SandwichSearch =
    Result<(SandwichTransaction, SandwichTransaction, U256), Box<dyn std::error::Error>>;

// invoked if SANDWICH_ENABLED == true
pub fn sandwich_searcher(
    db: Arc<DatabaseEnv>,
    pool: Arc<Pools>,
    transaction: Arc<Transaction>,
) -> SandwichSearch {
    let now = Instant::now();

    // Set up the DB
    let (db_latest_block, _) = db
        .clone()
        .tx()?
        .new_cursor::<tables::BlockBodyIndices>()?
        .last()?
        .ok_or("Failed to get last block DB")?;
    let db = LatestState.read().unwrap().clone();
    let mut cache_db = State::builder().with_database_ref(db.clone()).build();
    cache_db.insert_account(
        SEARCHER_ADDRESS,
        AccountInfo {
            balance: parse_ether("100")?,
            nonce: 0,
            code_hash: SEARCHER_HASH,
            code: Some(SEARCHER_BYTE_CODE.clone()),
        },
    );

    // Set up the EVM
    let mut evm = Evm::builder()
        .with_db(cache_db)
        .with_spec_id(revm::primitives::SpecId::CANCUN)
        .modify_cfg_env(|c| {
            c.chain_id = CHAIN_ID;
            c.disable_base_fee = true;
            c.disable_balance_check = true;
        })
        .modify_block_env(|b| {
            b.basefee = U256::ZERO;
            b.number = U256::from(db_latest_block);
            b.timestamp = U256::from(
                DateTime::from_system_time(SystemTime::now())
                    .unwrap()
                    .unix_duration()
                    .as_secs(),
            );
        })
        .build();

    // Set the Swap
    let token_out = *pool
        .tokens()
        .iter()
        .find(|t| **t != WETH_ADDRESS)
        .ok_or("No token but WETH")?;
    let route = if token_out > WETH_ADDRESS {
        (0, 1)
    } else {
        (1, 0)
    };
    let frontrun_swap = DirectedGraphEdge::new(pool.clone(), token_out, route);
    let swaps = vec![frontrun_swap.to_swap()];

    let mut lower_bound = U256::ZERO;
    let mut upper_bound = parse_ether("1.6")?;
    let mut sandwichable = false;

    for _ in 0..100 {
        let mid = (lower_bound + upper_bound) / U256::from(2);

        // Set front run
        let mev = evm.tx_mut();
        mev.caller = WALLET_SIGNER.address();
        mev.transact_to = TransactTo::Call(SEARCHER_ADDRESS);
        mev.value = U256::ZERO;
        mev.data = Searcher::forSandwichCall {
            amount_in: mid,
            swaps: swaps.clone(),
            is_backrun: false,
        }
        .abi_encode()
        .into();

        match evm.transact_commit()? {
            ExecutionResult::Success { .. } => {}
            _ => {
                upper_bound = mid;
                continue;
            }
        }

        // Now check the victim
        let victim = evm.tx_mut();
        victim.transact_to = TransactTo::Call(transaction.to().unwrap());
        victim.caller = transaction.from();
        victim.value = transaction.value();
        victim.data = transaction.input().clone();

        match evm.transact()?.result {
            ExecutionResult::Success { .. } => {
                sandwichable = true;
                lower_bound = mid
            }
            _ => upper_bound = mid,
        }

        // Clear
        let mut cache_db = State::builder().with_database_ref(db.clone()).build();
        cache_db.insert_account(
            SEARCHER_ADDRESS,
            AccountInfo {
                balance: parse_ether("100")?,
                nonce: 0,
                code_hash: SEARCHER_HASH,
                code: Some(SEARCHER_BYTE_CODE.clone()),
            },
        );
        *evm.db_mut() = cache_db;
    }

    if !sandwichable {
        return Err("No Sandwich Found".into());
    }

    // Set front run
    {
        let mev = evm.tx_mut();
        mev.caller = WALLET_SIGNER.address();
        mev.transact_to = TransactTo::Call(SEARCHER_ADDRESS);
        mev.value = U256::ZERO;
        mev.data = Searcher::forSandwichCall {
            amount_in: lower_bound,
            swaps: swaps.clone(),
            is_backrun: false,
        }
        .abi_encode()
        .into();
    }

    let frontrun_values = match evm.transact_commit()? {
        ExecutionResult::Success { output, .. } => {
            Searcher::forSandwichCall::abi_decode_returns(output.data(), false)?._0
        }
        _ => return Err("Setting Env failed".into()),
    };

    // Set victim
    {
        let victim = evm.tx_mut();
        victim.transact_to = TransactTo::Call(transaction.to().unwrap());
        victim.caller = transaction.from();
        victim.value = transaction.value();
        victim.data = transaction.input().clone();

        match evm.transact_commit()? {
            ExecutionResult::Success { .. } => {}
            _ => return Err("Env Victim failed".into()),
        }
    }

    // Backrun
    let route = if token_out > WETH_ADDRESS {
        (1, 0)
    } else {
        (0, 1)
    };
    let backrun_swap = DirectedGraphEdge::new(pool, WETH_ADDRESS, route);

    // Set back run
    {
        let mev = evm.tx_mut();
        mev.caller = WALLET_SIGNER.address();
        mev.transact_to = TransactTo::Call(SEARCHER_ADDRESS);
        mev.value = U256::ZERO;
        mev.data = Searcher::forSandwichCall {
            amount_in: *frontrun_values.last().unwrap(),
            swaps: vec![backrun_swap.to_swap()],
            is_backrun: true,
        }
        .abi_encode()
        .into();
    }

    let backrun_values = match evm.transact()?.result {
        ExecutionResult::Success { output, .. } => {
            Searcher::forSandwichCall::abi_decode_returns(output.data(), false)?._0
        }
        _ => return Err("Setting Env Backrun failed".into()),
    };

    let profit = match backrun_values
        .last()
        .unwrap()
        .checked_sub(*frontrun_values.first().unwrap())
    {
        Some(U256::ZERO) | None => return Err("No Profitable Sandwich Found".into()),
        Some(num) => num,
    };

    // build MEV swaps
    let mev_frontrun_swaps = frontrun_swap.to_swap_mev(frontrun_values[0], frontrun_values[1]);
    let mev_frontrun_calldata = Rustitrage::swapCall {
        miner: Address::ZERO,
        finalBalance: U256::ZERO,
        swaps: vec![mev_frontrun_swaps],
    }
    .abi_encode();
    let mev_backrun_swaps = backrun_swap.to_swap_mev(backrun_values[0], backrun_values[1]);

    // Insert Access List Inspector
    let mut evm = evm
        .modify()
        .reset_handler_with_external_context(AccessListInspector::new(
            Default::default(),
            WALLET_SIGNER.address(),
            MEV_ADDRESS,
            Vec::default(),
        ))
        .append_handler_register(inspector_handle_register)
        .build();

    // Clear
    *evm.db_mut() = State::builder().with_database_ref(db.clone()).build();

    // Set real MEV Frontrun
    {
        let real_mev = evm.tx_mut();
        real_mev.caller = WALLET_SIGNER.address();
        real_mev.transact_to = TransactTo::Call(MEV_ADDRESS);
        real_mev.value = U256::ZERO;
        real_mev.data = mev_frontrun_calldata.clone().into();
    }

    let (frontrun_gas, frontrun_accesslist) = match evm.transact_commit()? {
        ExecutionResult::Success { gas_used, .. } => (gas_used, evm.context.external.access_list()),
        _ => return Err("Real MEV frontrun failed".into()),
    };

    // Set victim
    {
        let victim = evm.tx_mut();
        victim.transact_to = TransactTo::Call(transaction.to().unwrap());
        victim.caller = transaction.from();
        victim.value = transaction.value();
        victim.data = transaction.input().clone();

        match evm.transact_commit()? {
            ExecutionResult::Success { .. } => {}
            _ => return Err("Last Env Victim failed".into()),
        }
    }

    // Insert Access List Inspector
    let mut evm = evm
        .modify()
        .reset_handler_with_external_context(AccessListInspector::new(
            Default::default(),
            WALLET_SIGNER.address(),
            MEV_ADDRESS,
            Vec::default(),
        ))
        .append_handler_register(inspector_handle_register)
        .build();

    // Set real MEV Backrun
    {
        let real_mev = evm.tx_mut();
        real_mev.caller = WALLET_SIGNER.address();
        real_mev.transact_to = TransactTo::Call(MEV_ADDRESS);
        real_mev.value = U256::ZERO;
        real_mev.data = Rustitrage::swapCall {
            miner: Address::ZERO,
            finalBalance: U256::ZERO,
            swaps: vec![mev_backrun_swaps.clone()],
        }
        .abi_encode()
        .into();
    }

    let ResultAndState { result, state } = evm.transact()?;
    let final_balance = state
        .get(&WETH_ADDRESS)
        .ok_or("No WETH Found in last result")?
        .storage
        .get(&U256::from_str(MEV_SLOT_WETH)?)
        .ok_or("MEV WETH Slot not found in results")?
        .present_value();
    let (backrun_gas, backrun_accesslist) = match result {
        ExecutionResult::Success { gas_used, .. } => (gas_used, evm.context.external.access_list()),
        _ => return Err("Real MEV backrun failed".into()),
    };

    info!(profit=?format_ether(profit), ?transaction.hash, ?frontrun_values, ?backrun_values, ?frontrun_gas, ?backrun_gas, elapsed=?now.elapsed(), "Sandwich Found");

    Ok((
        (frontrun_gas, frontrun_accesslist, mev_frontrun_calldata),
        (
            backrun_gas,
            backrun_accesslist,
            Rustitrage::swapCall {
                miner: address!("4848489f0b2bedd788c696e2d79b6b69d7484848"),
                finalBalance: final_balance,
                swaps: vec![mev_backrun_swaps],
            }
            .abi_encode(),
        ),
        profit,
    ))
}

pub fn state_handler(
    pools: HashMap<Address, Arc<Pools>>,
    transactions: Vec<Arc<Transaction>>,
    db: Arc<DatabaseEnv>,
    raw: Vec<Arc<String>>,
    bundler: Bsender<BundleSender>,
) {
    let (send, rec) = channel();
    let araw = raw.clone();
    let now = Instant::now();
    // Spawn receiver
    tokio::spawn(async move {
        while let Ok(Some((arb, tracker))) = rec.recv() {
            searcher_handler(
                arb,
                transactions.clone(),
                db.clone(),
                araw.clone(),
                bundler.clone(),
                tracker,
            );
        }
    });

    let mut graph = GRAPH.read().unwrap().clone();
    for pool in pools.values() {
        graph.update_edge(pool);
    }
    let count = graph.find_arbs(
        Address::random(),
        send.clone(),
        &WETH_ADDRESS,
        3,
        pools.keys().cloned().collect::<Vec<_>>(),
    );
    debug!(elapsed=?now.elapsed(), ?count, ?raw, "ARB ROUTINGs");
}

// figure out the logic for leg priroity in backruns
// it could be middle leg also

// Logic of legs should be based on price and not direction

#[cfg(test)]
mod search_tests {
    use hex::FromHex;
    use reth::primitives::{Bytecode, Receipt};
    use reth::providers::ReceiptProvider;
    use reth::providers::{providers::StaticFileProvider, BlockNumReader, DatabaseProvider};
    use reth::revm::database::StateProviderDatabase;
    use reth::rpc::types::trace::geth::CallConfig;
    use reth_db::cursor::DbCursorRO;
    use reth_db::mdbx::DatabaseArguments;
    use reth_db::{open_db_read_only, tables, Database};
    use revm::primitives::{b256, keccak256};
    use revm::DatabaseRef;
    use serde_json::json;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::Path;

    use super::*;

    #[derive(Deserialize, Serialize)]
    struct Found {
        address: Address,
        dex: PoolType,
    }

    #[tokio::test]
    #[ignore = "to be implemented"]
    async fn huff() {
        // Create a provider for both HTTP and WS
        let ipc_provider = ProviderBuilder::new()
            .on_ipc(IPC_NODE_URL.clone().into())
            .await
            .unwrap();

        let one_inch = Pools::OneInch(serde_json::from_str::<OneInch>(r#"{
            "signature": "0x627529a48a94c6c6d4a9e1d09575bc19b6432f58f665c84a5a444eca751e9faa27318b12387f853c5f5f8ba4534aa2d97541f0c1d4915ac24e46d96d1a68fc051b",
            "orderHash": "0x1e5a48a216a6d5761a0c3fabbfb78e51b68d24f8fb93229ab3c8cbea75d4c3b4",
            "createDateTime": "2024-09-18T12:07:37.632Z",
            "remainingMakerAmount": "847480080780060000000000",
            "makerBalance": "847480080780064387000629",
            "makerAllowance": "115792089237316195423570985008687907853269984665640563894239031577355103737174",
            "data": {
            "makerAsset": "0xccdf812aa7cdee4ff7cb89546d7f1718bb8d46e1",
            "takerAsset": "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c",
            "salt": "26982845715926716192792853340",
            "receiver": "0x0000000000000000000000000000000000000000",
            "makingAmount": "847480080780060000000000",
            "takingAmount": "5580000000000000000",
            "maker": "0xaa53d75c4f70725369b4d1b61d2ba2dd782a9af3",
            "extension": "0x",
            "makerTraits": "0x40800000000000000000000000000000000066eeb6fd00000000000000000000"
            },
            "makerRate": "0.000006584225548834",
            "takerRate": "151878.150677430107526882",
            "isMakerContract": false,
            "orderInvalidReason": null
        }"#).unwrap());

        let r = dbg!(one_inch.rate((0, 1)));
        let r2 = dbg!(one_inch.rate((1, 0)));
        dbg!(3792587.0 * r.unwrap());
        dbg!(3792587.0 * r2.unwrap());

        let direct = DirectedGraphEdge::new(
            one_inch.clone().into(),
            address!("ccdf812aa7cdee4ff7cb89546d7f1718bb8d46e1"),
            (0, 1),
        )
        .to_swap_mev(U256::from(3792587u128), U256::from(91860000000000u64));

        let _tx = TransactionRequest::default()
            .with_from(WALLET_SIGNER.address())
            .with_to(MEV_ADDRESS)
            .with_input(
                Rustitrage::swapCall {
                    miner: address!("4848489f0b2BEdd788c696e2D79b6b69D7484848"),
                    finalBalance: U256::ZERO,
                    swaps: vec![direct],
                }
                .abi_encode(),
            );

        let tx = TransactionRequest::default()
            .with_from(WALLET_SIGNER.address())
            .with_to(SEARCHER_ADDRESS)
            .with_input(
                Searcher::searchAmountsOutCall {
                    min_size: U256::from(3792587u128),
                    max_size: parse_ether("100").unwrap(),
                    iterations: U256::from(2),
                    accuracy: U256::from(10000000000000u128),
                    swaps: vec![DirectedGraphEdge::new(
                        one_inch.into(),
                        address!("ccdf812aa7cdee4ff7cb89546d7f1718bb8d46e1"),
                        (0, 1),
                    )
                    .to_swap()],
                }
                .abi_encode(),
            );

        let trace_options = GethDebugTracingCallOptions::default()
            .with_tracing_options(
                GethDebugTracingOptions::default()
                    .with_tracer(GethDebugTracerType::BuiltInTracer(
                        GethDebugBuiltInTracerType::CallTracer,
                    ))
                    .with_config(CallConfig::default().only_top_call()),
            )
            .with_state_overrides(StateOverride::from([
                (
                    MEV_ADDRESS,
                    AccountOverride {
                        code: Some(
                            include_str!("../metadata/bytecode/Rustitrage.bin")
                                .parse::<Bytes>()
                                .unwrap(),
                        ),
                        ..Default::default()
                    },
                ),
                (
                    SEARCHER_ADDRESS,
                    AccountOverride {
                        code: Some(
                            include_str!("../metadata/bytecode/Searcher.bin")
                                .parse::<Bytes>()
                                .unwrap(),
                        ),
                        ..Default::default()
                    },
                ),
                (
                    WETH_ADDRESS,
                    AccountOverride {
                        state_diff: Some(HashMap::from([(
                            U256::from_str(SEARCHER_SLOT_WETH).unwrap().into(),
                            parse_ether("100").unwrap().into(),
                        )])),
                        ..Default::default()
                    },
                ),
            ]));

        let _result = ipc_provider
            .debug_trace_call(tx, 42426142.into(), trace_options)
            .await;
    }

    #[test]
    #[ignore = "to be implemented"]
    fn found_to_pools() {
        let time = Instant::now();
        let x: Vec<Found> = load_pools(PoolType::V2).unwrap();
        dbg!(time.elapsed(), x.len());
        if let Ok(db) = open_db_read_only(Path::new("/reth_db/db"), Default::default())
            .unwrap()
            .tx()
        {
            let db_read_only = DatabaseProvider::new(
                db,
                Default::default(),
                StaticFileProvider::read_only(Path::new("/reth_db/static_files")).unwrap(),
                Default::default(),
            );
            let lb = db_read_only.last_block_number().unwrap();
            let state = StateProviderDatabase::new(
                db_read_only.state_provider_by_block_number(lb).unwrap(),
            );
            let mut f = OpenOptions::new()
                .append(true)
                .create(true)
                .open("./V2fixed.json")
                .unwrap();

            for p in x {
                let t0 = state.storage_ref(p.address, U256::from(6)).unwrap();
                let t1 = state.storage_ref(p.address, U256::from(7)).unwrap();
                let v2 = V2 {
                    address: p.address,
                    token0: Address::from_hex(format!("{:0>40x}", t0)).unwrap(),
                    token1: Address::from_hex(format!("{:0>40x}", t1)).unwrap(),
                    fee: U256::from(3),
                    dex: PoolType::V2,
                    ..Default::default()
                };
                writeln!(f, "{:?}", v2).unwrap();
            }
            println!("Done!")
        }
    }

    #[test]
    #[ignore = "to be implemented"]
    fn walk() {
        if let Ok(db) = open_db_read_only(
            Path::new("/reth_db/db"),
            DatabaseArguments::default().with_max_read_transaction_duration(Some(
                reth_libmdbx::MaxReadTransactionDuration::Unbounded,
            )),
        )
        .unwrap()
        .tx()
        {
            let mut x = db.new_cursor::<tables::PlainAccountState>().unwrap();
            let time = Instant::now();
            let mut f = OpenOptions::new()
                .append(true)
                .create(true)
                .open("./master.json")
                .unwrap();

            // Checked
            let uniswp_v2 =
                b256!("5b83bdbcc56b2e630f2807bbadd2b0c21619108066b92a58de081261089e9ce5");
            let _pncke_v2 =
                b256!("60e4bcb14447615ab7c14fda2c2d70ca4191570e8841c75618e627c8f72662f8");
            let sushi_v2 =
                b256!("cd82e2d9daddbf51cbce8d5429a0996e16fc670c4056566f19cf8864ad45a746");
            let tube_v2 = b256!("e5c6d330e62c41961a65a689d18701fa8002f7a154c3a11d5d2587ab58845d5c");

            // Checked
            let ape_v2 = b256!("b3cf10ee2534be06eddb48bbdedbf0d2a27b1fe0198461572ce156879ebebe9f");

            // Checked
            let daoswap_v2 =
                b256!("8f79991c4fa4c79a3d3f4d7109ec898802f55f2753ddb2f14246dfd2945f561f");

            // Checked
            let squad_v2 =
                b256!("cd80f7e9333ea052dbd9535200d4105f7985f33adf665171c364a995b5cfe96d");

            // Checked
            let smardex_v2 =
                b256!("40b03b0faeec1b54982da8b12512fec8397ef5c3d1bcfd366662b42ab3817a9c");

            while let Ok(Some((address, account))) = x.next() {
                let (pool_type, protocol) = match account.bytecode_hash {
                    Some(hash) if hash == uniswp_v2 => (PoolType::V2, "uniswap"),
                    Some(hash) if hash == tube_v2 => (PoolType::V2, "tube"),
                    Some(hash) if hash == squad_v2 => (PoolType::V2, "squad"),
                    Some(hash) if hash == smardex_v2 => (PoolType::V2, "smardex"),
                    Some(hash) if hash == daoswap_v2 => (PoolType::V2, "daoswap"),
                    Some(hash) if hash == sushi_v2 => (PoolType::V2, "sushi"),
                    Some(hash) if hash == ape_v2 => (PoolType::V2, "ape"),
                    _ => continue,
                };

                let m = json!({
                    "address": address,
                    "dex": pool_type,
                    "protocol": protocol,
                })
                .to_string();
                writeln!(f, "{},", m).unwrap();
            }

            println!("Done! {:#?}", time.elapsed())
        }
    }

    #[test]
    fn to_hash() {
        let hash = Bytecode::new_raw(Bytes::from_hex("").unwrap());
        dbg!(hash.hash_slow());
    }

    #[test]
    #[ignore = "to be implemented"]
    fn walk_3() {
        let static_provider =
            StaticFileProvider::read_only(Path::new(&format!("{}/static_files", *DB_PATH_DIR)))
                .unwrap();

        let db = Arc::new(
            open_db_read_only(
                Path::new(&format!("{}/db", *DB_PATH_DIR)),
                Default::default(),
            )
            .unwrap(),
        );
        let dbx = DatabaseProvider::new(
            db.clone().tx().unwrap(),
            Default::default(),
            static_provider,
            Default::default(),
        );

        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .open("./V3_2.json")
            .unwrap();
        let mut f2 = OpenOptions::new()
            .append(true)
            .create(true)
            .open("./V2_2.json")
            .unwrap();
        let time = Instant::now();

        // Checked
        let standard_v2 = b256!("0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9");
        let standard_v3 = b256!("783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118");
        let algebra_v3 = b256!("91ccaa7a278130b65168c3a0c8d3bcae84cf5e43704342bd3ec0b59e59c036db");

        for i in 6100..=7001 {
            eprintln!("{i}");
            if let Ok(l) = dbx.receipts_by_tx_range(i * 1_000_000..(i + 1) * 1_000_000) {
                for Receipt { logs, .. } in l {
                    for log in logs {
                        let topics = log.topics();
                        let (pool_type, pool) = match topics.first() {
                            None => continue,
                            // Standard V2
                            Some(hash) if *hash == standard_v2 => (
                                &mut f2,
                                json!({
                                    "protocol": "Standard",
                                    "dex": PoolType::V2,
                                    "token0": Address::from_slice(&topics[1][12..]),
                                    "token1": Address::from_slice(&topics[2][12..]),
                                    "fee": "3",
                                    "address": log.data.data.slice(12..32),
                                    "factory": log.address,
                                }),
                            ),
                            // Standard V3
                            Some(hash) if *hash == standard_v3 => (
                                &mut f,
                                json!({
                                    "protocol": "Standard",
                                    "dex": PoolType::V3,
                                    "token0": Address::from_slice(&topics[1][12..]),
                                    "token1": Address::from_slice(&topics[2][12..]),
                                    "feeTier": topics[3],
                                    "address": log.data.data.slice(44..),
                                    "factory": log.address,
                                }),
                            ),
                            // algebra V3
                            Some(hash) if *hash == algebra_v3 => (
                                &mut f,
                                json!({
                                    "protocol": "Algebra",
                                    "dex": PoolType::V3,
                                    "token0": Address::from_slice(&topics[1][12..]),
                                    "token1": Address::from_slice(&topics[2][12..]),
                                    "feeTier": "100",
                                    "address": log.data.data.slice(12..),
                                    "factory": log.address,
                                }),
                            ),
                            _ => continue,
                        };

                        writeln!(pool_type, "{},", pool).unwrap();
                    }
                }
            }
        }

        println!("Done! {:#?}", time.elapsed())
    }

    #[tokio::test]
    #[ignore = "to be implemented"]
    async fn walk_org() {
        let db = Arc::new(open_db_read_only(Path::new("/reth_db/db"), Default::default()).unwrap());

        let state_provider = DatabaseProvider::new(
            db.clone().tx().unwrap(),
            Default::default(),
            StaticFileProvider::read_only(Path::new("/reth_db/static_files")).unwrap(),
            Default::default(),
        );

        let state = State::builder()
            .with_database(StateProviderDatabase::new(
                state_provider.state_provider_by_block_number(2).unwrap(),
            ))
            .build();
        let mut _evm = Evm::builder().with_db(state).build();

        let fstatic = StaticFileProvider::read_only(Path::new("/reth_db/static_files")).unwrap();
        let time = Instant::now();
        let _y = fstatic.receipts_by_tx_range(0..10_000_000).unwrap();
        // for i in 0..100_000 {
        // let tx = db.tx().unwrap();
        // dbg!(
        //     tx.new_cursor::<tables::Headers>()
        //         .unwrap()
        //         .last()
        //         .unwrap()
        //         .unwrap()
        //         .0
        // );
        // time::sleep(Duration::from_secs(5)).await;
        // }
        dbg!(time.elapsed());
    }

    #[test]
    fn singed_trx_to_trx_hash() {
        let trx = bytes!("f901ee8306b98d84b2d05e028307a15f940000000000d755e8ec7eea817cedcc2ff977818680b9018451d022e300000000000000000000000063230caefc0f8220536db18136b83b5098b5acbc000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000f18bb68b3d5de03808fc000000000000000000000000bb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c00000000000000000000000059f4f336bf3d0c49dbfba4a74ebd2a6ace40539a00000000000000000000000000000000000000000000000000000000000026f700000000000000000000000000000000000000000000000004bbbd35b472305a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008194a0c6ed6125136010531eaa828cda060350ee9517ee2e81672311ac409b981d83dea01df1be7a1a4751e6021a27a55141147c57a5502b0add5fb440ef178d3dc4562e");
        dbg!(keccak256(trx));
    }
}
