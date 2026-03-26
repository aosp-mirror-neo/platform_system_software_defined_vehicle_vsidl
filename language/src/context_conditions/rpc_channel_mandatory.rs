/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use super::{FindingCollector, FindingId};
use crate::impl_context_condition;
use vsidl_v1::syntax::service_bundle::{Client, Server};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct RpcChannelMandatory {
    finding_collector: FindingCollector,
}

impl_context_condition!(RpcChannelMandatory);

impl VsidlVisitor for RpcChannelMandatory {
    fn start_visit_server(&mut self, node: &Server) {
        if node.channel.is_empty() {
            self.finding_collector.error(
                FindingId::RpcChannelMandatory,
                "RPC server channel is mandatory. Please specify 'channel: \"channel-name\"'",
                None,
                vec![node.span.clone().into_option()],
            );
        }
    }

    fn start_visit_client(&mut self, node: &Client) {
        if node.channel.is_empty() {
            self.finding_collector.error(
                FindingId::RpcChannelMandatory,
                "RPC client channel is mandatory. Please specify 'channel: \"channel-name\"'",
                None,
                vec![node.span.clone().into_option()],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_condition::ContextCondition;
    use crate::finding_severity::FindingSeverity;

    #[test]
    fn test_should_reject_missing_channel_server() {
        // GIVEN an RPC server WITHOUT channel
        let mut check = RpcChannelMandatory::default();
        let server = Server::default();

        // WHEN visiting the server
        check.start_visit_server(&server);

        // THEN it should be an error
        assert!(check.is_err());
        assert!(
            check.any_finding_contains(FindingSeverity::Error, "RPC server channel is mandatory")
        );
    }

    #[test]
    fn test_should_accept_provided_channel_server() {
        // GIVEN an RPC server WITH channel
        let mut check = RpcChannelMandatory::default();
        let server = Server { channel: "main".to_string(), ..Default::default() };

        // WHEN visiting the server
        check.start_visit_server(&server);

        // THEN it should be OK
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_reject_missing_channel_client() {
        // GIVEN an RPC client WITHOUT channel
        let mut check = RpcChannelMandatory::default();
        let client = Client::default();

        // WHEN visiting the client
        check.start_visit_client(&client);

        // THEN it should be an error
        assert!(check.is_err());
        assert!(
            check.any_finding_contains(FindingSeverity::Error, "RPC client channel is mandatory")
        );
    }

    #[test]
    fn test_should_accept_provided_channel_client() {
        // GIVEN an RPC client WITH channel
        let mut check = RpcChannelMandatory::default();
        let client = Client { channel: "main".to_string(), ..Default::default() };

        // WHEN visiting the client
        check.start_visit_client(&client);

        // THEN it should be OK
        assert!(check.is_ok());
    }
}
