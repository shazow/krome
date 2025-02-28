use std::sync::Mutex;
use std::path::PathBuf;
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;
use serde_json::Value;
use tauri::{AppHandle, State};

use helios::ethereum::EthereumClient;
use helios::ethereum::database::FileDB;
use helios::core::types::BlockTag;
use helios::ethereum::{
    config::networks::Network,
    EthereumClientBuilder,
};

// Global Tokio runtime
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

// Global Helios client
pub struct HeliosState(pub Mutex<Option<EthereumClient<FileDB>>>);

fn get_network(chain_id: u64) -> Result<Network, String> {
    match chain_id {
        1 => Ok(Network::Mainnet),
        _ => Err(format!("Unsupported chain ID: {}", chain_id)),
    }
}

#[tauri::command]
pub async fn start_helios(
    state: State<'_, HeliosState>,
    app_handle: AppHandle,
    rpc_url: String,
    consensus_rpc: Option<String>,
    chain_id: u64,
) -> Result<(), String> {
    // Use a local helper function to get the data dir from app_handle
    let data_dir = {
        let maybe_path = app_handle
            .path_resolver()
            .app_data_dir(); // or app_dir(), app_config_dir(), etc.
        maybe_path
            .ok_or_else(|| "could not resolve the app data directory".to_string())?
            .join("helios")
    };

    let consensus_rpc = consensus_rpc.unwrap_or_else(|| "https://www.lightclientdata.org".to_string());
    
    let result: Result<EthereumClient<FileDB>, String> = RUNTIME.block_on(async {
        let network = get_network(chain_id)?;
        
        let mut client = EthereumClientBuilder::new()
            .network(network)
            .execution_rpc(&rpc_url)
            .consensus_rpc(&consensus_rpc)
            .data_dir(data_dir)
            .build()
            .map_err(|e| format!("Failed to build client: {:?}", e))?;

        // Start the client and wait for sync
        client.start().await.map_err(|e| format!("Failed to start client: {:?}", e))?;
        client.wait_synced().await;
        Ok(client)
    });

    match result {
        Ok(client) => {
            let mut guard = state.0.lock().unwrap();
            *guard = Some(client);
            Ok(())
        },
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub async fn get_latest_block(state: State<'_, HeliosState>) -> Result<Value, String> {
    RUNTIME.block_on(async {
        let guard = state.0.lock().unwrap();
        if let Some(client) = guard.as_ref() {
            let block = client
                .get_block_by_number(BlockTag::Latest, false)
                .await
                .map_err(|e| format!("Failed to get block: {:?}", e))?;
            
            serde_json::to_value(block)
                .map_err(|e| format!("Serialization error: {:?}", e))
        } else {
            Err("Client not started".to_string())
        }
    })
} 
