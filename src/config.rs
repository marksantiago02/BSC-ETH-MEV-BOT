use crate::*;

#[macro_export]
/// Load all environment variables with their respective types
macro_rules! load_env_vars {
    ($($var_name:ident : $type:ty),*) => {
        lazy_static! {
            $(
                pub static ref $var_name: $type = {
                    dotenv().ok();

                    let var_value = dotenv::var(stringify!($var_name)).unwrap_or_else(|_| panic!("{} NOT FOUND", stringify!($var_name)));
                    var_value.parse::<$type>().unwrap_or_else(|_| panic!("{} IS NOT A {}", stringify!($var_name), stringify!($type)))
                };
            )*
        }

        /// Makes sure all the environment variables are loaded
        pub fn init_env_vars() {
            $(
                lazy_static::initialize(&$var_name);
            )*
        }
    };
}

// Load all environment variables with their respective types
load_env_vars!(
    IPC_NODE_URL: String,
    POOLS_DATA_DIR: String,
    DB_PATH_DIR: String,
    WALLET_SIGNER: PrivateKeySigner,
    DISCORD_NEW_POOL: Url,
    DISCORD_DETECTED_ARB: Url,
    DISCORD_DETECTED_SWAP: Url,
    DISCORD_STORAGE: Url,
    DISCORD_PRIVATE_FLOW: Url,
    ARBONLY_ENABLED: bool,
    FAST_MEMPOOL_ENABLED: bool,
    MERKLE_ENABLED: bool,
    PUBLIC_MEMPOOL_ENABLED: bool,
    NEXTBLOCK_ENABLED: bool,
    SANDWICH_ENABLED: bool,
    ARBITRAGE_ENABLED: bool,
    HTTP_NODE_URL: Url,
    WS_NODE_URL: Url
);
