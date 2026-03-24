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
pub struct PublisherCapacity {
    finding_collector: FindingCollector,
}

impl_context_condition!(PublisherCapacity);

impl VsidlVisitor for PublisherCapacity {
    fn start_visit_publisher(&mut self, node: &Publisher) {
        if node.capacity_span.is_none() {
            // handled by PublisherCapacityMandatory
            return;
        }
        if node.capacity < 2 {
            self.finding_collector.error(
                FindingId::PublisherCapacity,
                format!("Publisher capacity must be at least 2, but was {}", node.capacity),
                vec![node.capacity_span.clone().into_option()],
            );
        } else if node.capacity > u32::MAX.into() {
            self.finding_collector.error(
                FindingId::PublisherCapacity,
                format!(
                    "Publisher capacity must be at most {}, but was {}",
                    u32::MAX,
                    node.capacity
                ),
                vec![node.capacity_span.clone().into_option()],
            );
        } else if node.capacity % 2 != 0 {
            self.finding_collector.error(
                FindingId::PublisherCapacity,
                format!("Publisher capacity must be an even number, but was {}", node.capacity),
                vec![node.capacity_span.clone().into_option()],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_condition::ContextCondition;
    use crate::finding_severity::FindingSeverity;
    use vsidl_v1::span::Span;

    #[test]
    fn test_should_reject_invalid_capacity() {
        let test_cases = vec![
            (-4, "Publisher capacity must be at least 2, but was -4"),
            (0, "Publisher capacity must be at least 2, but was 0"),
            (1, "Publisher capacity must be at least 2, but was 1"),
            (3, "Publisher capacity must be an even number, but was 3"),
            (101, "Publisher capacity must be an even number, but was 101"),
            (4294967296, "Publisher capacity must be at most 4294967295, but was 4294967296"),
        ];

        for (capacity, expected_msg) in test_cases {
            let mut check = PublisherCapacity::default();
            let publisher = Publisher {
                capacity,
                capacity_span: protobuf::MessageField::some(Span::default()),
                ..Default::default()
            };

            check.start_visit_publisher(&publisher);
            assert!(check.is_err(), "Should have rejected invalid capacity: {}", capacity);
            assert!(check.any_finding_contains(FindingSeverity::Error, expected_msg));
        }
    }

    #[test]
    fn test_should_accept_valid_capacity() {
        let test_cases = vec![2, 4, 8, 100, 1024, 4294967294];

        for capacity in test_cases {
            let mut check = PublisherCapacity::default();
            let publisher = Publisher {
                capacity,
                capacity_span: protobuf::MessageField::some(Span::default()),
                ..Default::default()
            };

            check.start_visit_publisher(&publisher);
            assert!(check.is_ok(), "Should have accepted valid capacity: {}", capacity);
        }
    }
}
