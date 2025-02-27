use std::{collections::HashMap, net::SocketAddr, path::Path, sync::Arc};

use anyhow::Result;
use async_channel::{Receiver, Sender};
use async_trait::async_trait;
use hydroflow_cli_integration::ServerBindConfig;
use tokio::sync::RwLock;

pub mod deployment;
pub use deployment::Deployment;

pub mod localhost;
pub use localhost::LocalhostHost;

pub mod ssh;

pub mod gcp;
pub use gcp::GCPComputeEngineHost;

pub mod hydroflow_crate;
pub use hydroflow_crate::HydroflowCrate;

pub mod custom_service;
pub use custom_service::CustomService;

pub mod terraform;

pub mod util;

#[derive(Default)]
pub struct ResourcePool {
    pub terraform: terraform::TerraformPool,
}

pub struct ResourceBatch {
    pub terraform: terraform::TerraformBatch,
}

impl ResourceBatch {
    fn new() -> ResourceBatch {
        ResourceBatch {
            terraform: terraform::TerraformBatch::default(),
        }
    }

    async fn provision(self, pool: &mut ResourcePool) -> Result<ResourceResult> {
        Ok(ResourceResult {
            terraform: self.terraform.provision(&mut pool.terraform).await?,
        })
    }
}

pub struct ResourceResult {
    pub terraform: terraform::TerraformResult,
}

#[async_trait]
pub trait LaunchedBinary: Send + Sync {
    async fn stdin(&self) -> Sender<String>;
    async fn stdout(&self) -> Receiver<String>;
    async fn stderr(&self) -> Receiver<String>;

    async fn exit_code(&self) -> Option<i32>;

    async fn wait(&mut self) -> Option<i32>;
}

#[async_trait]
pub trait LaunchedHost: Send + Sync {
    /// Given a pre-selected network type, computes concrete information needed for a service
    /// to listen to network connections (such as the IP address to bind to).
    fn server_config(&self, strategy: &ServerStrategy) -> ServerBindConfig;

    async fn launch_binary(
        &self,
        binary: &Path,
        args: &[String],
    ) -> Result<Arc<RwLock<dyn LaunchedBinary>>>;

    async fn forward_port(&self, addr: &SocketAddr) -> Result<SocketAddr>;
}

/// Types of connections that a host can make to another host.
pub enum ServerStrategy {
    UnixSocket,
    InternalTcpPort,
    ExternalTcpPort(
        /// The port number to bind to, which must be explicit to open the firewall.
        u16,
    ),
    Demux(HashMap<u32, ServerStrategy>),
    Merge(Vec<ServerStrategy>),
    Null,
}

/// Like BindType, but includes metadata for determining whether a connection is possible.
pub enum ClientStrategy<'a> {
    UnixSocket(
        /// Unique identifier for the host this socket will be on.
        usize,
    ),
    InternalTcpPort(
        /// The host that this port is available on.
        &'a dyn Host,
    ),
    ForwardedTcpPort(
        /// The host that this port is available on.
        &'a dyn Host,
    ),
}

pub enum HostTargetType {
    Local,
    Linux,
}

pub type HostStrategyGetter = Box<dyn FnOnce(&mut dyn std::any::Any) -> ServerStrategy>;

#[async_trait]
pub trait Host: Send + Sync {
    fn target_type(&self) -> HostTargetType;

    fn request_port(&mut self, bind_type: &ServerStrategy);

    /// An identifier for this host, which is unique within a deployment.
    fn id(&self) -> usize;

    /// Returns a reference to the host as a trait object.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Returns a reference to the host as a trait object.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Configures the host to support copying and running a custom binary.
    fn request_custom_binary(&mut self);

    /// Makes requests for physical resources (servers) that this host needs to run.
    fn collect_resources(&self, resource_batch: &mut ResourceBatch);

    /// Connects to the acquired resources and prepares the host to run services.
    async fn provision(&mut self, resource_result: &Arc<ResourceResult>) -> Arc<dyn LaunchedHost>;

    /// Identifies a network type that this host can use for connections if it is the server.
    /// The host will be `None` if the connection is from the same host as the target.
    fn strategy_as_server<'a>(
        &'a self,
        connection_from: Option<&dyn Host>,
    ) -> Result<(ClientStrategy<'a>, HostStrategyGetter)>;

    /// Determines whether this host can connect to another host using the given strategy.
    fn can_connect_to(&self, typ: ClientStrategy) -> bool;
}

#[async_trait]
pub trait Service: Send + Sync {
    /// Makes requests for physical resources server ports that this service needs to run.
    /// This should **not** recursively call `collect_resources` on the host, since
    /// we guarantee that `collect_resources` is only called once per host.
    ///
    /// This should also perform any "free", non-blocking computations (compilations),
    /// because the `deploy` method will be called after these resources are allocated.
    fn collect_resources(&mut self, resource_batch: &mut ResourceBatch);

    /// Connects to the acquired resources and prepares the service to be launched.
    async fn deploy(&mut self, resource_result: &Arc<ResourceResult>);

    /// Launches the service, which should start listening for incoming network
    /// connections. The service should not start computing at this point.
    async fn ready(&mut self) -> Result<()>;

    /// Starts the service by having it connect to other services and start computations.
    async fn start(&mut self);

    /// Stops the service by having it disconnect from other services and stop computations.
    async fn stop(&mut self) -> Result<()>;
}
