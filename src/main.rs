use alloy::primitives::utils::parse_ether;
use reth::providers::providers::StaticFileProvider;
use reth::providers::{LatestStateProvider, StateProvider};
use reth::revm::database::StateProviderDatabase;
use reth_db::cursor::DbCursorRO;
use reth_db::{tables, Database};
use revm::DatabaseRef;
use hades::*;
use std::path::Path;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::Instant;
use ureq::Agent;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a rolling file appender that rotates logs every day.
    let minimum_liq = parse_ether("1").ok();
    let mut last_synced = 0;
    let file_appender = RollingFileAppender::new(Rotation::HOURLY, "./logs", "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Terminal Layer
    let terminal_layer = tracing_subscriber::fmt::layer()
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_target(false)
        .without_time()
        .with_line_number(false)
        .with_file(false)
        .with_filter(filter::LevelFilter::INFO);

    // File Layer
    let file_layer = tracing_subscriber::fmt::layer()
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_target(false)
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_filter(filter::Targets::new().with_targets(vec![("rustitrage", Level::TRACE)]));

    // Logger
    tracing_subscriber::Registry::default()
        .with(terminal_layer)
        .with(file_layer)
        .init();

    // Makes sure all the environment variables are loaded
    init_env_vars();
    trace!("Initialized environment vars");
    let startup_message = format!("Starting hades\n- MEV:  *{}*\n- Merkle:  *{}*\n- ArbOnly:  *{}*\n- Fast Mempool:  *{}*\n- Public Mempool:  *{}*\n- NextBlock:  *{}*\n- Sandwich:  *{}*\n- Arbitrage:  *{}*",
        MEV_ADDRESS,
        *MERKLE_ENABLED,
        *ARBONLY_ENABLED,
        *FAST_MEMPOOL_ENABLED,
        *PUBLIC_MEMPOOL_ENABLED,
        *NEXTBLOCK_ENABLED,
        *SANDWICH_ENABLED,
        *ARBITRAGE_ENABLED
    );
    info!("{}", &startup_message);
    construct_discord_message(&Agent::new(), &DiscordLog::UpdatePools, startup_message);

    // Load pools
    trace!("Loading pools");
    {
        let mut storage = POOL_STORAGE.write()?;
        load_pools(PoolType::DoDo)?.into_iter().for_each(|p: DoDo| {
            storage.insert(p.address, Pools::DoDo(p).into());
        });
        load_pools(PoolType::V2)?.into_iter().for_each(|p: V2| {
            storage.insert(p.address, Pools::V2(p).into());
        });
        load_pools(PoolType::V3)?.into_iter().for_each(|p: V3| {
            storage.insert(p.address, Pools::V3(p).into());
        });
        info!("Loaded {} pools", storage.len());
    }

    // Create a provider for IPC
    trace!("Connecting to IPC at {}", *IPC_NODE_URL);
    let ipc_provider = match ProviderBuilder::new()
        .on_ipc(IPC_NODE_URL.clone().into())
        .await
    {
        Ok(provider) => provider,
        Err(err) => {
            error!(
                "Could not connect to reth node, the error was: {}",
                err
            );
            return Err(err.into());
        }
    };

    // Subscribe to new events
    let mut stream_mined_blocks = ipc_provider.subscribe_blocks().await?;

    // Send Bundles
    trace!("Setting up bundle handlers");
    let (bundle_sender, bundle_receiver) = broadcast::channel::<BundleSender>(10000);
    tokio::spawn(setup_48club(bundle_sender.subscribe()));
    tokio::spawn(setup_txboost(bundle_sender.subscribe()));
    tokio::spawn(setup_blackrazor(bundle_sender.subscribe()));
    tokio::spawn(setup_bloxroute(bundle_receiver));

    let (mixer_sender, mixer_reciver) = channel::<FoundVictim>();
    let mixer_sender = Arc::new(mixer_sender);

    // Bloxroute Flow
    if *ARBONLY_ENABLED || *FAST_MEMPOOL_ENABLED {
        trace!("Setting up Arb only");
        tokio::spawn(setup_arbonly(bundle_sender.subscribe()));

        trace!("Setting up Bloxroute flow");
        tokio::spawn(setup_bloxroute_flow(
            Some(mixer_sender.clone()),
            bundle_sender.clone(),
        ));
    }

    // Merkle Flow
    if *MERKLE_ENABLED {
        trace!("Setting up Merkle flow");
        tokio::spawn(setup_merkle_flow(
            Some(mixer_sender.clone()),
            bundle_sender.clone(),
        ));
    }

    // Public Flow
    if *PUBLIC_MEMPOOL_ENABLED {
        trace!("Setting up public flow");
        tokio::spawn(setup_public_flow(
            ipc_provider.clone(),
            Some(mixer_sender.clone()),
            bundle_sender.clone(),
        ));
    }

    // TODO Mixer thread
    tokio::spawn(async move {
        loop {
            if let Ok((_address, _route, _transaction, _signed)) = mixer_reciver.recv() {
                // let mut mixer = MIXER.write().unwrap_or_else(|e| {
                //     error!("Failed to get mixer {}", e);
                //     panic!("Failed to get mixer {}", e)
                // });
                // let backruns = mixer.entry((address, route)).or_default();
                // backruns.insert(transaction.from(), (transaction.clone(), signed.clone()));

                // // if there is already a backrun
                // if backruns.len() >= 2 {
                //     let (transactions, signed): (Vec<_>, Vec<_>) =
                //         backruns.values().cloned().unzip();
                //     drop(mixer);
                //     let state_provider = LatestState.read().unwrap().clone();

                //     if let Ok(Some((state, useful_pools))) =
                //         pending_transaction_state(&state_provider, &transactions[..], None, None)
                //     {
                //         debug!(pools = ?useful_pools.values(), ?signed, "MIXED");
                //         tokio::spawn(state_handler(
                //             useful_pools,
                //             state,
                //             cloned_db.clone(),
                //             signed,
                //             bundle_sender3.clone(),
                //         ));
                //     }
                // }
            }
        }
    });

    // Fetch Limit orders
    tokio::spawn(async move {
        trace!("Fetching limit orders");
        let time = Instant::now();
        let agent = ureq::AgentBuilder::new()
            .timeout_read(Duration::from_secs(60))
            .timeout_write(Duration::from_secs(60))
            .build();
        let mut orders_map = HashMap::with_capacity(50_000);
        let mut orders = Vec::with_capacity(50_000);
        for page in 1..50_000 / 500 {
            let orders_v4: Vec<OneInch> = agent.get(&format!("https://limit-orders.1inch.io/v4.0/{CHAIN_ID}/all?page={page}&statuses=1&limit=500&sortBy=createDateTime")).call().unwrap_or_else(|e| {
                error!("Failed to get orders {}", e);
                panic!("Failed to get orders {}", e)
            }).into_json().unwrap_or_else(|e| {
                error!("Failed to parse orders {}", e);
                panic!("Failed to parse orders {}", e)
            });

            orders.extend(orders_v4);
        }

        for order in orders {
            if order.data.receiver != Address::ZERO {
                continue;
            }
            let pools_order = Arc::new(Pools::OneInch(order.clone()));
            let limit = orders_map
                .entry((order.data.taker_asset, order.data.maker_asset))
                .or_insert(pools_order.clone());

            if limit.rate((0, 1)) < pools_order.rate((0, 1))
                || limit.reserves().last() < pools_order.reserves().last()
            {
                *limit = pools_order;
            }
        }

        let mut counter = 0;
        {
            let mut graph = GRAPH.write().unwrap_or_else(|e| {
                error!("Failed to get graph {}", e);
                panic!("Failed to get graph {}", e)
            });
            for ((from, to), order) in orders_map {
                let route = if from < to { (0, 1) } else { (1, 0) };
                graph
                    .insert_edge_by_keys(order, &from, &to, route)
                    .unwrap_or_else(|e| {
                        error!("Failed to insert edge main {}", e);
                        panic!("Failed to insert edge main {}", e)
                    });
                counter += 1;
            }
        }

        info!(elapsed = ?time.elapsed(), ?counter, "Loaded Limit Orders");
        construct_discord_message(
            &ureq::Agent::new(),
            &DiscordLog::UpdatePools,
            format!("1inch Orders loaded: {}", counter),
        );
    });

    // Next Block bundler
    if *NEXTBLOCK_ENABLED {
        tokio::spawn(async move {
            while !*SYNCED.read().unwrap_or_else(|e| {
                error!("Failed to get sync {}", e);
                panic!("Failed to get sync {}", e)
            }) {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let routes = ROUTES.read().unwrap();

            loop {
                for arb in routes.values() {
                    searcher_handler(
                        arb.clone(),
                        Default::default(),
                        Opened_DB.clone(),
                        Default::default(),
                        bundle_sender.clone(),
                        Default::default(),
                    );
                }

                error!("Next Block Bundler Done");
            }
        });
    }

    // New pool detection
    // tokio::spawn(setup_new_pools_detector(ipc_provider.clone()));
    while let Ok(new_block) = stream_mined_blocks.recv().await {
        let now = Instant::now();
        {
            {
                MIXER.write()?.clear();
                TRANSACTION_TRACKER.lock()?.clear();
                NONCE_MANAGER.write()?.take();
                NextBlocks.lock()?.clear();
            }

            let stage_two = Instant::now();
            let latest_block = new_block.header.number.ok_or("Failed to get last block")?;
            let (db_latest_block, _) = Opened_DB
                .clone()
                .tx()?
                .new_cursor::<tables::BlockBodyIndices>()?
                .last()?
                .ok_or("Failed to get last block DB")?;

            // Check if we are behind
            if latest_block < db_latest_block {
                warn!(elapsed= ?stage_two.elapsed(), ?last_synced, ?latest_block, ?db_latest_block, "Behind");
                continue;
            }

            let get_state = Instant::now();
            {
                let mut state_provider = LatestState.write()?;
                *state_provider = Arc::new(StateProviderDatabase::new(Box::new(
                    LatestStateProvider::new(
                        Opened_DB.clone().tx()?,
                        StaticFileProvider::read_only(Path::new(&format!(
                            "{}/static_files",
                            *DB_PATH_DIR
                        )))?,
                    ),
                )));
            }
            let state_provider = LatestState.read()?.clone();
            info!(stage_two = ?get_state.elapsed(), ?db_latest_block, "Stage getting state");

            let set_block_nonce = Instant::now();
            *NONCE_MANAGER.write()? = state_provider.account_nonce(WALLET_SIGNER.address())?;
            info!(stage_two = ?set_block_nonce.elapsed(), "Stage setting block nonce");

            let stage_storage = Instant::now();
            let mut storage = POOL_STORAGE.write()?;
            info!(stage_two = ?stage_storage.elapsed(), "Stage stage_storage");

            if last_synced == 0 {
                let now = Instant::now();
                let mut to_be_removed = Vec::new();
                for (address, pool) in storage.iter_mut() {
                    match state_provider.storage_ref(*address, pool.reserve_slot())? {
                        U256::ZERO => to_be_removed.push(*address),
                        value => {
                            let mut update_pool = pool.actual_pool();
                            update_pool.update_balances_state(&value.into());
                            // Filter Out empty pools
                            match &update_pool {
                                Pools::V2(v2) => {
                                    if (v2.token0 == WETH_ADDRESS && v2.reserve0 < minimum_liq)
                                        || (v2.token1 == WETH_ADDRESS && v2.reserve1 < minimum_liq)
                                        || v2.reserve0 == Some(U256::ZERO)
                                        || v2.reserve1 == Some(U256::ZERO)
                                    {
                                        to_be_removed.push(v2.address);
                                    }
                                }
                                Pools::V3(v3) => {
                                    if v3.sqrt_price.is_none() {
                                        to_be_removed.push(v3.address);
                                    }
                                }
                                Pools::DoDo(dodo) => {
                                    if (dodo.base_token == WETH_ADDRESS
                                        && dodo.base_reserve < minimum_liq)
                                        || (dodo.quote_token == WETH_ADDRESS
                                            && dodo.quote_reserve < minimum_liq)
                                        || dodo.base_reserve == Some(U256::ZERO)
                                        || dodo.quote_reserve == Some(U256::ZERO)
                                    {
                                        to_be_removed.push(dodo.address);
                                    }
                                }
                                _ => {}
                            }
                            *pool = Arc::new(update_pool)
                        }
                    }
                }
                for address in to_be_removed {
                    storage.remove(&address);
                }

                // Set the graph
                let mut graph = GRAPH.write()?;
                graph.from_storage(&storage.values().cloned().collect::<Vec<_>>()[..]);
                last_synced = latest_block - 1;
                *ROUTES.write()? = graph.generate_all_routes(WETH_ADDRESS, 3);
                *SYNCED.write()? = true;
                info!(initial_pool_update=?now.elapsed(), pools=?storage.len(), ?last_synced)
            }

            last_synced = latest_block;
        }

        info!(bn=new_block.header.number, elapse=?now.elapsed());
    }

    Ok(())
}
