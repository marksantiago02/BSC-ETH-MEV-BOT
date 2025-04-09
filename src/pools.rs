use crate::*;
use alloy::primitives::U512;
use alloy::rpc::types::Filter;
use serde_json::json;
use std::io::Write;
use std::{collections::BTreeSet, fs::OpenOptions, panic, time::Duration};

// TODO Most of these protocols use the same pool struct
// there is no need for specifying the pool type
// SHOULD BE GENERIC
#[derive(Default, Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum PoolType {
    #[default]
    INVALID = -1,
    #[serde(rename = "UNISWAPV3")]
    V3 = 2,
    V2 = 1,
    DoDo = 3,
    OneInch = 91,
    ZeroEx = 92,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum PoolFactory {
    #[serde(
        rename = "0xc805a432342b1e09456d64170aaafcd93fce97e4",
        alias = "0x8a1e9d3aebbbd5ba2a64d3355a48dd5e9b511256"
    )]
    BurgurSwap,
    #[serde(rename = "0x001a1a9a42c35e16561b415d0575c3789b85c038")]
    SafeSwap,
    #[serde(rename = "0x4693b62e5fc9c0a45f89d62e6300a03c85f43137")]
    TreatSwap,
    #[serde(rename = "0x82fa7b2ce2a76c7888a9d3b0a81e0b2ecfd8d40c")]
    UnchainX,
    #[serde(rename = "0x306F06C147f064A010530292A1EB6737c3e378e4")]
    Thena,
    #[serde(
        rename = "0x5f1dddbf348ac2fbe22a163e30f99f9ece3dd50a",
        alias = "0xc7a590291e07b9fe9e64b86c58fd8fc764308c4a"
    )]
    KyberV3,
    Standard(String),
}

impl Default for PoolFactory {
    fn default() -> Self {
        PoolFactory::Standard(String::default())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Pools {
    #[default]
    INVALID,
    Mock(u8),
    V3(V3),
    V2(V2),
    OneInch(OneInch),
    ZeroEx(Box<ZeroEx>),
    DoDo(DoDo),
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct V2 {
    pub address: Address,
    pub token0: Address,
    pub token1: Address,
    pub reserve0: Option<U256>,
    pub reserve1: Option<U256>,
    pub fee: U256,
    pub dex: PoolType,
    pub factory: PoolFactory,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct V3 {
    pub address: Address,
    pub token0: Address,
    pub token1: Address,
    pub reserve0: Option<U256>,
    pub reserve1: Option<U256>,
    pub sqrt_price: Option<f64>,
    #[serde(rename = "feeTier")]
    pub fee: U256,
    pub dex: PoolType,
    pub factory: PoolFactory,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OneInch {
    #[serde(deserialize_with = "deserialize_signature_from_str")]
    pub signature: Bytes,
    pub order_hash: B256,
    pub remaining_maker_amount: U256,
    pub maker_balance: U256,
    pub maker_allowance: U256,
    pub data: OneInchData,
    #[serde(deserialize_with = "deserialize_f64_from_str")]
    pub maker_rate: f64,
    #[serde(deserialize_with = "deserialize_f64_from_str")]
    pub taker_rate: f64,
    pub is_maker_contract: bool,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OneInchData {
    pub maker_asset: Address,
    pub taker_asset: Address,
    pub maker: Address,
    pub receiver: Address,
    pub making_amount: U256,
    pub taking_amount: U256,
    pub salt: U256,

    // Optional fields for v4
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<Bytes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maker_traits: Option<B256>,

    // Optional fields for v3
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_sender: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactions: Option<Bytes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offsets: Option<U256>,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ZeroEx {
    pub order: ZeroExOrder,
    pub meta_data: ZeroExMetaData,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ZeroExOrder {
    pub signature: ZeroExSignature,
    pub sender: Address,
    pub maker: Address,
    pub taker: Address,
    pub taker_token_fee_amount: U256,
    pub maker_amount: U256,
    pub taker_amount: U256,
    pub maker_token: Address,
    pub taker_token: Address,
    pub salt: U256,
    pub verifying_contract: Address,
    pub fee_recipient: Address,
    pub pool: B256,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ZeroExSignature {
    pub signature_type: u8,
    pub r: B256,
    pub s: B256,
    pub v: u8,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ZeroExMetaData {
    pub order_hash: B256,
    pub remaining_fillable_taker_amount: U256,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DoDo {
    pub address: Address,
    pub base_token: Address,
    pub quote_token: Address,
    pub base_reserve: Option<U256>,
    pub quote_reserve: Option<U256>,
    pub factory: PoolFactory,
}

pub trait Pool<T> {
    fn actual_pool(&self) -> Pools;
    fn address(&self) -> Address;
    fn tokens(&self) -> BTreeSet<T>;
    fn reserves(&self) -> Vec<U256>;
    fn update_balances_state(&mut self, reserves: &B256);
    fn rate(&self, direction: (usize, usize)) -> Option<f64>;
    fn fee(&self) -> f64;
    fn dex(&self) -> PoolType;
    fn reserve_slot(&self) -> U256;
}

impl Pool<Address> for Pools {
    fn actual_pool(&self) -> Pools {
        match self {
            Pools::V3(pool) => Pools::V3(pool.clone()),
            Pools::V2(pool) => Pools::V2(pool.clone()),
            Pools::OneInch(pool) => Pools::OneInch(pool.clone()),
            Pools::DoDo(pool) => Pools::DoDo(pool.clone()),
            _ => Pools::INVALID,
        }
    }

    fn dex(&self) -> PoolType {
        match self {
            Pools::V2(_) => PoolType::V2,
            Pools::V3(_) => PoolType::V3,
            Pools::OneInch(_) => PoolType::OneInch,
            Pools::DoDo(_) => PoolType::DoDo,
            _ => PoolType::INVALID,
        }
    }

    fn fee(&self) -> f64 {
        match self {
            Pools::V2(_) => 0.997,
            Pools::V3(pool) => 1.0 - pool.fee.to::<u64>() as f64 / 1_000_000.0,
            _ => 1.0,
        }
    }

    fn address(&self) -> Address {
        match self {
            Pools::V3(pool) => pool.address,
            Pools::V2(pool) => pool.address,
            Pools::OneInch(pool) => Address::from_slice(&pool.order_hash[..20]),
            Pools::DoDo(pool) => pool.address,
            _ => Address::default(),
        }
    }

    fn tokens(&self) -> BTreeSet<Address> {
        match self {
            Pools::V3(pool) => BTreeSet::from([pool.token0, pool.token1]),
            Pools::V2(pool) => BTreeSet::from([pool.token0, pool.token1]),
            Pools::OneInch(pool) => BTreeSet::from([pool.data.maker_asset, pool.data.taker_asset]),
            Pools::DoDo(pool) => BTreeSet::from([pool.base_token, pool.quote_token]),
            _ => BTreeSet::new(),
        }
    }

    fn reserves(&self) -> Vec<U256> {
        match self {
            Pools::V2(pool) => vec![
                pool.reserve0.unwrap_or_else(|| {
                    error!("Reserve0 is missing for pool {:?}", pool.address);
                    panic!("Reserve0 is missing for pool {:?}", pool.address);
                }),
                pool.reserve1.unwrap_or_else(|| {
                    error!("Reserve1 is missing for pool {:?}", pool.address);
                    panic!("Reserve1 is missing for pool {:?}", pool.address);
                }),
            ],
            Pools::DoDo(pool) => vec![
                pool.base_reserve.unwrap_or_else(|| {
                    error!("Base reserve is missing for pool {:?}", pool.address);
                    panic!("Base reserve is missing for pool {:?}", pool.address);
                }),
                pool.quote_reserve.unwrap_or_else(|| {
                    error!("Quote reserve is missing for pool {:?}", pool.address);
                    panic!("Quote reserve is missing for pool {:?}", pool.address);
                }),
            ],
            Pools::OneInch(pool) => vec![pool.data.making_amount, pool.data.taking_amount],
            _ => Vec::new(),
        }
    }

    fn rate(&self, direction: (usize, usize)) -> Option<f64> {
        match self {
            Pools::V3(pool) => match (pool.sqrt_price, direction) {
                (Some(price), (0, 1)) => Some(price),
                (Some(price), (1, 0)) => Some(1.0 / price),
                _ => None,
            },
            Pools::V2(pool) => match (pool.reserve0, pool.reserve1, direction) {
                (Some(reserve0), Some(reserve1), (0, 1)) => {
                    Some(f64::from(reserve1) / f64::from(reserve0))
                }
                (Some(reserve0), Some(reserve1), (1, 0)) => {
                    Some(f64::from(reserve0) / f64::from(reserve1))
                }
                _ => None,
            },
            Pools::OneInch(pool) => Some(pool.taker_rate),
            Pools::DoDo(pool) => match (pool.base_reserve, pool.quote_reserve, direction) {
                // SellBase
                (Some(base_reserve), Some(quote_reserve), (0, 1)) => {
                    Some(f64::from(quote_reserve) / f64::from(base_reserve) * 1.012)
                }
                // SellQuote
                (Some(base_reserve), Some(quote_reserve), (1, 0)) => {
                    Some(f64::from(base_reserve) / f64::from(quote_reserve) * 0.988)
                }
                _ => None,
            },
            _ => None,
        }
    }

    // using state overrides from traces
    // TODO this is ugly needs to be rewritten
    fn update_balances_state(&mut self, reserves: &B256) {
        match self {
            Pools::V2(pool) => {
                // Extract slices based on the provided index ranges.
                let r1 = &reserves[4..18];
                let r0 = &reserves[18..32];

                // Convert slices to hex strings and then parse them as U256.
                pool.reserve0 = U256::from_str_radix(&r0.encode_hex(), 16).ok();
                pool.reserve1 = U256::from_str_radix(&r1.encode_hex(), 16).ok();
            }
            Pools::V3(pool) => {
                let square_price_x_96 =
                    U512::from_str_radix(&(&reserves[reserves.len() - 20..]).encode_hex(), 16)
                        .unwrap_or_else(|e| {
                            error!("Error parsing square_price_x_96: {:?}", e);
                            panic!("Error parsing square_price_x_96: {:?}", e);
                        });

                if square_price_x_96 == U512::from(4295128740u64)
                    || square_price_x_96
                        == U512::from_str("1461446703485210103287273052203988822378723970341")
                            .unwrap()
                {
                    return pool.sqrt_price = None;
                }

                let denom =
                    U512::from_str("6277101735386680763835789423207666416102355444464034512896")
                        .unwrap_or_else(|e| {
                            error!("Error parsing denom: {:?}", e);
                            panic!("Error parsing denom: {:?}", e);
                        });

                let pow_two = square_price_x_96.pow(U512::from(2));

                let price = if pow_two < denom {
                    let calc = denom.div_rem(pow_two);
                    1.0 / format!("{}.{}", calc.0, calc.1)
                        .parse::<f64>()
                        .unwrap_or_else(|e| {
                            error!("Error parsing price V3: {:?}", e);
                            panic!("Error parsing price V3: {:?}", e);
                        })
                } else {
                    let calc = pow_two.div_rem(denom);
                    format!("{}.{}", calc.0, calc.1)
                        .parse::<f64>()
                        .unwrap_or_else(|e| {
                            error!("Error parsing price V3: {:?}", e);
                            panic!("Error parsing price V3: {:?}", e);
                        })
                };

                pool.sqrt_price = Some(price);
            }
            Pools::DoDo(pool) => {
                let r1 = &reserves[4..18];
                let r0 = &reserves[18..32];

                pool.base_reserve = U256::from_str_radix(&r0.encode_hex(), 16).ok();
                pool.quote_reserve = U256::from_str_radix(&r1.encode_hex(), 16).ok();
            }
            _ => {}
        }
    }

    fn reserve_slot(&self) -> U256 {
        match self {
            Pools::V2(pool) => match pool.factory {
                PoolFactory::Standard(_) => U256::from(8),
                PoolFactory::TreatSwap => U256::from(9),
                PoolFactory::SafeSwap => U256::from(10),
                PoolFactory::BurgurSwap => U256::from(13),
                _ => panic!("Invalid pool factory"),
            },
            Pools::V3(pool) => match pool.factory {
                PoolFactory::Standard(_) => U256::ZERO,
                PoolFactory::UnchainX => U256::from(1),
                PoolFactory::Thena => U256::from(2),
                PoolFactory::KyberV3 => U256::from(3),
                _ => panic!("Invalid pool factory"),
            },
            Pools::DoDo(pool) => match pool.factory {
                PoolFactory::Standard(_) => U256::from(3),
                _ => panic!("Invalid pool factory"),
            },
            _ => panic!("Invalid pool type"),
        }
    }
}

pub fn load_pools<P>(pool_type: PoolType) -> Result<Vec<P>, Box<dyn std::error::Error>>
where
    P: for<'de> Deserialize<'de>,
{
    // Load file from POOLS_DATA_DIR path
    let file = File::open(format!("{}{:?}.json", *POOLS_DATA_DIR, pool_type))?;
    let reader = BufReader::new(file);

    // Deserialize JSON
    let result: Vec<P> = serde_json::from_reader(reader)?;

    Ok(result)
}

pub async fn setup_new_pools_detector(
    provider: RootProvider<PubSubFrontend>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(60))
        .timeout_write(Duration::from_secs(60))
        .build();

    // V3 Pools
    let mut f = OpenOptions::new()
        .append(true)
        .create(true)
        .open("./V3_detections.json")?;

    // V2 Pools
    let mut f2 = OpenOptions::new()
        .append(true)
        .create(true)
        .open("./V2_detections.json")?;

    // Log Subscription stream
    let mut steam_new_pool = provider
        .subscribe_logs(
            &Filter::new()
                .event_signature(V2_LOG)
                .from_block(BlockNumberOrTag::Finalized)
                .to_block(BlockNumberOrTag::Finalized),
        )
        .await?;

    loop {
        if let Ok(log) = steam_new_pool.recv().await {
            let factory = log.address();
            let topics = log.topics();
            let mut storage = POOL_STORAGE
                .write()
                .map_err(|e| format!("Fetching Pools Stroage Failed {:?}", e))?;

            let (pool_type, pool) = match topics[0] {
                V2_LOG => {
                    let pool = json!({
                        "protocol": "Standard",
                        "dex": PoolType::V2,
                        "token0": Address::from_slice(&topics[1][12..]),
                        "token1": Address::from_slice(&topics[2][12..]),
                        "fee": "3",
                        "address": log.data().data.slice(12..32),
                        "factory": factory,
                    });
                    let v2: V2 = serde_json::from_value(pool.clone())?;
                    POOL_STORAGE
                        .write()
                        .unwrap()
                        .insert(v2.address, Pools::V2(v2).into());

                    (&mut f2, pool)
                }
                V3_LOG => {
                    let pool = json!({
                        "protocol": "Standard",
                        "dex": PoolType::V3,
                        "token0": Address::from_slice(&topics[1][12..]),
                        "token1": Address::from_slice(&topics[2][12..]),
                        "feeTier": log.topics(),
                        "address": log.data().data.slice(44..),
                        "factory": factory,
                    });

                    let v3: V3 = serde_json::from_value(pool.clone())?;
                    storage.insert(v3.address, Pools::V3(v3).into());

                    (&mut f, pool)
                }
                _ => continue,
            };

            construct_discord_message(&agent, &DiscordLog::NewPool, pool.to_string());
            writeln!(pool_type, "{},", pool).unwrap();
        }
    }
}

#[cfg(test)]
mod tests_pools {

    use super::*;
    use alloy::primitives::U256;

    #[test]
    fn dodo_v2() {
        let loaded: Vec<DoDo> = load_pools(PoolType::DoDo).unwrap();

        assert_ne!(loaded.len(), 0);

        let json_response = r#"{
            "baseToken": "0x4fa7163e153419e0e1064e418dd7a99314ed27b6",
            "address": "0x912d765fe41059a1b68b687bfcbf49d7ce5c7f7e",
            "quoteToken": "0xe9e7cea3dedca5984780bafc599bd69add087d56",
            "factory": "DVM"
        }"#;

        let pool = Pools::DoDo(serde_json::from_str(json_response).unwrap());

        dbg!(pool.reserve_slot());
        dbg!(pool.tokens());
    }

    #[test]
    fn zero_ex() {
        let json_response = r#"
                {
            "order": {
                "signature": {
                "signatureType": 2,
                "r": "0xd23175b146c0ed8af9131e7e34e1f54c547fd59c86cccface58aeb89f9704ffc",
                "s": "0x0b41d8bffe3239f2eeb7f2285645ba70e22b2f53ce861ed980d530d5ee58c606",
                "v": 28
                },
                "sender": "0x0000000000000000000000000000000000000000",
                "maker": "0xb7c91a9efd8b0f60541cc797741de50c657a0cf3",
                "taker": "0x0000000000000000000000000000000000000000",
                "takerTokenFeeAmount": "48605000000000",
                "makerAmount": "555510000000000000000",
                "takerAmount": "19442000000000000",
                "makerToken": "0xfd54e565e6de7509b07cdba5769178045f212530",
                "takerToken": "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c",
                "salt": "1726728998",
                "verifyingContract": "0xdef1c0ded9bec7f1a1670819833240f027b25eff",
                "feeRecipient": "0x9b858be6e3047d88820f439b240deac2418a2551",
                "expiry": "1729148198",
                "chainId": 56,
                "pool": "0x0000000000000000000000000000000000000000000000000000000000000000"
            },
            "metaData": {
                "orderHash": "0x0420b619c7dda274ee26023cc7c7520f10cc012148eb39c064d387d4408885e7",
                "remainingFillableTakerAmount": "19442000000000000",
                "createdAt": "2024-09-19T06:56:44.858Z"
            }
        }"#;

        let orders: ZeroEx = serde_json::from_str(json_response).unwrap();
        dbg!(orders);
    }

    #[test]
    fn one_inch_limit_order() {
        let json_response = r#"{
            "signature": "0xdc2a09e261101c4d8cef3c3b84bc22f89bdb93652785d4843f4d25833537fcc043d0e78e343a09e2616379e04bae78b0ebd57642adb3fa4f6f8d82839c31c2671b",
            "orderHash": "0x92c2058a7d848e4f93ee4552e1571d47f4daa64aea14ce0fb8e7bdef8e3e8339",
            "createDateTime": "2024-09-12T21:38:27.209Z",
            "remainingMakerAmount": "1342393370000000000",
            "makerBalance": "0",
            "makerAllowance": "115792089237316195423570985008687907853269984665640564039457584007913129639935",
            "data": {
            "makerAsset": "0x55d398326f99059ff775485246999027b3197955",
            "takerAsset": "0x76a797a59ba2c17726896976b7b3747bfd1d220f",
            "salt": "29839716993534142955284115661",
            "receiver": "0x0000000000000000000000000000000000000000",
            "makingAmount": "1342393370000000000",
            "takingAmount": "240017000",
            "maker": "0x6845170662bd6a39431894af3829440b7296eae1",
            "extension": "0x",
            "makerTraits": "0x40000000000000000000000000000000000066e4b0cf00000000000000000000"
            },
            "makerRate": "0.000000000178797814",
            "takerRate": "5592909543.907306565784923568",
            "isMakerContract": false,
            "orderInvalidReason": null
        }"#;

        let orders: OneInch = serde_json::from_str(json_response).unwrap();
        let x = Pools::OneInch(orders);
        dbg!(x.rate((0, 1)));
        dbg!(x.rate((1, 0)));
    }

    #[test]
    fn test_load_v3() {
        let loaded: Vec<V3> = load_pools(PoolType::V3).unwrap();

        assert_ne!(loaded.len(), 0);
    }

    #[test]
    fn test_rate() {
        let mut pool = Pools::V3(V3 {
            address: Address::default(),
            token0: WETH_ADDRESS,
            token1: USDC_ADDRESS,
            reserve0: Some(U256::from(1)),
            reserve1: Some(U256::from(3000)),
            fee: U256::from(3000),
            dex: PoolType::V3,
            factory: PoolFactory::Standard(String::default()),
            sqrt_price: None,
        });

        let v =
            U256::from_str("105313898606653067387525734467736646296745144938568372480263697350")
                .unwrap();

        pool.update_balances_state(&v.into());
        assert_eq!(pool.rate((0, 1)).unwrap(), 34.119870451746344);
        assert_eq!(pool.rate((1, 0)).unwrap(), 0.02930843484339248);

        let mut v3 = Pools::V3(V3::default());

        v3.update_balances_state(
            &U256::from_str("105313898631029444427558040255038395939652525334840319963668280134")
                .unwrap()
                .into(),
        );

        assert_eq!(v3.rate((0, 1)).unwrap(), 0.0018583400297326042);
        assert_eq!(v3.rate((1, 0)).unwrap(), 538.114652862474);
    }
}
