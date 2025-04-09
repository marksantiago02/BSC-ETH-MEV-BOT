use crate::*;
use alloy::rpc::types::Transaction;
use reth_db::cursor::DbCursorRO;
use reth_db::{tables, Database};
use revm::primitives::{Account, EvmStorageSlot, ExecutionResult, ResultAndState};
use revm::DatabaseCommit;
use revm::{db::State, Evm};

pub type PendingTransactionResult =
    Option<(HashMap<Address, Account>, HashMap<Address, Arc<Pools>>)>;
pub fn pending_transaction_state(
    transactions: &[Arc<Transaction>],
    signed: Option<Arc<String>>,
    sender: Option<Arc<Sender<FoundVictim>>>,
) -> PendingTransactionResult {
    // create a cache db
    let mut mega_state: HashMap<Address, Account> = HashMap::new();

    // Set up the DB
    let db = Opened_DB.clone();
    let (db_latest_block, _) = db
        .clone()
        .tx()
        .unwrap()
        .new_cursor::<tables::BlockBodyIndices>()
        .unwrap()
        .last()
        .unwrap()
        .unwrap();
    let db = LatestState.read().unwrap().clone();
    let mut cache_db = State::builder().with_database_ref(db).build();

    // create an evm instance
    let mut evm = Evm::builder()
        .with_db(&mut cache_db)
        .with_spec_id(revm::primitives::SpecId::CANCUN)
        .modify_cfg_env(|c| {
            c.chain_id = CHAIN_ID;
        })
        .modify_block_env(|b| {
            b.basefee = U256::ZERO;
            b.number = U256::from(db_latest_block);
        })
        .build();

    for transaction in transactions {
        // set the transaction data
        let tx = evm.tx_mut();
        tx.caller = transaction.from;
        tx.transact_to = transaction.to.into();
        tx.value = transaction.value;
        tx.data = transaction.input.clone();

        // execute the transaction
        match evm.transact() {
            Ok(ResultAndState {
                result: ExecutionResult::Success { .. },
                state,
            }) => {
                // Update the local mega state
                for (address, account) in state.clone() {
                    let c = mega_state.entry(address).or_default();
                    c.info = account.info;
                    c.status = account.status;
                    for (slot, value) in account.storage {
                        c.storage.insert(slot, value);
                    }
                }

                // Commit changes to the cache db
                evm.context.evm.db.commit(state);
            }
            _ => return None,
        }
    }

    let pool_storage = POOL_STORAGE.read().unwrap_or_else(|e| {
        error!("Error reading pool storage: {:?}", e);
        panic!("Error reading pool storage: {:?}", e);
    });
    let mut useful_pools = HashMap::new();

    // iterate over the state and extract useful pools
    for (address, account) in &mega_state {
        if let Some(pool) = pool_storage.get(address) {
            // if the pool is present in the state, update the balances
            let update_pool = match account.storage.get(&pool.reserve_slot()) {
                Some(EvmStorageSlot { present_value, .. }) => {
                    let mut update_pool = pool.actual_pool();
                    update_pool.update_balances_state(&B256::from(*present_value));
                    Arc::new(update_pool)
                }
                _ => continue,
            };

            // if we have a sender, send the transaction to the sender
            if let Some(sender) = sender.as_ref() {
                let route = if update_pool.rate((0, 1)) > pool.rate((0, 1)) {
                    (1, 0)
                } else {
                    (0, 1)
                };
                sender
                    .send((
                        *address,
                        route,
                        transactions[0].clone(),
                        signed.as_ref()?.clone(),
                    ))
                    .ok()?;
            }

            // insert the pool into the useful pools
            useful_pools.insert(*address, update_pool);
        }
    }

    // if we have useful pools, return the state and the pools
    if !useful_pools.is_empty() {
        return Some((mega_state, useful_pools));
    }
    None
}
