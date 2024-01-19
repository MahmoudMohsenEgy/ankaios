// Copyright (c) 2023 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::execution_command_proxy;
use crate::execution_command_proxy::GRPCExecutionRequestStreaming;
use crate::grpc_middleware_error::GrpcMiddlewareError;
use crate::state_change_proxy;
use api::proto;
use api::proto::agent_connection_client::AgentConnectionClient;
use api::proto::cli_connection_client::CliConnectionClient;
use api::proto::state_change_request::StateChangeRequestEnum;
use api::proto::AgentHello;

use common::communications_client::CommunicationsClient;
use common::communications_error::CommunicationMiddlewareError;
use common::execution_interface::ExecutionCommand;

use common::state_change_interface::StateChangeReceiver;

use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_stream::wrappers::ReceiverStream;

use async_trait::async_trait;

use url::Url;

const RECONNECT_TIMEOUT_SECONDS: u64 = 1;

enum ConnectionType {
    Agent,
    Cli,
}

pub struct GRPCCommunicationsClient {
    name: String,
    server_address: Url,
    connection_type: ConnectionType,
}

impl GRPCCommunicationsClient {
    pub fn new_agent_communication(name: String, server_address: Url) -> Self {
        Self {
            name,
            server_address,
            connection_type: ConnectionType::Agent,
        }
    }
    pub fn new_cli_communication(name: String, server_address: Url) -> Self {
        Self {
            name,
            server_address,
            connection_type: ConnectionType::Cli,
        }
    }
}

#[async_trait]
impl CommunicationsClient for GRPCCommunicationsClient {
    async fn run(
        &mut self,
        mut server_rx: StateChangeReceiver,
        agent_tx: Sender<ExecutionCommand>,
    ) -> Result<(), CommunicationMiddlewareError> {
        log::debug!("gRPC Communication Client starts.");

        // [impl->swdd~grpc-client-retries-connection~2]
        loop {
            let result = self.run_internal(&mut server_rx, &agent_tx).await;

            match self.connection_type {
                ConnectionType::Agent => {
                    log::warn!("Connection to server interrupted: '{:?}'", result);

                    use tokio::time::{sleep, Duration};
                    sleep(Duration::from_secs(RECONNECT_TIMEOUT_SECONDS)).await;
                }
                ConnectionType::Cli => {
                    match result {
                        // [impl->swdd~grpc-client-outputs-error-server-unavailability-for-cli-connection~1]
                        Err(GrpcMiddlewareError::ServerNotAvailable(err)) => {
                            log::debug!("No connection to the server: '{err}'");
                            return Err(CommunicationMiddlewareError(format!(
                                "Could not connect to Ankaios server on '{}'.",
                                self.server_address
                            )));
                        }
                        // [impl->swdd~grpc-client-outputs-error-server-connection-loss-for-cli-connection~1]
                        Err(GrpcMiddlewareError::ConnectionInterrupted(err)) => {
                            log::debug!(
                                "The connection to the Ankaios Server was interrupted: '{err}'"
                            );
                        }
                        _ => {
                            log::debug!("The connection to the Ankaios Server was closed.");
                        }
                    }
                    // [impl->swdd~grpc-client-never-retries-cli-connection~1]
                    break; // no retry of cli connection
                }
            }
        }

        Ok(())
    }
}

impl GRPCCommunicationsClient {
    /// This functions establishes the connection to the gRPC server and starts listening and forwarding messages
    /// on the two communications channels. The method returns only if the connection could not be established or
    /// is interrupted.
    async fn run_internal(
        &self,
        server_rx: &mut StateChangeReceiver,
        agent_tx: &Sender<ExecutionCommand>,
    ) -> Result<(), GrpcMiddlewareError> {
        // [impl->swdd~grpc-client-creates-state-change-channel~1]
        let (grpc_tx, grpc_rx) =
            tokio::sync::mpsc::channel::<proto::StateChangeRequest>(common::CHANNEL_CAPACITY);

        match self.connection_type {
            ConnectionType::Agent => {
                grpc_tx
                    .send(proto::StateChangeRequest {
                        state_change_request_enum: Some(StateChangeRequestEnum::AgentHello(
                            AgentHello {
                                agent_name: self.name.to_owned(),
                            },
                        )),
                    })
                    .await?;
            }
            ConnectionType::Cli => (), //no need to send AgentHello for Cli connection
        }

        // [impl->swdd~grpc-client-connects-with-agent-hello~1]
        let mut grpc_execution_request_streaming =
            GRPCExecutionRequestStreaming::new(self.connect_to_server(grpc_rx).await?);

        // [impl->swdd~grpc-client-forwards-commands-to-agent~1]
        let forward_exec_from_proto_task = execution_command_proxy::forward_from_proto_to_ankaios(
            self.name.as_str(),
            &mut grpc_execution_request_streaming,
            agent_tx,
        );

        // [impl->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
        let forward_state_change_from_ank_task =
            state_change_proxy::forward_from_ankaios_to_proto(grpc_tx, server_rx);

        select! {
            _ = forward_exec_from_proto_task => {log::debug!("Forward execution command from proto to Ankaios task completed");}
            _ = forward_state_change_from_ank_task => {log::debug!("Forward execution command from Ankaios to proto task completed");}
        };

        Ok(())
    }

    async fn connect_to_server(
        &self,
        grpc_rx: Receiver<proto::StateChangeRequest>,
    ) -> Result<tonic::Streaming<proto::ExecutionRequest>, GrpcMiddlewareError> {
        match self.connection_type {
            ConnectionType::Agent => {
                let mut client =
                    AgentConnectionClient::connect(self.server_address.to_string()).await?;

                let res = client
                    .connect_agent(ReceiverStream::new(grpc_rx))
                    .await?
                    .into_inner();
                Ok(res)
            }
            ConnectionType::Cli => {
                let mut client =
                    CliConnectionClient::connect(self.server_address.to_string()).await?;

                let res = client
                    .connect_cli(ReceiverStream::new(grpc_rx))
                    .await?
                    .into_inner();
                Ok(res)
            }
        }
    }
}
