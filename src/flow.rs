use crate::*;
use alloy::consensus::TxEnvelope;
use alloy::eips::eip2718::Encodable2718;
use alloy_rlp::Decodable;
use futures_util::{SinkExt, StreamExt};
use reth::primitives::TransactionSignedEcRecovered;
use std::{thread::sleep, time::Duration};
use tokio::sync::broadcast::Sender as Bsender;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{handshake::client::generate_key, http::Request, Message},
};
use ureq::json;

pub async fn setup_merkle_flow(
    mixer: Option<Arc<Sender<FoundVictim>>>,
    bundler: Bsender<BundleSender>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while !*SYNCED
        .read()
        .map_err(|e| format!("Fetching Sync Status Failed {:?}", e))?
    {
        sleep(Duration::from_secs(1))
    }

    // Merkle
    let (mut socket, _) = connect_async::<String>(
        "wss://mempool.merkle.io/stream/private/keg-6ozH8Qn3TcOcco04GIsduQQjpbYa6".into(),
    )
    .await
    .map_err(|e| format!("Fetching Merkle Flow Failed {:?}", e))?;

    let db = Opened_DB.clone();

    loop {
        if let Some(Ok(Message::Text(raw))) = socket.next().await {
            let raw = match raw.split('"').nth(1) {
                Some(raw) => Arc::new(raw.to_string()),
                None => {
                    error!("Failed to split merkle raw, {}", raw);
                    continue;
                }
            };

            let signed_transaction =
                TransactionSignedEcRecovered::decode(&mut hex::decode(&*raw)?.as_slice())?;

            // If transaction is a duplicate,
            // Or if the transaction fee is less than 1 gwei, skip
            let hash = signed_transaction.hash;
            if !TRANSACTION_TRACKER
                .lock()
                .map_err(|e| format!("Fetching Transactions History Failed {:?}", e))?
                .insert(hash)
                || signed_transaction.max_fee_per_gas() < 1_000_000_000
            {
                continue;
            };

            let new_trx = Arc::new(Transaction {
                hash,
                from: signed_transaction.signer(),
                to: signed_transaction.to(),
                value: signed_transaction.value(),
                input: signed_transaction.input().clone(),
                ..Default::default()
            });

            if let Some((_, useful_pools)) =
                pending_transaction_state(&[new_trx.clone()], Some(raw.clone()), mixer.clone())
            {
                debug!(merkle=?hash, pools=useful_pools.len(), ?raw);
                for p in useful_pools.keys() {
                    if *ARBITRAGE_ENABLED {
                        let routes = ROUTES
                            .read()
                            .map_err(|e| format!("Fetching Routes Failed {:?}", e))?;

                        for (_, arb) in routes.iter().filter(|(k, _)| k.contains(&p.encode_hex())) {
                            searcher_handler(
                                arb.clone(),
                                vec![new_trx.clone()],
                                db.clone(),
                                vec![raw.clone()],
                                bundler.clone(),
                                Address::from_word(hash),
                            );
                        }
                    }
                }
            }
        }
    }
}

pub async fn setup_bloxroute_flow(
    mixer: Option<Arc<Sender<FoundVictim>>>,
    bundler: Bsender<BundleSender>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while !*SYNCED
        .read()
        .map_err(|e| format!("Fetching Sync Status Failed {:?}", e))?
    {
        sleep(Duration::from_secs(1))
    }

    let request = Request::builder()
        .method("GET")
        .header("Authorization", "NzNjMzcwNDMtYWQxYy00ZDk2LWIxOWQtYWQyNjE5YTNkZTkxOjQ1NDc4M2FmY2UwZDhiNzQ2NzFlMmU4ODA5ZTVmOTY1")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Host", "backrunme.blxrbdn.com")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", generate_key())
        .uri("wss://backrunme.blxrbdn.com/ws")
        .body(())?;

    let (mut ws_remote, _) = connect_async(request).await?;

    // Fast Mempool
    if *FAST_MEMPOOL_ENABLED {
        ws_remote
            .send(Message::Text(
                json!({"jsonrpc": "2.0", "id": 1, "method": "subscribe", "params": ["newTxs", {"include": ["tx_contents"], "duplicates": false, "blockchain_network": "BSC-Mainnet"}]})
                .to_string(),
            ))
            .await?;
        ws_remote.next().await.transpose()?;
    }

    // Arb Only
    if *ARBONLY_ENABLED {
        ws_remote
            .send(Message::Text(
                json!({"id": 1, "method": "subscribe", "params": ["bscArbOnlyMEV", {"include": []}]})
                    .to_string(),
            ))
            .await?;
        ws_remote.next().await.transpose()?;
    }

    let db = Opened_DB.clone();

    loop {
        if let Some(Ok(Message::Text(raw))) = ws_remote.next().await {
            let ResultData {
                tx_contents,
                transactions,
                backrunme_config,
            } = serde_json::from_str::<RpcRequest>(&raw)?.params.result;

            // Fast Mempool
            if let Some(tx_contents) = tx_contents {
                // Get raw signed transaction
                let raw = Arc::new(
                    TxEnvelope::try_from((*tx_contents).clone())?
                        .encoded_2718()
                        .encode_hex(),
                );

                if let Some((_, useful_pools)) = pending_transaction_state(
                    &[tx_contents.clone()],
                    Some(raw.clone()),
                    mixer.clone(),
                ) {
                    debug!(bloxroute=?tx_contents.hash, pools=useful_pools.len(), ?raw);
                    for (pool_address, pool) in useful_pools {
                        if *ARBITRAGE_ENABLED {
                            let routes = ROUTES
                                .read()
                                .map_err(|e| format!("Fetching Routes Failed {:?}", e))?;

                            for (_, arb) in routes
                                .iter()
                                .filter(|(k, _)| k.contains(&pool_address.encode_hex()))
                            {
                                searcher_handler(
                                    arb.clone(),
                                    vec![tx_contents.clone()],
                                    db.clone(),
                                    vec![raw.clone()],
                                    bundler.clone(),
                                    Address::from_word(tx_contents.hash),
                                );
                            }
                        }

                        if *SANDWICH_ENABLED {
                            if let Ok((frontrun, backrun, profit)) =
                                sandwich_searcher(db.clone(), pool, tx_contents.clone())
                            {
                                tokio::spawn(sandwich_bundler(
                                    Address::from_word(tx_contents.hash),
                                    frontrun,
                                    backrun,
                                    profit,
                                    vec![raw.clone()],
                                    bundler.clone(),
                                ));
                            }
                        }
                    }
                }
            }

            // Arb Only
            if let (Some(transactions), Some(config)) = (transactions, backrunme_config) {
                // TODO remove this filter
                if config.user_share != 0.0 || config.additional_wallet.is_some() {
                    continue;
                }

                for TransactionBlox { tx_contents } in transactions {
                    let raw = Arc::new(tx_contents.hash.encode_hex());

                    if let Some((_, useful_pools)) = pending_transaction_state(
                        &[tx_contents.clone()],
                        Some(raw.clone()),
                        mixer.clone(),
                    ) {
                        debug!(arbOnly=?tx_contents.hash, pools=useful_pools.len(), ?raw);
                        if *ARBITRAGE_ENABLED {
                            for p in useful_pools.keys() {
                                let routes = ROUTES
                                    .read()
                                    .map_err(|e| format!("Fetching Routes Failed {:?}", e))?;
                                let raw = vec![raw.clone(), serde_json::to_string(&config)?.into()];
                                let tracker = Address::from_word(tx_contents.hash);

                                for (_, arb) in
                                    routes.iter().filter(|(k, _)| k.contains(&p.encode_hex()))
                                {
                                    searcher_handler(
                                        arb.clone(),
                                        vec![tx_contents.clone()],
                                        db.clone(),
                                        raw.clone(),
                                        bundler.clone(),
                                        tracker,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub async fn setup_public_flow(
    provider: RootProvider<PubSubFrontend>,
    mixer: Option<Arc<Sender<FoundVictim>>>,
    bundler: Bsender<BundleSender>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while !*SYNCED
        .read()
        .map_err(|e| format!("Fetching Sync Status Failed {:?}", e))?
    {
        sleep(Duration::from_secs(1))
    }

    let mut stream_pending_transactions = provider.subscribe_full_pending_transactions().await?;
    let db = Opened_DB.clone();

    loop {
        if let Ok(transaction) = stream_pending_transactions.recv().await {
            let transaction = Arc::new(transaction);

            let raw = Arc::new(
                TxEnvelope::try_from((*transaction).clone())?
                    .encoded_2718()
                    .encode_hex(),
            );

            if let Some((_, useful_pools)) =
                pending_transaction_state(&[transaction.clone()], Some(raw.clone()), mixer.clone())
            {
                debug!(public=?transaction.hash, pools=useful_pools.len(), ?raw);
                for (pool_address, pool) in useful_pools {
                    if *ARBITRAGE_ENABLED {
                        let routes = ROUTES
                            .read()
                            .map_err(|e| format!("Fetching Routes Failed {:?}", e))?;

                        for (_, arb) in routes
                            .iter()
                            .filter(|(k, _)| k.contains(&pool_address.encode_hex()))
                        {
                            searcher_handler(
                                arb.clone(),
                                vec![transaction.clone()],
                                db.clone(),
                                vec![raw.clone()],
                                bundler.clone(),
                                Address::from_word(transaction.hash),
                            );
                        }
                    }

                    if *SANDWICH_ENABLED {
                        if let Ok((frontrun, backrun, profit)) =
                            sandwich_searcher(db.clone(), pool.clone(), transaction.clone())
                        {
                            tokio::spawn(sandwich_bundler(
                                Address::from_word(transaction.hash),
                                frontrun,
                                backrun,
                                profit,
                                vec![raw.clone()],
                                bundler.clone(),
                            ));
                        }
                    }
                }
            }
        }
    }
}
