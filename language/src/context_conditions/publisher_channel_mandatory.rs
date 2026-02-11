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
use vsidl_v1::syntax::service_bundle::Publisher;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct PublisherChannelMandatory {
    finding_collector: FindingCollector,
}

impl_context_condition!(PublisherChannelMandatory);

impl VsidlVisitor for PublisherChannelMandatory {
    fn start_visit_publisher(&mut self, node: &Publisher) {
        if node.channel.is_empty() {
            self.finding_collector.error(
                FindingId::PublisherChannelMandatory,
                "Publisher channel is mandatory in latest VSIDL syntax. Please specify 'channel: \"channel_name\"'",
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
    fn test_should_reject_missing_channel() {
        // GIVEN a publisher WITHOUT channels
        let mut check = PublisherChannelMandatory::default();
        let publisher = Publisher::default();

        // WHEN visiting the publisher
        check.start_visit_publisher(&publisher);

        // THEN it should be an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Publisher channel is mandatory in latest VSIDL syntax"
        ));
    }

    #[test]
    fn test_should_accept_provided_channel() {
        // GIVEN a publisher WITH channels
        let mut check = PublisherChannelMandatory::default();
        let publisher =
            Publisher { channel: vec!["some_channel".to_string()], ..Default::default() };

        // WHEN visiting the publisher
        check.start_visit_publisher(&publisher);

        // THEN it should be OK
        assert!(check.is_ok());
    }
}
