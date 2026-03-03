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
pub struct RpcChannelNamingConvention {
    finding_collector: FindingCollector,
}

impl_context_condition!(RpcChannelNamingConvention);

fn is_kebab_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars().peekable();

    // Must start with a lowercase letter
    if let Some(c) = chars.next() {
        if !c.is_ascii_lowercase() {
            return false;
        }
    }

    while let Some(c) = chars.next() {
        if c == '-' {
            // Cannot have consecutive hyphens
            if let Some(&next) = chars.peek() {
                if next == '-' {
                    return false;
                }
            } else {
                // Cannot end with a hyphen
                return false;
            }
        } else if !c.is_ascii_lowercase() && !c.is_ascii_digit() {
            // Only lowercase alphanumeric and hyphens allowed
            return false;
        }
    }
    true
}

impl VsidlVisitor for RpcChannelNamingConvention {
    fn start_visit_server(&mut self, node: &Server) {
        if !node.channel.is_empty() && !is_kebab_case(&node.channel) {
            self.finding_collector.error(
                FindingId::RpcChannelNamingConvention,
                format!("RPC channel '{}' must be in lowercase kebab case", node.channel),
                vec![node.span.clone().into_option()],
            );
        }
    }

    fn start_visit_client(&mut self, node: &Client) {
        if !node.channel.is_empty() && !is_kebab_case(&node.channel) {
            self.finding_collector.error(
                FindingId::RpcChannelNamingConvention,
                format!("RPC channel '{}' must be in lowercase kebab case", node.channel),
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
    fn test_should_reject_invalid_channel_names() {
        let test_cases = vec![
            "UPPERCASE",
            "camelCase",
            "PascalCase",
            "snake_case",
            "trailing-",
            "-leading",
            "double--hyphen",
            "with space",
            "1starting-with-digit",
        ];

        for channel in test_cases {
            let mut check = RpcChannelNamingConvention::default();
            let server = Server { channel: channel.to_string(), ..Default::default() };
            check.start_visit_server(&server);
            assert!(
                check.is_err(),
                "Should have rejected invalid server channel name: '{}'",
                channel
            );
            assert!(check.any_finding_contains(
                FindingSeverity::Error,
                &format!("RPC channel '{}' must be in lowercase kebab case", channel)
            ));

            let mut check = RpcChannelNamingConvention::default();
            let client = Client { channel: channel.to_string(), ..Default::default() };
            check.start_visit_client(&client);
            assert!(
                check.is_err(),
                "Should have rejected invalid client channel name: '{}'",
                channel
            );
        }
    }

    #[test]
    fn test_should_accept_valid_channel_names() {
        let test_cases = vec!["valid", "kebab-case", "with-digits-123", "v1"];

        for channel in test_cases {
            let mut check = RpcChannelNamingConvention::default();
            let server = Server { channel: channel.to_string(), ..Default::default() };
            check.start_visit_server(&server);
            assert!(check.is_ok(), "Should have accepted valid server channel name: '{}'", channel);

            let mut check = RpcChannelNamingConvention::default();
            let client = Client { channel: channel.to_string(), ..Default::default() };
            check.start_visit_client(&client);
            assert!(check.is_ok(), "Should have accepted valid client channel name: '{}'", channel);
        }
    }

    #[test]
    fn test_should_ignore_empty_channel() {
        // Empty channel is handled by RpcChannelMandatory
        let mut check = RpcChannelNamingConvention::default();
        let server = Server::default();
        check.start_visit_server(&server);
        assert!(check.is_ok());

        let mut check = RpcChannelNamingConvention::default();
        let client = Client::default();
        check.start_visit_client(&client);
        assert!(check.is_ok());
    }
}
