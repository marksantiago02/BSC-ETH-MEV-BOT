use alloy::sol;

sol!(
    #[sol(rpc)]
    Searcher,
    "./src/metadata/ABI/Searcher.json"
);

sol!(
    #[sol(rpc)]
    Rustitrage,
    "./src/metadata/ABI/Rustitrage.json"
);

sol!(
    struct OneInchOrderV3 {
        uint256 salt;
        address makerAsset;
        address takerAsset;
        address maker;
        address receiver;
        address allowedSender; // equals to Zero address on public orders
        uint256 makingAmount;
        uint256 takingAmount;
        uint256 offsets;
        bytes interactions; // concat(makerAssetData, takerAssetData, getMakingAmount, getTakingAmount, predicate, permit, preIntercation, postInteraction)
    }

    struct Order {
        uint256 salt;
        address maker;
        address receiver;
        address makerAsset;
        address takerAsset;
        uint256 makingAmount;
        uint256 takingAmount;
        uint256 makerTraits;
    }

    function fillOrder(
        Order calldata order,
        bytes32 r,
        bytes32 vs,
        uint256 amount,
        uint256 takerTraits
    ) external payable returns(uint256 makingAmount, uint256 takingAmount, bytes32 orderHash);

    function fillOrderArgs(
        Order calldata order,
        bytes32 r,
        bytes32 vs,
        uint256 amount,
        uint256 takerTraits,
        bytes calldata args
    ) external payable returns(uint256 makingAmount, uint256 takingAmount, bytes32 orderHash);
);

#[cfg(test)]
mod tests_pools {

    use super::*;
    use crate::*;
    use alloy::{
        primitives::{B256, U256},
        signers::Signature,
        sol_types::SolCall,
    };
    use std::str::FromStr;

    #[test]
    fn decodddd() {
        let b = bytes!("000000000000000000000000d66783a8092789f56f9da29113a6780135d956ad00000000000000000000000015112f70659ff634b0101ba49b15a9ee180eab3200000000000000000000000000000000000000000000000000000000000000000000000000000000000000008ac76a51cc950d9822d68b83fe1ad97b32cd580d0000000000000000000000006ba5657bbff83cb579503847c6baa47295ef79a80000000000000000000000000000000000000000000000056bc75e2d63100000000000000000000000000000000000000000000000000860016c7d7dd980000044000000000000000000000000000000010066ee36b000000000000000000000e14af8af6decaa91bc0f2b230ad237ee4134ae8bff21611e29ef5c2be80dfe9cc5761857e17d515273c09170981d0671d1537d263bfe1805a0e460f9903c28c400000000000000000000000000000000000000000000000056b27989842baff6080000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001a00000000000000000000000000000000000000000000000000000000000000014ec6557348085aa57c72514d67070dc863c0a5a8c000000000000000000000000");

        let d = fillOrderArgsCall::abi_decode_raw(&b[..], false);
        dbg!(d.clone().unwrap().r);
        dbg!(d.unwrap().vs);
        let s = Signature::from_str("e14af8af6decaa91bc0f2b230ad237ee4134ae8bff21611e29ef5c2be80dfe9c45761857e17d515273c09170981d0671d1537d263bfe1805a0e460f9903c28c41c")
            .unwrap()
            .with_chain_id(CHAIN_ID);

        let r: B256 = s.r().into();
        let vs: B256 = if s.v().y_parity() {
            (s.s() | (U256::from(1) << U256::from(255))).into()
        } else {
            s.s().into()
        };

        dbg!(r, vs);
    }
}
