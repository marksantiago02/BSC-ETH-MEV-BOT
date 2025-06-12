use crate::*;
use alloy::rpc::types::Transaction;
use reth::providers::{providers::StaticFileProvider, LatestStateProvider};
use reth::revm::database::StateProviderDatabase;
use reth_db::mdbx::DatabaseArguments;
use reth_db::{open_db_read_only, Database, DatabaseEnv};
use revm::primitives::{b256, Bytecode};
use serde::Deserializer;
use std::{path::Path, sync::RwLock, time::Duration};
use ureq::Agent;
use std::sync::Mutex;

pub const CHAIN_ID: u64 = 56;

// Common function selectors
pub const V2_LOG: B256 = b256!("0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9");
pub const V3_LOG: B256 = b256!("783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118");

// MEV (Rustitrage) Contract
pub static MEV_ADDRESS: Address = address!("368dF74E00D36FDD717B0094045b710000DE00C6");
pub const MEV_SLOT_WETH: &str =
    "84930398699318396925179693117496578481646381870277157171424212974640293258716";

// Searcher Contract
pub static SEARCHER_ADDRESS: Address = address!("56625000b706f187002000007ccb28ffeb006f00");
pub const SEARCHER_SLOT_WETH: &str =
    "69353662601361346910526914951202745723779464966833356848182111469031178806463";
pub const SEARCHER_HASH: B256 =
    b256!("03ff0e9c5ddc8dde9838ede291ae9ea5b23a1657fc92f8966dbd0fd3239682fb");

// Common tokens
pub static WETH_ADDRESS: Address = address!("bb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c");
pub static USDC_ADDRESS: Address = address!("55d398326f99059fF775485246999027B3197955");
pub static USDT_ADDRESS: Address = address!("dac17f958d2ee523a2206206994597c13d831ec7");
pub static BALANCER_VAULT: Address = address!("BA12222222228d8Ba445958a75a0704d566BF2C8");

// Approved tokens catch
lazy_static! {
    pub static ref MIXER: RwLock<VictimMixer> = RwLock::new(HashMap::new());
    pub static ref NextBlocks: Mutex<BTreeMap<U256, Vec<Rustitrage::Swap>>> =
        Mutex::new(BTreeMap::new());
    pub static ref POOL_STORAGE: RwLock<HashMap<Address, Arc<Pools>>> = RwLock::new(HashMap::new());
    pub static ref TRANSACTION_TRACKER: Mutex<HashSet<B256>> = Mutex::new(HashSet::default());
    pub static ref NONCE_MANAGER: RwLock<Option<u64>> = RwLock::new(None);
    pub static ref GRAPH: RwLock<DirectedGraph> = RwLock::new(DirectedGraph::default());
    pub static ref ROUTES: RwLock<HashMap<String, Vec<DirectedGraphEdge>>> =
        RwLock::new(HashMap::new());
    pub static ref SYNCED: RwLock<bool> = RwLock::new(false);
    pub static ref Opened_DB: Arc<DatabaseEnv> = Arc::new(
        open_db_read_only(
            Path::new(&format!("{}/db", *DB_PATH_DIR)),
            DatabaseArguments::default().with_max_read_transaction_duration(Some(
                reth_libmdbx::MaxReadTransactionDuration::Set(Duration::from_secs(60)),
            )),
        )
        .unwrap()
    );
    pub static ref LatestState: Arc<RwLock<ArcStateProvider>> = Arc::new(RwLock::new(Arc::new(
        StateProviderDatabase::new(Box::new(LatestStateProvider::new(
            Opened_DB.clone().tx().unwrap(),
            StaticFileProvider::read_only(Path::new(&format!("{}/static_files", *DB_PATH_DIR)))
                .unwrap(),
        ),))
    )));
    pub static ref SEARCHER_BYTE_CODE: Bytecode = Bytecode::new_raw(
        include_str!("./src/metadata/bytecode/Searcher.bin")
            .parse::<Bytes>()
            .unwrap_or_else(|e| {
                panic!("Failed to parse bytecode: {:?}", e);
            })
    );
}

// Construct Discord Bot message
#[derive(Debug, Deserialize, Serialize)]
pub enum DiscordLog {
    NewPool,
    ArbDetected,
    SwapDetected,
    UpdatePools,
    PrivateFlow,
}

pub fn construct_discord_message<D>(agent: &Agent, mode: &DiscordLog, message: D)
where
    D: Serialize,
{
    let value = serde_json::json!({
        "content": "",
        "embeds": [{
            "color": 5814783,
            "fields": [{
                "name": mode,
                "value": message
            }]
            }
        ],
        "username": "hades",
        "avatar_url": "https://metapro.app/img/icon-128.png"
    });

    // It's written this way so we could use PATCH/DELETE in the future
    let client = match mode {
        DiscordLog::NewPool => agent.post(DISCORD_NEW_POOL.as_ref()),
        DiscordLog::ArbDetected => agent.post(DISCORD_DETECTED_ARB.as_ref()),
        DiscordLog::SwapDetected => agent.post(DISCORD_DETECTED_SWAP.as_ref()),
        DiscordLog::UpdatePools => agent.post(DISCORD_STORAGE.as_ref()),
        DiscordLog::PrivateFlow => agent.post(DISCORD_PRIVATE_FLOW.as_ref()),
    };

    client.send_json(&value).ok();
}

// Calculate the amount out for a given pool
pub fn get_amount_out(amount_in: U256, reserve_in: U256, reserve_out: U256) -> U256 {
    let amount_in_with_fee = amount_in * U256::from(997);
    let numerator = amount_in_with_fee * reserve_out;
    let denominator = reserve_in * U256::from(1000) + amount_in_with_fee;

    numerator / denominator
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcRequest {
    pub params: Params,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Params {
    pub result: ResultData,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ResultData {
    pub transactions: Option<Vec<TransactionBlox>>,
    pub tx_contents: Option<Arc<Transaction>>,
    pub backrunme_config: Option<BackrunmeConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransactionBlox {
    pub tx_contents: Arc<Transaction>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BackrunmeConfig {
    pub miner_share: f64,
    pub bloxroute_share: f64,
    pub searcher_share: f64,
    pub user_share: f64,
    pub additional_wallet: Option<Address>,
}

pub fn deserialize_f64_from_str<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)?
        .parse::<f64>()
        .map_err(serde::de::Error::custom)
}

pub fn deserialize_signature_from_str<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
where
    D: Deserializer<'de>,
{
    let mut string = String::deserialize(deserializer)?;

    if let Some(stripped) = string.strip_prefix("0x") {
        string = stripped.to_string();
    }

    if string.len() % 2 == 1 {
        string.insert(0, '0');
    }

    hex::decode(&string)
        .map(Bytes::from)
        .map_err(serde::de::Error::custom)
}

// Verbose Types
pub type VictimMixer =
    HashMap<(Address, (usize, usize)), HashMap<Address, (Arc<Transaction>, Arc<String>)>>;
pub type ArcStateProvider =
    Arc<StateProviderDatabase<Box<LatestStateProvider<reth_db::mdbx::tx::Tx<reth_libmdbx::RO>>>>>;
pub type FoundVictim = (Address, (usize, usize), Arc<Transaction>, Arc<String>);
