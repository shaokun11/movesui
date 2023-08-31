use std::{collections::HashMap, fs, io::{self, Error, ErrorKind}, sync::Arc};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration};
use avalanche_types::{
    choices, ids,
    subnet::{self, rpc::snow},
};
use avalanche_types::subnet::rpc::database::manager::{DatabaseManager, Manager};
use avalanche_types::subnet::rpc::health::Checkable;
use avalanche_types::subnet::rpc::snow::engine::common::appsender::AppSender;
use avalanche_types::subnet::rpc::snow::engine::common::appsender::client::AppSenderClient;
use avalanche_types::subnet::rpc::snow::engine::common::engine::{AppHandler, CrossChainAppHandler, NetworkAppHandler};
use avalanche_types::subnet::rpc::snow::engine::common::http_handler::{HttpHandler, LockOptions};
use avalanche_types::subnet::rpc::snow::engine::common::message::Message::PendingTxs;
use avalanche_types::subnet::rpc::snow::engine::common::vm::{CommonVm, Connector};
use avalanche_types::subnet::rpc::snow::validators::client::ValidatorStateClient;
use avalanche_types::subnet::rpc::snowman::block::{BatchedChainVm, ChainVm, Getter, Parser};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use hex;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::Sender, RwLock};
use sui_cluster_test::cluster::{Cluster, LocalNewCluster};
use sui_cluster_test::config::ClusterTestOpt;


use crate::{block::Block, state};
use crate::api::chain_handlers::{ChainHandler, ChainService};
use crate::api::static_handlers::{StaticHandler, StaticService};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const MOVE_DB_DIR: &str = ".move-chain-data";


/// Represents VM-specific states.
/// Defined in a separate struct, for interior mutability in [`Vm`](Vm).
/// To be protected with `Arc` and `RwLock`.
pub struct VmState {
    pub ctx: Option<subnet::rpc::context::Context<ValidatorStateClient>>,

    /// Represents persistent Vm state.
    pub state: Option<state::State>,
    /// Currently preferred block Id.
    pub preferred: ids::Id,

    /// Set "true" to indicate that the Vm has finished bootstrapping
    /// for the chain.
    pub bootstrapped: bool,
}

impl Default for VmState {
    fn default() -> Self {
        Self {
            ctx: None,
            state: None,
            preferred: ids::Id::empty(),
            bootstrapped: false,
        }
    }
}

/// Implements [`snowman.block.ChainVM`](https://pkg.go.dev/github.com/ava-labs/avalanchego/snow/engine/snowman/block#ChainVM) interface.
#[derive(Clone)]
pub struct Vm {
    pub state: Arc<RwLock<VmState>>,
}

impl Default for Vm

{
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(VmState::default())),
        }
    }

    pub async fn is_bootstrapped(&self) -> bool {
        let vm_state = self.state.read().await;
        vm_state.bootstrapped
    }


    /// Sets the state of the Vm.
    /// # Errors
    /// Will fail if the `snow::State` is syncing
    pub async fn set_state(&self, snow_state: snow::State) -> io::Result<()> {
        let mut vm_state = self.state.write().await;
        match snow_state {
            // called by chains manager when it is creating the chain.
            snow::State::Initializing => {
                log::info!("set_state: initializing");
                vm_state.bootstrapped = false;
                Ok(())
            }

            snow::State::StateSyncing => {
                log::info!("set_state: state syncing");
                Err(Error::new(ErrorKind::Other, "state sync is not supported"))
            }

            // called by the bootstrapper to signal bootstrapping has started.
            snow::State::Bootstrapping => {
                log::info!("set_state: bootstrapping");
                vm_state.bootstrapped = false;
                Ok(())
            }

            // called when consensus has started signalling bootstrap phase is complete.
            snow::State::NormalOp => {
                log::info!("set_state: normal op");
                vm_state.bootstrapped = true;
                Ok(())
            }
        }
    }


    /// Sets the container preference of the Vm.
    pub async fn set_preference(&self, id: ids::Id) -> io::Result<()> {
        let mut vm_state = self.state.write().await;
        vm_state.preferred = id;

        Ok(())
    }

    /// Returns the last accepted block Id.
    pub async fn last_accepted(&self) -> io::Result<ids::Id> {
        let vm_state = self.state.read().await;
        if let Some(state) = &vm_state.state {
            let blk_id = state.get_last_accepted_block_id().await?;
            return Ok(blk_id);
        }
        Err(Error::new(ErrorKind::NotFound, "state manager not found"))
    }


    pub async fn inner_build_block(&self, data: Vec<u8>) -> io::Result<()> {
        Ok(())
    }

    async fn init_chain(&mut self) {
        let cluster = LocalNewCluster::start(&ClusterTestOpt {
            env: sui_cluster_test::config::Env::NewLocal,
            fullnode_address: Some(format!("127.0.0.1:{}", 9000)),
            indexer_address: None,
            pg_address: None,
            faucet_address: None,
            epoch_duration_ms: None,
            use_indexer_experimental_methods: false,
            config_dir: Some(PathBuf::from("/tmp/.suiconfig")),
        })
            .await.unwrap();
        println!("Fullnode RPC URL: {}", cluster.fullnode_url());
        // start_faucet(&cluster, 9123).await?;
    }
}

#[tonic::async_trait]
impl BatchedChainVm for Vm {
    type Block = Block;

    async fn get_ancestors(
        &self,
        _block_id: ids::Id,
        _max_block_num: i32,
        _max_block_size: i32,
        _max_block_retrival_time: Duration,
    ) -> io::Result<Vec<Bytes>> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "get_ancestors not implemented",
        ))
    }

    async fn batched_parse_block(&self, _blocks: &[Vec<u8>]) -> io::Result<Vec<Self::Block>> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "batched_parse_block not implemented",
        ))
    }
}

#[tonic::async_trait]
impl ChainVm for Vm
{
    type Block = Block;

    async fn build_block(
        &self,
    ) -> io::Result<<Self as ChainVm>::Block> {
        let vm_state = self.state.read().await;
        if let Some(state_b) = vm_state.state.as_ref() {
            let prnt_blk = state_b.get_block(&vm_state.preferred).await.unwrap();
            let unix_now = Utc::now().timestamp() as u64;
            let mut block_ = Block::new(
                prnt_blk.id(),
                prnt_blk.height() + 1,
                unix_now,
                vec![],
                choices::status::Status::Processing,
            ).unwrap();
            block_.set_state(state_b.clone());
            println!("--------vm_build_block------{}---", block_.id());
            block_.verify().await.unwrap();
            return Ok(block_);
        }
        Err(Error::new(
            ErrorKind::Other,
            "not implement",
        ))
    }

    async fn issue_tx(
        &self,
    ) -> io::Result<<Self as ChainVm>::Block> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "issue_tx not implemented",
        ))
    }

    async fn set_preference(&self, id: ids::Id) -> io::Result<()> {
        self.set_preference(id).await
    }

    async fn last_accepted(&self) -> io::Result<ids::Id> {
        self.last_accepted().await
    }
}

#[tonic::async_trait]
impl NetworkAppHandler for Vm
{
    /// Currently, no app-specific messages, so returning Ok.
    async fn app_request(
        &self,
        _node_id: &ids::node::Id,
        _request_id: u32,
        _deadline: DateTime<Utc>,
        _request: &[u8],
    ) -> io::Result<()> {
        Ok(())
    }

    /// Currently, no app-specific messages, so returning Ok.
    async fn app_request_failed(
        &self,
        _node_id: &ids::node::Id,
        _request_id: u32,
    ) -> io::Result<()> {
        Ok(())
    }

    /// Currently, no app-specific messages, so returning Ok.
    async fn app_response(
        &self,
        _node_id: &ids::node::Id,
        _request_id: u32,
        _response: &[u8],
    ) -> io::Result<()> {
        Ok(())
    }

    async fn app_gossip(&self, _node_id: &ids::node::Id, msg: &[u8]) -> io::Result<()> {
        Ok(())
    }
}

#[tonic::async_trait]
impl CrossChainAppHandler for Vm
{
    /// Currently, no cross chain specific messages, so returning Ok.
    async fn cross_chain_app_request(
        &self,
        _chain_id: &ids::Id,
        _request_id: u32,
        _deadline: DateTime<Utc>,
        _request: &[u8],
    ) -> io::Result<()> {
        Ok(())
    }

    /// Currently, no cross chain specific messages, so returning Ok.
    async fn cross_chain_app_request_failed(
        &self,
        _chain_id: &ids::Id,
        _request_id: u32,
    ) -> io::Result<()> {
        Ok(())
    }

    /// Currently, no cross chain specific messages, so returning Ok.
    async fn cross_chain_app_response(
        &self,
        _chain_id: &ids::Id,
        _request_id: u32,
        _response: &[u8],
    ) -> io::Result<()> {
        Ok(())
    }
}

impl AppHandler for Vm {}

#[tonic::async_trait]
impl Connector for Vm

{
    async fn connected(&self, _id: &ids::node::Id) -> io::Result<()> {
        // no-op
        Ok(())
    }

    async fn disconnected(&self, _id: &ids::node::Id) -> io::Result<()> {
        // no-op
        Ok(())
    }
}

#[tonic::async_trait]
impl Checkable for Vm
{
    async fn health_check(&self) -> io::Result<Vec<u8>> {
        Ok("200".as_bytes().to_vec())
    }
}

#[tonic::async_trait]
impl Getter for Vm
{
    type Block = Block;

    async fn get_block(
        &self,
        blk_id: ids::Id,
    ) -> io::Result<<Self as Getter>::Block> {
        let vm_state = self.state.read().await;
        if let Some(state) = &vm_state.state {
            let mut block = state.get_block(&blk_id).await?;
            let mut new_state = state.clone();
            new_state.set_vm(self.clone());
            block.set_state(new_state);
            return Ok(block);
        }
        Err(Error::new(ErrorKind::NotFound, "state manager not found"))
    }
}

#[tonic::async_trait]
impl Parser for Vm
{
    type Block = Block;
    async fn parse_block(
        &self,
        bytes: &[u8],
    ) -> io::Result<<Self as Parser>::Block> {
        let vm_state = self.state.read().await;
        if let Some(state) = vm_state.state.as_ref() {
            let mut new_block = Block::from_slice(bytes)?;
            new_block.set_status(choices::status::Status::Processing);
            let mut new_state = state.clone();
            new_state.set_vm(self.clone());
            new_block.set_state(new_state);
            return match state.get_block(&new_block.id()).await {
                Ok(prev) => {
                    Ok(prev)
                }
                Err(_) => {
                    Ok(new_block)
                }
            };
        }
        Err(Error::new(ErrorKind::NotFound, "state manager not found"))
    }
}

#[tonic::async_trait]
impl CommonVm for Vm
{
    type DatabaseManager = DatabaseManager;
    type AppSender = AppSenderClient;
    type ChainHandler = ChainHandler<ChainService>;
    type StaticHandler = StaticHandler;
    type ValidatorState = ValidatorStateClient;

    async fn initialize(
        &mut self,
        ctx: Option<subnet::rpc::context::Context<Self::ValidatorState>>,
        db_manager: Self::DatabaseManager,
        genesis_bytes: &[u8],
        _upgrade_bytes: &[u8],
        _config_bytes: &[u8],
        to_engine: Sender<snow::engine::common::message::Message>,
        _fxs: &[snow::engine::common::vm::Fx],
        app_sender: Self::AppSender,
    ) -> io::Result<()> {
        self.init_chain().await;
        let mut vm_state = self.state.write().await;
        vm_state.ctx = ctx.clone();
        let current = db_manager.current().await?;
        let state = state::State {
            db: Arc::new(RwLock::new(current.clone().db)),
            verified_blocks: Arc::new(RwLock::new(HashMap::new())),
            vm: None,
        };

        let mut vm_state = self.state.write().await;
        let genesis = "hello world";
        let has_last_accepted = state.has_last_accepted_block().await?;
        if has_last_accepted {
            let last_accepted_blk_id = state.get_last_accepted_block_id().await?;
            vm_state.preferred = last_accepted_blk_id;
        } else {
            let genesis_bytes = genesis.as_bytes().to_vec();
            let mut genesis_block = Block::new(
                ids::Id::empty(),
                0,
                0,
                genesis_bytes,
                choices::status::Status::default(),
            ).unwrap();
            genesis_block.set_state(state.clone());
            genesis_block.accept().await?;

            let genesis_blk_id = genesis_block.id();
            vm_state.preferred = genesis_blk_id;
        }
        log::info!("successfully initialized Vm");
        Ok(())
    }

    async fn set_state(&self, snow_state: snow::State) -> io::Result<()> {
        self.set_state(snow_state).await
    }

    /// Called when the node is shutting down.
    async fn shutdown(&self) -> io::Result<()> {
        Ok(())
    }

    async fn version(&self) -> io::Result<String> {
        Ok(String::from(VERSION))
    }

    async fn create_static_handlers(
        &mut self,
    ) -> io::Result<HashMap<String, HttpHandler<Self::StaticHandler>>> {
        let handler = StaticHandler::new(StaticService::new());
        let mut handlers = HashMap::new();
        handlers.insert(
            "/static".to_string(),
            HttpHandler {
                lock_option: LockOptions::WriteLock,
                handler,
                server_addr: None,
            },
        );

        Ok(handlers)
    }

    async fn create_handlers(
        &mut self,
    ) -> io::Result<HashMap<String, HttpHandler<Self::ChainHandler>>> {
        let handler = ChainHandler::new(ChainService::new(self.clone()));
        let mut handlers = HashMap::new();
        handlers.insert(
            "/rpc".to_string(),
            HttpHandler {
                lock_option: LockOptions::WriteLock,
                handler,
                server_addr: None,
            },
        );

        Ok(handlers)
    }
}