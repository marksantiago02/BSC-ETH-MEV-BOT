use crate::*;
use alloy::eips::eip2718::Encodable2718;
use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    signers::SignerSync,
    sol_types::SolCall,
};
use reth_db::cursor::DbCursorRO;
use reth_db::{tables, Database};
use revm::primitives::keccak256;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::Receiver as BReceiver;
use ureq::{json, Agent};

// TODO this should be a trait
// Do eth
pub fn send_bundle_to_arbonly(
    agent: &Agent,
    hash: &Arc<String>,
    transactions: &[String],
    coinbase_profit: u128,
    block_number: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let json = json!({
        "method": "submit_arb_only_bundle",
        "id": "1",
        "params": {
            "transaction_hash": hash,
            "transaction": transactions,
            "block_number": format!("0x{:x}", block_number),
            "blockchain_network": "BSC-Mainnet",
            "coinbase_profit": coinbase_profit,
        }
    });

    let recv_body = agent
        .post("https://backrunme.blxrbdn.com")
        .set("Authorization", "NzNjMzcwNDMtYWQxYy00ZDk2LWIxOWQtYWQyNjE5YTNkZTkxOjQ1NDc4M2FmY2UwZDhiNzQ2NzFlMmU4ODA5ZTVmOTY1")
        .send_json(&json)?;

    Ok(recv_body.into_string()?)
}

// do ETH instead
pub fn send_bundle_to_bloxroute(
    agent: &Agent,
    transactions: &[Arc<String>],
    block_number: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let json = json!({
        "id": "1",
        "method": "blxr_submit_bundle",
        "params": {
            "transaction": transactions,
            "blockchain_network": "BSC-Mainnet",
            "block_number": format!("0x{:x}", block_number),
            "priority_fee_refund": true,
            "refund_recipient": "0xAC2828E9eb4237EABEd48F20fB5D9F722Dd35f84",
            "mev_builders": {
                "bloxroute": "",
                "all": ""
            }
        }
    });

    let recv_body = agent
        .post("https://mev.api.blxrbdn.com")
        .set("Authorization", "NzNjMzcwNDMtYWQxYy00ZDk2LWIxOWQtYWQyNjE5YTNkZTkxOjQ1NDc4M2FmY2UwZDhiNzQ2NzFlMmU4ODA5ZTVmOTY1")
        .send_json(&json)?;

    Ok(recv_body.into_string()?)
}

pub fn send_bundle_to_txboost(
    agent: &Agent,
    transactions: &[Arc<String>],
    block_number: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let json = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_sendBundle",
        "params": [{
            "txs": transactions.iter().map(|x| format!("0x{x}")).collect::<Vec<_>>(),
            "blockNumber": format!("0x{:x}", block_number)
        }]
    });

    let recv_body = agent
        .post("https://fastbundle-us.blocksmith.org/")
        .set("Authorization", "Basic NTcyOWI1OTgtMzBlOC0zMjM0LWE0MjAtODkzNGViODIwNzI2Ojc2ZjE4Y2ZiNzgzZDI2NWZkZjdmYTdlYjdhZDk3ZTc0")
        .send_json(&json)?;

    Ok(recv_body.into_string()?)
}

pub fn send_bundle_to_blockrazor(
    agent: &Agent,
    transactions: &[Arc<String>],
) -> Result<String, Box<dyn std::error::Error>> {
    let json = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "eth_sendBundle",
        "params": [{
            "txs": transactions.iter().map(|x| format!("0x{x}")).collect::<Vec<_>>()
        }]
    });

    let recv_body = agent
        .post("https://blockrazor-builder-virginia.48.club")
        .set("Authorization", "ITRUh7T7uIAthdVDhxKqmJz8eQMiUIGVW1LhIWYhMWjGreNP63g68QJrFqyozDC8pRTSvOv9A7vTLbKmH9qI9OjE6xdvCexz")
        .send_json(&json)?;

    Ok(recv_body.into_string()?)
}

pub fn send_bundle_to_48(
    agent: &Agent,
    transactions: &[Arc<String>],
) -> Result<String, Box<dyn std::error::Error>> {
    let json = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "eth_sendBundle",
        "params": [{
            "txs": transactions.iter().map(|x| format!("0x{x}")).collect::<Vec<_>>(),
            "48spSign": sign_48_club_bundle(transactions)?,
        }]
    });

    let recv_body = agent
        .post("https://puissant-builder.48.club/")
        .send_json(&json)?;

    Ok(recv_body.into_string()?)
}

fn sign_48_club_bundle(transactions: &[Arc<String>]) -> Result<String, Box<dyn std::error::Error>> {
    let hashes = transactions.iter().fold(String::new(), |mut acc, x| {
        acc += &keccak256(x.parse::<Bytes>().unwrap()).encode_hex();
        acc
    });

    let combined = keccak256(hashes.parse::<Bytes>()?);

    let message = WALLET_SIGNER
        .sign_hash_sync(&combined)?
        .with_chain_id(CHAIN_ID);

    Ok(message.as_bytes().encode_hex_with_prefix())
}

pub async fn setup_48club(mut receiver: BReceiver<BundleSender>) {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(3600))
        .timeout_write(Duration::from_secs(3600))
        .build();

    let wallet = EthereumWallet::from(WALLET_SIGNER.clone());

    while let Ok(Some((tracker, mut arb, mut transactions, profit))) = receiver.recv().await {
        let now = Instant::now();
        let mut calldata =
            Rustitrage::swapCall::abi_decode(arb.input.input().unwrap(), false).unwrap();

        // if next block, combine
        calldata.miner = address!("4848489f0b2bedd788c696e2d79b6b69d7484848");
        for (p, s) in NextBlocks.lock().unwrap().iter().rev() {
            if *p == U256::from(profit) {
                continue;
            }

            calldata.swaps.extend(s.clone());
            arb.value = Some(arb.value.unwrap_or_default() + p);
            break;
        }

        let tx_envelope = arb
            .with_input(calldata.abi_encode())
            .build(&wallet)
            .await
            .unwrap();
        transactions.push(tx_envelope.encoded_2718().encode_hex().into());
        let result_48 = send_bundle_to_48(&agent, &transactions);
        debug!(?tracker, elapsed = ?now.elapsed(), ?result_48, "48Club Sending bundle");
        construct_discord_message(
            &agent,
            &DiscordLog::ArbDetected,
            format!("48Club bundle: ```json\n{:#?}```", result_48),
        );
    }
}

pub async fn setup_bloxroute(mut receiver: BReceiver<BundleSender>) {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(3600))
        .timeout_write(Duration::from_secs(3600))
        .build();

    let wallet = EthereumWallet::from(WALLET_SIGNER.clone());
    let db = Opened_DB.clone();

    while let Ok(Some((tracker, mut arb, mut transactions, profit))) = receiver.recv().await {
        let now = Instant::now();
        let mut calldata =
            Rustitrage::swapCall::abi_decode(arb.input.input().unwrap(), false).unwrap();

        // if next block, combine
        calldata.miner = address!("74c5F8C6ffe41AD4789602BDB9a48E6Cad623520");
        for (p, s) in NextBlocks.lock().unwrap().iter().rev() {
            if *p == U256::from(profit) {
                continue;
            }

            calldata.swaps.extend(s.clone());
            arb.value = Some(arb.value.unwrap_or_default() + p);
            break;
        }

        // get latest block
        let (db_latest_block, _) = db
            .clone()
            .tx()
            .unwrap()
            .new_cursor::<tables::BlockBodyIndices>()
            .unwrap()
            .last()
            .unwrap()
            .unwrap();

        let tx_envelope = arb
            .with_input(calldata.abi_encode())
            .build(&wallet)
            .await
            .unwrap();
        transactions.push(tx_envelope.encoded_2718().encode_hex().into());
        let result_bloxroute = send_bundle_to_bloxroute(&agent, &transactions, db_latest_block + 1);
        debug!(?tracker, elapsed = ?now.elapsed(), ?result_bloxroute, "Bloxroute Sending bundle");
        construct_discord_message(
            &agent,
            &DiscordLog::ArbDetected,
            format!("Bloxroute bundle: ```json\n{:#?}```", result_bloxroute),
        );
    }
}

pub async fn setup_txboost(mut receiver: BReceiver<BundleSender>) {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(3600))
        .timeout_write(Duration::from_secs(3600))
        .build();

    let wallet = EthereumWallet::from(WALLET_SIGNER.clone());
    let db = Opened_DB.clone();

    while let Ok(Some((tracker, mut arb, mut transactions, profit))) = receiver.recv().await {
        let now = Instant::now();
        let mut calldata =
            Rustitrage::swapCall::abi_decode(arb.input.input().unwrap(), false).unwrap();

        // if next block, combine
        calldata.miner = address!("0000000000007592b04bB3BB8985402cC37Ca224");
        for (p, s) in NextBlocks.lock().unwrap().iter().rev() {
            if *p == U256::from(profit) {
                continue;
            }

            calldata.swaps.extend(s.clone());
            arb.value = Some(arb.value.unwrap_or_default() + p);
            break;
        }

        // get latest block
        let (db_latest_block, _) = db
            .clone()
            .tx()
            .unwrap()
            .new_cursor::<tables::BlockBodyIndices>()
            .unwrap()
            .last()
            .unwrap()
            .unwrap();

        let tx_envelope = arb
            .with_input(calldata.abi_encode())
            .build(&wallet)
            .await
            .unwrap();
        transactions.push(tx_envelope.encoded_2718().encode_hex().into());
        let result_txboost = send_bundle_to_txboost(&agent, &transactions, db_latest_block + 1);
        debug!(?tracker, elapsed = ?now.elapsed(), ?result_txboost, "TxBoost Sending bundle");
        construct_discord_message(
            &agent,
            &DiscordLog::ArbDetected,
            format!("TxBoost bundle: ```json\n{:#?}```", result_txboost),
        );
    }
}

pub async fn setup_blackrazor(mut receiver: BReceiver<BundleSender>) {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(3600))
        .timeout_write(Duration::from_secs(3600))
        .build();

    let wallet = EthereumWallet::from(WALLET_SIGNER.clone());

    while let Ok(Some((tracker, mut arb, mut transactions, profit))) = receiver.recv().await {
        let now = Instant::now();
        let mut calldata =
            Rustitrage::swapCall::abi_decode(arb.input.input().unwrap(), false).unwrap();

        // if next block, combine
        calldata.miner = address!("1266C6bE60392A8Ff346E8d5ECCd3E69dD9c5F20");
        for (p, s) in NextBlocks.lock().unwrap().iter().rev() {
            if *p == U256::from(profit) {
                continue;
            }

            calldata.swaps.extend(s.clone());
            arb.value = Some(arb.value.unwrap_or_default() + p);
            break;
        }

        let tx_envelope = arb
            .with_input(calldata.abi_encode())
            .build(&wallet)
            .await
            .unwrap();
        transactions.push(tx_envelope.encoded_2718().encode_hex().into());
        let result_blackrazor = send_bundle_to_blockrazor(&agent, &transactions);
        debug!(?tracker, elapsed = ?now.elapsed(), ?result_blackrazor, "BlackRazor Sending bundle");
        construct_discord_message(
            &agent,
            &DiscordLog::ArbDetected,
            format!("BlackRazor bundle: ```json\n{:#?}```", result_blackrazor),
        );
    }
}

pub async fn setup_arbonly(mut receiver: BReceiver<BundleSender>) {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(3600))
        .timeout_write(Duration::from_secs(3600))
        .build();

    let wallet = EthereumWallet::from(WALLET_SIGNER.clone());
    let db = Opened_DB.clone();

    while let Ok(Some((tracker, mut arb, transactions, profit))) = receiver.recv().await {
        let now = Instant::now();
        let mut calldata =
            Rustitrage::swapCall::abi_decode(arb.input.input().unwrap(), false).unwrap();

        // if next block, combine
        calldata.miner = address!("965Df5Ff6116C395187E288e5C87fb96CfB8141c");
        for (p, s) in NextBlocks.lock().unwrap().iter().rev() {
            if *p == U256::from(profit) {
                continue;
            }

            calldata.swaps.extend(s.clone());
            arb.value = Some(arb.value.unwrap_or_default() + p);
            break;
        }

        // get latest block
        let (db_latest_block, _) = db
            .clone()
            .tx()
            .unwrap()
            .new_cursor::<tables::BlockBodyIndices>()
            .unwrap()
            .last()
            .unwrap()
            .unwrap();

        // ArbOnly
        if let Some(hash) = transactions.first() {
            // if not arbonly, skip
            if hash.len() != 64 {
                continue;
            }

            let config = transactions.clone().pop().unwrap();
            let config: BackrunmeConfig = serde_json::from_str(&config).unwrap();

            // change bribes
            let profit = arb.value.unwrap_or_default().to::<u128>() as f64;
            let expected_gas = arb.gas.unwrap() / 2;
            let coinbase_profit = profit * config.miner_share / 100.0;
            let bloxroute_porfit = profit * config.bloxroute_share / 100.0;

            let arb_only_backrun = [arb
                .with_value(U256::from(bloxroute_porfit))
                .with_gas_price(coinbase_profit as u128 / expected_gas)
                .with_input(calldata.abi_encode())
                .build(&wallet)
                .await
                .unwrap()
                .encoded_2718()
                .encode_hex()];

            for b in 1..=3 {
                let result = send_bundle_to_arbonly(
                    &agent,
                    hash,
                    &arb_only_backrun[..],
                    coinbase_profit as u128,
                    db_latest_block + b,
                );

                debug!(?tracker, elapsed = ?now.elapsed(), ?result, "ArbOnlyBundle Sending bundle");
                construct_discord_message(
                    &agent,
                    &DiscordLog::ArbDetected,
                    format!("ArbOnly bundle: ```json\n{:#?}```", result),
                );
            }
        }
    }
}
