use crate::*;
use alloy::sol_types::SolValue;
use Searcher::Swap;

// Define a struct to represent a directed graph edge
#[derive(Default, Debug, Clone, PartialEq)]
pub struct DirectedGraphEdge {
    pub pool: Arc<Pools>,
    pub to: Address,
    pub route: (usize, usize),
}

// Define a struct to represent the entire directed graph
#[derive(Default, Debug, PartialEq, Clone)]
pub struct DirectedGraph(HashMap<Address, HashMap<Address, HashMap<Address, DirectedGraphEdge>>>);

impl DirectedGraphEdge {
    pub fn new(pool: Arc<Pools>, to: Address, route: (usize, usize)) -> Self {
        Self { pool, to, route }
    }

    pub fn weight(&self) -> f64 {
        self.pool.rate(self.route).unwrap_or_else(|| {
            error!("Failed Getting Weight");
            panic!("Failed Getting Weight")
        })
    }

    pub fn to_swap_mev(&self, amount_in: U256, amount_out: U256) -> Rustitrage::Swap {
        let (pool_type, pool, from, extra) = match self.pool.as_ref() {
            Pools::V2(v2) => (
                PoolType::V2,
                v2.address,
                *self
                    .pool
                    .tokens()
                    .iter()
                    .find(|&&token| token != self.to)
                    .unwrap_or_else(|| {
                        error!("Failed Getting the TO Token");
                        panic!("Failed Getting the TO Token")
                    }),
                None,
            ),
            Pools::V3(v3) => (
                PoolType::V3,
                v3.address,
                *self
                    .pool
                    .tokens()
                    .iter()
                    .find(|&&token| token != self.to)
                    .unwrap_or_else(|| {
                        error!("Failed Getting the TO Token");
                        panic!("Failed Getting the TO Token")
                    }),
                None,
            ),
            Pools::OneInch(one_inch) => (
                PoolType::OneInch,
                Address::ZERO,
                one_inch.data.taker_asset,
                {
                    let order = Order {
                        salt: one_inch.data.salt,
                        makerAsset: one_inch.data.maker_asset,
                        takerAsset: one_inch.data.taker_asset,
                        maker: one_inch.data.maker,
                        receiver: one_inch.data.receiver,
                        makingAmount: one_inch.data.making_amount,
                        takingAmount: one_inch.data.taking_amount,
                        makerTraits: one_inch
                            .data
                            .maker_traits
                            .unwrap_or_else(|| {
                                error!("Failed Getting the Maker Traits");
                                panic!("Failed Getting the Maker Traits")
                            })
                            .into(),
                    };

                    let r = B256::from_slice(&one_inch.signature[..32]);
                    let s = B256::from_slice(&one_inch.signature[32..64]);

                    Some((order, r, s).abi_encode().into())
                },
            ),
            Pools::DoDo(dodo) => (
                PoolType::DoDo,
                dodo.address,
                *self
                    .pool
                    .tokens()
                    .iter()
                    .find(|&&token| token != self.to)
                    .unwrap_or_else(|| {
                        error!("Failed Getting the Maker Traits");
                        panic!("Failed Getting the Maker Traits")
                    }),
                None,
            ),
            _ => todo!(),
        };

        Rustitrage::Swap {
            dex: pool_type as u8,
            pool,
            from,
            to: self.to,
            zeroForOne: self.route.0 == 0,
            amountIn: amount_in,
            amountOut: amount_out,
            extra: extra.unwrap_or_default(),
        }
    }

    pub fn to_swap(&self) -> Swap {
        let (pool_type, pool, from, extra) = match self.pool.as_ref() {
            Pools::V2(v2) => (
                PoolType::V2,
                v2.address,
                *self
                    .pool
                    .tokens()
                    .iter()
                    .find(|&&token| token != self.to)
                    .unwrap_or_else(|| {
                        error!("Failed Getting the Maker Traits");
                        panic!("Failed Getting the Maker Traits")
                    }),
                None,
            ),
            Pools::V3(v3) => (
                PoolType::V3,
                v3.address,
                *self
                    .pool
                    .tokens()
                    .iter()
                    .find(|&&token| token != self.to)
                    .unwrap_or_else(|| {
                        error!("Failed Getting the Maker Traits");
                        panic!("Failed Getting the Maker Traits")
                    }),
                None,
            ),
            Pools::OneInch(one_inch) => (
                PoolType::OneInch,
                Address::ZERO,
                one_inch.data.taker_asset,
                {
                    let order = Order {
                        salt: one_inch.data.salt,
                        makerAsset: one_inch.data.maker_asset,
                        takerAsset: one_inch.data.taker_asset,
                        maker: one_inch.data.maker,
                        receiver: one_inch.data.receiver,
                        makingAmount: one_inch.data.making_amount,
                        takingAmount: one_inch.data.taking_amount,
                        makerTraits: one_inch.data.maker_traits.unwrap().into(),
                    };

                    let r = B256::from_slice(&one_inch.signature[..32]);
                    let s = B256::from_slice(&one_inch.signature[32..64]);

                    Some((order, r, s).abi_encode().into())
                },
            ),
            Pools::DoDo(dodo) => (
                PoolType::DoDo,
                dodo.address,
                *self
                    .pool
                    .tokens()
                    .iter()
                    .find(|&&token| token != self.to)
                    .unwrap_or_else(|| {
                        error!("Failed Getting the Maker Traits");
                        panic!("Failed Getting the Maker Traits")
                    }),
                None,
            ),
            _ => todo!(),
        };

        Searcher::Swap {
            dex: pool_type as i8,
            pool,
            from,
            to: self.to,
            fromIndex: if from < self.to { 0 } else { 1 },
            toIndex: if from < self.to { 1 } else { 0 },
            extra: extra.unwrap_or_default(),
        }
    }
}

impl DirectedGraph {
    pub fn insert_edge_by_keys(
        &mut self,
        pool: Arc<Pools>,
        from: &Address,
        to: &Address,
        route: (usize, usize),
    ) -> Result<(), String> {
        let edge_address = pool.address();
        let new_e = DirectedGraphEdge::new(pool, *to, route);

        let all_tos = self.0.entry(*from).or_default();
        let edge = all_tos.entry(*to).or_default();
        edge.insert(edge_address, new_e);

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn dfs(
        &self,
        tracker: Address,
        send: Sender<Option<(Vec<DirectedGraphEdge>, Address)>>,
        must_include: Vec<Address>,
        from: Address,
        visited: &mut Vec<Address>,
        root: Address,
        path: &mut Vec<DirectedGraphEdge>,
        max_depth: usize,
        count: &mut u64,
    ) {
        visited.push(from);
        for (&to_index, edge) in self.0.get(&from).unwrap_or(&HashMap::new()) {
            let e = edge.values().next().unwrap();
            if to_index == root && !path.is_empty() {
                let mut arb = e.weight();
                for p in path.iter() {
                    arb *= p.weight();
                }
                if arb > 1.00 && !path.iter().any(|x| x.pool.address() == e.pool.address()) {
                    let mut new_path = path.clone();
                    new_path.push(e.clone());
                    if must_include.is_empty()
                        || new_path
                            .iter()
                            .any(|x| must_include.iter().any(|y| y == &x.pool.address()))
                    {
                        *count += 1;
                        send.send(Some((new_path, tracker))).unwrap_or_else(|e| {
                            error!("Failed Sending New Path: {:?}", e);
                            panic!("Failed Sending New Path: {:?}", e)
                        });
                    }
                }
            } else if !visited.contains(&to_index) && path.len() < max_depth - 1 {
                path.push(e.clone());
                self.dfs(
                    tracker,
                    send.clone(),
                    must_include.clone(),
                    to_index,
                    visited,
                    root,
                    path,
                    max_depth,
                    count,
                );
                path.pop();
            }
        }
        visited.pop();
    }

    fn routes_dfs(
        &self,
        from: Address,
        visited: &mut HashSet<Address>,
        root: Address,
        path: &mut Vec<DirectedGraphEdge>,
        max_depth: usize,
        total: &mut HashMap<String, Vec<DirectedGraphEdge>>,
    ) {
        visited.insert(from);
        for (&to_index, edge) in self.0.get(&from).unwrap_or(&HashMap::new()) {
            'edges: for e in edge.values() {
                if to_index == root && !path.is_empty() {
                    let mut orders = String::default();
                    for p in path.iter() {
                        if p.pool.address() == e.pool.address() {
                            continue 'edges;
                        }

                        orders.push_str(&p.pool.address().encode_hex());
                    }
                    orders.push_str(&e.pool.address().encode_hex());

                    let mut new_path = path.clone();
                    new_path.push(e.clone());
                    total.insert(orders, new_path);
                } else if !visited.contains(&to_index) && path.len() < max_depth - 1 {
                    path.push(e.clone());
                    self.routes_dfs(to_index, visited, root, path, max_depth, total);
                    path.pop();
                }
            }
        }
        visited.remove(&from);
    }

    pub fn generate_all_routes(
        &self,
        from: Address,
        max_depth: usize,
    ) -> HashMap<String, Vec<DirectedGraphEdge>> {
        let mut visited = HashSet::with_capacity(self.0.len());
        let mut total_routes = HashMap::new();
        let mut path = Vec::with_capacity(max_depth);
        self.routes_dfs(
            from,
            &mut visited,
            from,
            &mut path,
            max_depth,
            &mut total_routes,
        );

        total_routes
    }

    pub fn find_arbs(
        &self,
        tracker: Address,
        send: Sender<Option<(Vec<DirectedGraphEdge>, Address)>>,
        from: &Address,
        max_depth: usize,
        starter: Vec<Address>,
    ) -> u64 {
        let mut count = 0;
        trace!(?tracker, "Rout Find search started");

        let mut visited = Vec::with_capacity(self.0.len());
        let mut path = Vec::with_capacity(max_depth);
        self.dfs(
            tracker,
            send.clone(),
            starter,
            *from,
            &mut visited,
            *from,
            &mut path,
            max_depth,
            &mut count,
        );

        count
    }

    pub fn from_storage(&mut self, storage: &[Arc<Pools>]) {
        for pool in storage {
            // Insert Node
            let tokens = pool.tokens().into_iter().collect::<Vec<_>>();
            if tokens.len() < 2 {
                continue;
            }
            for pair in &[[0, 1], [1, 0]] {
                let (from, to) = (tokens[pair[0]], tokens[pair[1]]);
                self.insert_edge_by_keys(pool.clone(), &from, &to, (pair[0], pair[1]))
                    .unwrap_or_else(|e| {
                        error!("Failed FROM_STORAGE Inserting Edge by Keys: {}", e);
                        panic!("Failed FROM_STORAGE Inserting Edge by Keys: {}", e)
                    });
            }
        }
    }

    pub fn update_edge(&mut self, pool: &Arc<Pools>) {
        let tokens = pool.tokens().into_iter().collect::<Vec<_>>();
        self.insert_edge_by_keys(pool.clone(), &tokens[0], &tokens[1], (0, 1))
            .ok();
        self.insert_edge_by_keys(pool.clone(), &tokens[1], &tokens[0], (1, 0))
            .ok();
    }
}

#[cfg(test)]
mod tests_graph {
    use revm::primitives::b256;

    use super::*;

    #[test]
    fn all_routes() {
        let v2_d = Pools::V2(V2 {
            address: Address::random(),
            token1: WETH_ADDRESS,
            token0: USDC_ADDRESS,
            ..Default::default()
        });
        let v2_a = Pools::V2(V2 {
            address: Address::random(),
            token1: WETH_ADDRESS,
            token0: USDC_ADDRESS,
            ..Default::default()
        });
        let must_include = v2_a.address();
        let v2_b = Pools::V2(V2 {
            address: Address::random(),
            token1: WETH_ADDRESS,
            token0: USDC_ADDRESS,
            ..Default::default()
        });
        let v2_c = Pools::V2(V2 {
            address: Address::random(),
            token1: USDT_ADDRESS,
            token0: USDC_ADDRESS,
            ..Default::default()
        });

        let mut graph = DirectedGraph::default();
        graph.from_storage(&[
            Arc::new(v2_a),
            Arc::new(v2_b),
            Arc::new(v2_c),
            Arc::new(v2_d),
        ]);

        let l = graph.generate_all_routes(WETH_ADDRESS, 2);
        dbg!(&l);

        let l = l
            .iter()
            .filter(|(legs, _)| legs.contains(&must_include.encode_hex()))
            .collect_vec();
        dbg!(l);
    }

    #[test]
    fn is_profitable_route() {
        let mut v3_a = Pools::V3(V3 {
            address: address!("847165954680b989902e354f34d08b09afab3cd9"),
            token0: address!("6830a684df938d7bd409070eecee2ce5db6951a3"),
            token1: WETH_ADDRESS,
            fee: U256::from(100),
            ..Default::default()
        });

        v3_a.update_balances_state(&b256!(
            "0001000001000100000025cc00000000000000019f47b54c848cd4fea9efa82c"
        ));

        let oneinch_b = Pools::OneInch(serde_json::from_str(r#"{
            "id": 14690848,
            "orderHash": "0xd5fdba4007b8ffe8082e9b6a62406e48625d0ea0df0205a2d07e56346c067c6c",
            "createDateTime": "2024-09-26T03:24:46.988Z",
            "lastChangedDateTime": "2024-10-01T02:05:43.562Z",
            "signature": "0xe2956086240503ac6f91f818c5763c12a86fec426c741c97ad7e262d512ac14529d7b43c3e99ebfdf8a425b6977259e52a33c2b674edc878250c68219c2d2bd61b",
            "takerAsset": "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c",
            "makerAsset": "0x6830a684df938d7bd409070eecee2ce5db6951a3",
            "orderStatus": 1,
            "makerAmount": "4000000000000000000",
            "remainingMakerAmount": "1226213620343136131",
            "orderMaker": "0x86a7983127c5b7b83a288f401ec238fff290b509",
            "makerBalance": "1721283037206199409",
            "makerAllowance": "115792089237316195423570985008687907853269984665640564039457584007913129639935",
            "takerAmount": "2422326560000000000",
            "data": {
                "salt": "96351180577133012692043950367720879691292064037010782124449218975405618518516",
                "maker": "0x86a7983127c5b7b83a288f401ec238fff290b509",
                "receiver": "0x0000000000000000000000000000000000000000",
                "extension": "0x",
                "makerAsset": "0x6830a684df938d7bd409070eecee2ce5db6951a3",
                "takerAsset": "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c",
                "makerTraits": "0x44000000000000000000000000000000000068cf6ffb00000000000000000000",
                "makingAmount": "4000000000000000000",
                "takingAmount": "2422326560000000000"
            },
            "makerRate": "0.605581640000000000",
            "takerRate": "1.651305016446667703",
            "orderInvalidReason": null,
            "isMakerContract": false
        }"#).unwrap());

        let one_to_zero = dbg!(v3_a.rate((0, 1)));
        let zero_to_one = dbg!(oneinch_b.rate((1, 0)));
        let arb = dbg!(one_to_zero.unwrap() * zero_to_one.unwrap());
        let profit = arb * 0.002203291738047329;
        dbg!(profit);
    }

    #[test]
    fn is_profitable_route2() {
        let mut v3_a = Pools::V3(V3 {
            fee: U256::from(100),
            ..Default::default()
        });

        v3_a.update_balances_state(&b256!(
            "000000006400640003ff07f000000000000000000ab2f402affb2b361f40d9f7"
        ));

        let mut v3_b = Pools::V3(V3 {
            fee: U256::from(5000),
            ..Default::default()
        });

        v3_b.update_balances_state(&b256!(
            "000144000100010000ff9a7e000000000000000045d2c95b9d43fbfdd4c4ab05"
        ));
        let mut v3_c = Pools::V3(V3 {
            fee: U256::from(5000),
            ..Default::default()
        });

        v3_c.update_balances_state(&b256!(
            "0001440001000100000052680000000000000002df0cb9166c0ab8adcf2a2eb4"
        ));
        let mut v3_d = Pools::V3(V3 {
            fee: U256::from(2500),
            ..Default::default()
        });

        v3_d.update_balances_state(&b256!(
            "000000001400140007fef56c0000000000000000087106cd72daa78b5f6bce46"
        ));

        let r = v3_a.rate((1, 0)).unwrap() * v3_a.fee();
        let r2 = v3_b.rate((1, 0)).unwrap() * v3_b.fee();
        let r3 = v3_c.rate((1, 0)).unwrap() * v3_c.fee();
        let r4 = v3_d.rate((0, 1)).unwrap() * v3_d.fee();
        dbg!(r, r2, r3, r4);
        let arb = dbg!(r * r2 * r3 * r4);
        let profit = arb * 1.2157416421366434;
        dbg!(profit);
    }
}
