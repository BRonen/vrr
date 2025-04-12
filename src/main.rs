use axum::{
    extract::State, routing::{get, post}, Json, Router
};
use futures::{future, stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use reqwest;


type ReplicaId = String;
type ReplicaAddress = String;
type ViewNumber = u64;
type OpNumber = u64;
type ClientId = String;
type RequestNumber = usize;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
struct ReplicaInfo {
    id: ReplicaId,
    address: ReplicaAddress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Operation {
    Add(usize),
    Sub(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ReplicaStatus {
    Normal,
    ViewChange,
    Recovering,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OperationInfo {
    client_id: String,
    request_number: RequestNumber,
    operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ViewstampedReplicaState {
    replicas: Vec<ReplicaInfo>,
    replica_number: usize,
    view_number: ViewNumber,
    status: ReplicaStatus,
    op_number: OpNumber,
    log: Vec<OperationInfo>,
    commit_number: OpNumber,
    client_tables: HashMap<ClientId, Vec<Option<OperationInfo>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClientRequest {
    client_id: String,
    request_number: RequestNumber,
    operation: String,
    value: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrepareMessage {
    view_number: ViewNumber,
    op_number: OpNumber,
    commit_number: OpNumber,
    request: ClientRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrepareOkMessage {
    view_number: ViewNumber,
    op_number: OpNumber,
    commit_number: OpNumber,
    replica_id: String,
}

impl ViewstampedReplicaState {
    fn new(replica_number: usize, replicas: Vec<ReplicaInfo>) -> Self {
        let total_replicas = replicas.len();
        let max_failures = (total_replicas - 1) / 2;
        let quorum_size = max_failures + 1;

        println!("Initializing replica {:?} with quorum size: {}", replicas[replica_number], quorum_size);

        Self {
            replicas,
            replica_number,
            view_number: 0,
            op_number: 0,
            log: Vec::new(),
            commit_number: 0,
            status: ReplicaStatus::Normal,
            client_tables: HashMap::new(),
        }
    }

    fn get_self_replica(&self) -> &ReplicaInfo {
        let replica_number = self.replica_number;
        self.replicas.get(replica_number).unwrap()
    }

    fn get_primary(&self) -> &ReplicaInfo {
        let primary_idx = (self.view_number as usize) % self.replicas.len();
        &self.replicas[primary_idx]
    }

    fn is_primary(&self) -> bool {
        let primary_id = &self.get_primary().id;
        let self_id = &self.get_self_replica().id;
        primary_id == self_id
    }

    fn get_operation_from_payload(&self, operation: String, value: usize) -> Operation {
        match operation.as_str() {
            "add" => Operation::Add(value),
            "sub" => Operation::Sub(value),
            _ => panic!("Invalid operation: {}", operation),
        }
    }

    async fn handle_client_request(&mut self, request: ClientRequest) -> Result<(), String> {
        // TODO: repass request to primary
        if !self.is_primary() {
            return Err("Not the primary".to_string());
        }

        if self.status != ReplicaStatus::Normal {
            return Err("Replica not in normal operation".to_string());
        }

        if self.client_tables.len() >= request.request_number.clone() {
            return Err("Request number already exists".to_string());
        }

        if let Some(_) = self.client_tables.get(&request.client_id) {
            return Ok(());
        }

        let current_op = self.op_number;
        self.op_number += 1;

        let operation = OperationInfo {
            client_id: request.client_id.clone(),
            request_number: request.request_number,
            operation: self.get_operation_from_payload(request.operation.clone(), request.value.clone()),
        };
        self.log.push(operation);

        let prepare = PrepareMessage {
            view_number: self.view_number,
            op_number: current_op,
            commit_number: self.commit_number,
            request,
        };

        let Ok(_) = self.broadcast_till_quorum(prepare, &"/prepare".to_owned()).await
        else {
            return Err(String::from("error a"));
        };

        Ok(())
    }

    async fn handle_prepare(&mut self, prepare: PrepareMessage) -> Result<(), String> {
        if self.status != ReplicaStatus::Normal {
            return Err("Replica not in normal operation".to_string());
        }

        if prepare.view_number != self.view_number {
            return Err(format!(
                "View number mismatch. Got {}, expected {}",
                prepare.view_number, self.view_number
            ));
        }

        if prepare.op_number != self.op_number + 1 {
            return Err(format!(
                "Operation number mismatch. Got {}, expected {}",
                prepare.op_number,
                self.op_number + 1
            ));
        }

        self.op_number = prepare.op_number;

        let operation = OperationInfo {
            client_id: prepare.request.client_id.clone(),
            request_number: prepare.request.request_number,
            operation: self.get_operation_from_payload(prepare.request.operation, prepare.request.value),
        };

        self.log.push(operation.clone());

        let Some(logs) = self.client_tables.get(&prepare.request.client_id).clone()
        else { todo!() };

        let mut logs = logs.to_vec();

        logs.push(Some(operation));

        self.client_tables.insert(prepare.request.client_id, logs);

        // self.broadcast_till_quorum(, &"/prepare".to_owned()).await;

        Ok(())
    }

    // async fn handle_prepare_ok(&mut self, msg: PrepareOkMessage) -> Result<(), String> {
    //     if !self.is_primary() {
    //         return Err("Not the primary".to_string());
    //     }

    //     if self.status != ReplicaStatus::Normal {
    //         return Err("Replica not in normal operation".to_string());
    //     }

    //     if msg.view_number != self.view_number {
    //         return Err("View number mismatch".to_string());
    //     }

    //     if msg.op_number > self.op_number {
    //         return Err("Invalid operation number".to_string());
    //     }

    //     if self.has_prepare_quorum(msg.op_number) {
    //         while self.commit_number < msg.op_number {
    //             self.commit_number += 1;
    //             self.execute_operation(self.commit_number);
    //         }
    //     }

    //     Ok(())
    // }

    // fn has_prepare_quorum(&self, op_number: op_number) -> bool {
    //     // Calculate quorum size (f + 1, where f = (n-1)/2)
    //     let total_replicas = self.replicas.len();
    //     let max_failures = (total_replicas - 1) / 2;
    //     let quorum_size = max_failures + 1;

    //     // In a real implementation, you would:
    //     // 1. Track prepare-ok messages for each operation
    //     // 2. Count unique replicas that have sent prepare-ok
    //     // 3. Return true if count >= quorum_size

    //     // For now, returning true to simulate quorum
    //     true
    // }

    // // Helper method to execute operation
    // fn execute_operation(&mut self, op_number: op_number) {
    //     if op_number <= 0 || op_number > self.log.len() as u64 {
    //         return;
    //     }

    //     // In a real implementation, you would:
    //     // 1. Get operation from log
    //     // 2. Execute it against your state machine
    //     // 3. Update client table with result
    //     // 4. Send response to client if this is the primary
    // }

    async fn broadcast_till_quorum<M>(&self, message: M, endpoint: &String) -> Result<(), ()>
    where
        M: Serialize + Clone + Send + 'static
    {
        let client = reqwest::Client::new();
        let mut futures = Vec::new();

        for replica in &self.replicas {
            let message = message.clone();
            let replica_addr = replica.address.clone() + endpoint;
            let replica_id = replica.id.clone();
            let client = client.clone();

            let future = async move {
                match client.post(&replica_addr)
                    .json(&message)
                    .send()
                    .await {
                        Ok(response) => {
                            if response.status().is_success() {
                                Ok(())
                            } else {
                                println!("Failed to send prepare to replica {}: HTTP {}", 
                                    replica_id, response.status());
                                Err(())
                            }
                        },
                        Err(e) => {
                            println!("Error sending prepare to replica {}: {}", 
                                replica_id, e);
                            Err(())
                        }
                    }
            };
            futures.push(future);
        }

        let handlers = futures.into_iter()
            .map(tokio::spawn)
            .collect::<Vec<_>>();

        let quorum_size = (self.replicas.len() / 2) + 1;
        let mut successful_responses = 0;

        stream::iter(handlers)
            .buffer_unordered(self.replicas.len())
            .take_while(|result| {
                match result {
                    Ok(Ok(())) => {
                        successful_responses += 1;
                        future::ready(successful_responses < quorum_size)
                    },
                    _ => future::ready(true),
                }
            })
            .collect::<Vec<_>>()
            .await;

        Ok(())
    }

    // async fn send_prepare_ok(&self, view_number: view_number, op_number: op_number) {
    //     let prepare_ok = PrepareOkMessage {
    //         view_number,
    //         op_number,
    //         commit_number: self.commit_number,
    //         replica_id: self.get_self_replica().id.clone(),
    //     };

    //     let primary = self.get_primary().clone();
    //     let primary_addr = format!("{}/prepare-ok", primary.address);

    //     let client = reqwest::Client::new();

    //     tokio::spawn(async move {
    //         match client.post(&primary_addr)
    //             .json(&prepare_ok)
    //             .send()
    //             .await {
    //                 Ok(response) => {
    //                     if !response.status().is_success() {
    //                         println!("Failed to send prepare-ok to primary {}: HTTP {}",
    //                             primary.id, response.status());
    //                     }
    //                 },
    //                 Err(e) => {
    //                     println!("Error sending prepare-ok to primary {}: {}",
    //                         primary.id, e);
    //                 }
    //             }
    //     });
    // }
}

#[tokio::main]
async fn main() {
    let replicas = vec![
        ReplicaInfo {
            id: "0".to_string(),
            address: "0.0.0.0:3000".to_string(),
        },
        ReplicaInfo {
            id: "1".to_string(),
            address: "0.0.0.0:3001".to_string(),
        },
        ReplicaInfo {
            id: "2".to_string(),
            address: "0.0.0.0:3002".to_string(),
        },
    ];

    let Ok(self_replica_env) = std::env::var("REPLICA_INDEX")
    else {
        println!("REPLICA_INDEX is not set on .env");
        return;
    };

    let Ok(self_replica_index) = self_replica_env.parse::<usize>()
    else {
        println!("REPLICA_INDEX is not a valid number on .env");
        return;
    };

    let replica_state = ViewstampedReplicaState::new(self_replica_index, replicas.clone());
    let shared_state = Arc::new(Mutex::new(replica_state));

    let app = Router::new()
        .route("/request", post(handle_client_request))
        .route("/prepare", post(handle_prepare))
        //.route("/prepare-ok", post(handle_prepare_ok))
        .route("/healthcheck", get(|| async { Json("{\"status\": \"ok\"}") }))
        .with_state(shared_state);

    let Ok(listener) = tokio::net::TcpListener::bind(replicas[self_replica_index].address.clone()).await
    else {
        println!("Failed to bind listener: {}", replicas[self_replica_index].address);
        return;
    };

    let Ok(_) = axum::serve(listener, app).await
    else {
        println!("Failed to serve");
        return;
    };
}

async fn handle_client_request(
    State(state): State<Arc<Mutex<ViewstampedReplicaState>>>,
    Json(request): Json<ClientRequest>,
) -> Json<Result<(), String>> {
    let mut state = state.lock().await;
    let handler = ViewstampedReplicaState::handle_client_request(&mut state, request);
    Json(handler.await)
}

async fn handle_prepare(
    State(state): State<Arc<Mutex<ViewstampedReplicaState>>>,
    Json(prepare): Json<PrepareMessage>,
) -> Json<Result<(), String>> {
    let mut state = state.lock().await;
    let handler = ViewstampedReplicaState::handle_prepare(&mut state, prepare);
    Json(handler.await)
}

// async fn handle_prepare_ok(
//     State(state): State<Arc<Mutex<ViewstampedReplicaState>>>,
//     Json(prepare_ok): Json<PrepareOkMessage>,
// ) -> Json<Result<(), String>> {
//     let mut state = state.lock().await;
//     ViewstampedReplicaState::handle_prepare_ok(&mut state, prepare_ok).await;
//     Json(Ok(()))
// }
