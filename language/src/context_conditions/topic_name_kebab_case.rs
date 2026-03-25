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
use sdv_comms_id_validator::topic_name_validation::validate_topic_name;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::Event;
use vsidl_v1::syntax::service_bundle::{Publisher, Subscriber};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct TopicNameKebabCase {
    finding_collector: FindingCollector,
}

impl_context_condition!(TopicNameKebabCase);

fn is_kebab_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    validate_topic_name(s).is_ok()
}

impl VsidlVisitor for TopicNameKebabCase {
    fn start_visit_publisher(&mut self, node: &Publisher) {
        for topic in &node.topic {
            if !is_kebab_case(topic) {
                self.finding_collector.error(
                    FindingId::TopicNameKebabCase,
                    format!("Topic '{}' must be in lowercase kebab case", topic),
                    vec![node.span.clone().into_option()],
                );
            }
        }
    }

    fn start_visit_subscriber(&mut self, node: &Subscriber) {
        for topic in &node.topic {
            if !is_kebab_case(topic) {
                self.finding_collector.error(
                    FindingId::TopicNameKebabCase,
                    format!("Topic '{}' must be in lowercase kebab case", topic),
                    vec![node.span.clone().into_option()],
                );
            }
        }
    }

    fn start_visit_diagnostics_event(&mut self, node: &Event) {
        if !is_kebab_case(&node.topic) {
            self.finding_collector.error(
                FindingId::TopicNameKebabCase,
                format!("Topic '{}' must be in lowercase kebab case", &node.topic),
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
    fn test_should_reject_invalid_topic_names() {
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
            "",
        ];

        for topic in test_cases {
            let mut check = TopicNameKebabCase::default();
            let mut publisher = Publisher::default();
            publisher.topic.push(topic.to_string());

            check.start_visit_publisher(&publisher);
            assert!(check.is_err(), "Should have rejected invalid topic name: '{}'", topic);
            assert!(check.any_finding_contains(
                FindingSeverity::Error,
                &format!("Topic '{}' must be in lowercase kebab case", topic)
            ));

            let mut check = TopicNameKebabCase::default();
            let mut subscriber = Subscriber::default();
            subscriber.topic.push(topic.to_string());

            check.start_visit_subscriber(&subscriber);
            assert!(check.is_err(), "Should have rejected invalid topic name: '{}'", topic);

            // GIVEN a diagnostics event WITHOUT a topic
            let mut check = TopicNameKebabCase::default();
            let event = Event { topic: topic.to_string(), ..Default::default() };

            // WHEN visiting the event
            check.start_visit_diagnostics_event(&event);
            assert!(check.is_err(), "Should have rejected invalid topic name: '{}'", topic);
        }
    }

    #[test]
    fn test_should_accept_valid_topic_names() {
        let test_cases = vec!["valid", "kebab-case", "with-digits-123", "v1"];

        for topic in test_cases {
            let mut check = TopicNameKebabCase::default();
            let mut publisher = Publisher::default();
            publisher.topic.push(topic.to_string());

            check.start_visit_publisher(&publisher);
            assert!(check.is_ok(), "Should have accepted valid topic name: '{}'", topic);

            let mut check = TopicNameKebabCase::default();
            let mut subscriber = Subscriber::default();
            subscriber.topic.push(topic.to_string());

            check.start_visit_subscriber(&subscriber);
            assert!(check.is_ok(), "Should have accepted valid topic name: '{}'", topic);

            // GIVEN a diagnostics event WITHOUT a topic
            let mut check = TopicNameKebabCase::default();
            let event = Event { topic: topic.to_string(), ..Default::default() };

            // WHEN visiting the event
            check.start_visit_diagnostics_event(&event);
            assert!(check.is_ok(), "Should have accepted valid topic name: '{}'", topic);
        }
    }
}
