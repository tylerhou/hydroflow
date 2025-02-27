use std::collections::HashMap;

use hydroflow_cli_integration::{BoundConnection, ServerBindConfig, ServerPort};

pub use hydroflow_cli_integration::*;

use crate::scheduled::graph::Hydroflow;

async fn accept_incoming_connections(
    binds: HashMap<String, BoundConnection>,
) -> HashMap<String, ServerOrBound> {
    let mut bind_results = HashMap::new();
    for (name, bind) in binds {
        bind_results.insert(name, ServerOrBound::Bound(bind));
    }
    bind_results
}

pub async fn launch_flow(mut flow: Hydroflow) {
    let stop = tokio::sync::oneshot::channel();
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        assert!(line.starts_with("stop"));
        stop.0.send(()).unwrap();
    });

    tokio::select! {
        _ = stop.1 => {},
        _ = flow.run_async() => {}
    }
}

pub async fn init() -> HashMap<String, ServerOrBound> {
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();

    let bind_config = serde_json::from_str::<HashMap<String, ServerBindConfig>>(trimmed).unwrap();

    // config telling other services how to connect to me
    let mut bind_results: HashMap<String, ServerPort> = HashMap::new();
    let mut binds = HashMap::new();
    for (name, config) in bind_config {
        let bound = config.bind().await;
        bind_results.insert(name.clone(), bound.sink_port());
        binds.insert(name.clone(), bound);
    }

    let bind_connected_future = tokio::task::spawn(accept_incoming_connections(binds));

    let bind_serialized = serde_json::to_string(&bind_results).unwrap();
    println!("ready: {bind_serialized}");

    let mut start_buf = String::new();
    std::io::stdin().read_line(&mut start_buf).unwrap();
    let connection_defns = if start_buf.starts_with("start: ") {
        serde_json::from_str::<HashMap<String, ServerPort>>(
            start_buf.trim_start_matches("start: ").trim(),
        )
        .unwrap()
    } else {
        panic!("expected start");
    };

    let mut all_connected = HashMap::new();
    for (name, defn) in connection_defns {
        all_connected.insert(name, ServerOrBound::Server(defn));
    }

    let bind_connected = bind_connected_future.await.unwrap();
    for (name, defn) in bind_connected {
        all_connected.insert(name, defn);
    }

    all_connected
}
