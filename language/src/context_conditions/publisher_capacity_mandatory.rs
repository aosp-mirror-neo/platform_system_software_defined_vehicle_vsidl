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
pub struct PublisherCapacityMandatory {
    finding_collector: FindingCollector,
}

impl_context_condition!(PublisherCapacityMandatory);

impl VsidlVisitor for PublisherCapacityMandatory {
    fn start_visit_publisher(&mut self, node: &Publisher) {
        // If the capacity is set neither by the AST transformation nor by the user.
        if node.capacity == 0 && node.capacity_span.is_none() {
            self.finding_collector.error(
                FindingId::PublisherCapacityMandatory,
                "Publisher capacity is mandatory in latest VSIDL syntax. Please specify 'capacity: <even_number_ge_2>'",
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
    use vsidl_v1::span::Span;

    #[test]
    fn test_should_reject_missing_capacity() {
        // GIVEN a publisher WITHOUT capacity_span (meaning it was not provided in the VSIDL)
        let mut check = PublisherCapacityMandatory::default();
        let publisher = Publisher::default(); // capacity_span is empty by default

        // WHEN visiting the publisher
        check.start_visit_publisher(&publisher);

        // THEN it should be an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Publisher capacity is mandatory in latest VSIDL syntax"
        ));
    }

    #[test]
    fn test_should_accept_provided_capacity() {
        // GIVEN a publisher WITH capacity_span (meaning it was provided)
        let mut check = PublisherCapacityMandatory::default();
        let publisher = Publisher {
            capacity: 8,
            capacity_span: protobuf::MessageField::some(Span::default()),
            ..Default::default()
        };

        // WHEN visiting the publisher
        check.start_visit_publisher(&publisher);

        // THEN it should be OK
        assert!(check.is_ok());
    }
}
