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
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::Event;
use vsidl_v1::syntax::service_bundle::{Publisher, Subscriber};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct TopicMandatory {
    finding_collector: FindingCollector,
}

impl_context_condition!(TopicMandatory);

impl VsidlVisitor for TopicMandatory {
    fn start_visit_publisher(&mut self, node: &Publisher) {
        if node.topic.is_empty() {
            self.finding_collector.error(
                FindingId::TopicMandatory,
                "Publisher topic is mandatory. Please specify 'topic: \"topic-name\"'",
                Some("Topic is mandatory. Please specify 'topic: \"topic-name\"'".to_string()),
                vec![node.span.clone().into_option()],
            );
        }
    }

    fn start_visit_subscriber(&mut self, node: &Subscriber) {
        if node.topic.is_empty() {
            self.finding_collector.error(
                FindingId::TopicMandatory,
                "Subscriber topic is mandatory. Please specify 'topic: \"topic-name\"'",
                Some("Topic is mandatory. Please specify 'topic: \"topic-name\"'".to_string()),
                vec![node.span.clone().into_option()],
            );
        }
    }

    fn start_visit_diagnostics_event(&mut self, node: &Event) {
        if node.topic.is_empty() {
            self.finding_collector.error(
                FindingId::TopicMandatory,
                "Event topic is mandatory. Please specify 'topic: \"topic-name\"'",
                None,
                vec![],
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
    fn test_should_reject_publisher_missing_topic() {
        // GIVEN a publisher WITHOUT topics
        let mut check = TopicMandatory::default();
        let publisher = Publisher::default();

        // WHEN visiting the publisher
        check.start_visit_publisher(&publisher);

        // THEN it should be an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "Publisher topic is mandatory"));
    }

    #[test]
    fn test_should_accept_publisher_provided_topic() {
        // GIVEN a publisher WITH topics
        let mut check = TopicMandatory::default();
        let publisher = Publisher { topic: vec!["some-topic".to_string()], ..Default::default() };

        // WHEN visiting the publisher
        check.start_visit_publisher(&publisher);

        // THEN it should be OK
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_reject_subscriber_missing_topic() {
        // GIVEN a subscriber WITHOUT topics
        let mut check = TopicMandatory::default();
        let subscriber = Subscriber::default();

        // WHEN visiting the subscriber
        check.start_visit_subscriber(&subscriber);

        // THEN it should be an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "Subscriber topic is mandatory"));
    }

    #[test]
    fn test_should_accept_subscriber_provided_topic() {
        // GIVEN a subscriber WITH topics
        let mut check = TopicMandatory::default();
        let subscriber = Subscriber { topic: vec!["some-topic".to_string()], ..Default::default() };

        // WHEN visiting the subscriber
        check.start_visit_subscriber(&subscriber);

        // THEN it should be OK
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_reject_event_missing_topic() {
        // GIVEN a diagnostics event WITHOUT a topic
        let mut check = TopicMandatory::default();
        let event = Event::default();

        // WHEN visiting the event
        check.start_visit_diagnostics_event(&event);

        // THEN it should be an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "Event topic is mandatory"));
    }

    #[test]
    fn test_should_accept_event_provided_topic() {
        // GIVEN a diagnostics event WITH a topic
        let mut check = TopicMandatory::default();
        let event = Event { topic: "some-topic".to_string(), ..Default::default() };

        // WHEN visiting the event
        check.start_visit_diagnostics_event(&event);

        // THEN it should be OK
        assert!(check.is_ok());
    }
}
