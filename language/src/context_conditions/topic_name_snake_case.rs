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
use vsidl_v1::syntax::service_bundle::{Publisher, Subscriber};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct TopicNameSnakeCase {
    finding_collector: FindingCollector,
}

impl_context_condition!(TopicNameSnakeCase);

fn is_snake_case(s: &str) -> bool {
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
        if c == '_' {
            // Cannot have consecutive underscores
            if let Some(&next) = chars.peek() {
                if next == '_' {
                    return false;
                }
            } else {
                // Cannot end with an underscore
                return false;
            }
        } else if !c.is_ascii_lowercase() && !c.is_ascii_digit() {
            // Only lowercase alphanumeric and underscores allowed
            return false;
        }
    }
    true
}

impl VsidlVisitor for TopicNameSnakeCase {
    fn start_visit_publisher(&mut self, node: &Publisher) {
        for topic in &node.topic {
            if !is_snake_case(topic) {
                self.finding_collector.error(
                    FindingId::TopicNameSnakeCase,
                    format!("Topic '{}' must be in lowercase snake case", topic),
                    vec![node.span.clone().into_option()],
                );
            }
        }
    }

    fn start_visit_subscriber(&mut self, node: &Subscriber) {
        for topic in &node.topic {
            if !is_snake_case(topic) {
                self.finding_collector.error(
                    FindingId::TopicNameSnakeCase,
                    format!("Topic '{}' must be in lowercase snake case", topic),
                    vec![node.span.clone().into_option()],
                );
            }
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
            "kebab-case",
            "trailing_",
            "_leading",
            "double__underscore",
            "with space",
            "1starting_with_digit",
            "",
        ];

        for topic in test_cases {
            let mut check = TopicNameSnakeCase::default();
            let mut publisher = Publisher::default();
            publisher.topic.push(topic.to_string());

            check.start_visit_publisher(&publisher);
            assert!(check.is_err(), "Should have rejected invalid topic name: '{}'", topic);
            assert!(check.any_finding_contains(
                FindingSeverity::Error,
                &format!("Topic '{}' must be in lowercase snake case", topic)
            ));

            let mut check = TopicNameSnakeCase::default();
            let mut subscriber = Subscriber::default();
            subscriber.topic.push(topic.to_string());

            check.start_visit_subscriber(&subscriber);
            assert!(check.is_err(), "Should have rejected invalid topic name: '{}'", topic);
        }
    }

    #[test]
    fn test_should_accept_valid_topic_names() {
        let test_cases = vec!["valid", "snake_case", "with_digits_123", "v1"];

        for topic in test_cases {
            let mut check = TopicNameSnakeCase::default();
            let mut publisher = Publisher::default();
            publisher.topic.push(topic.to_string());

            check.start_visit_publisher(&publisher);
            assert!(check.is_ok(), "Should have accepted valid topic name: '{}'", topic);

            let mut check = TopicNameSnakeCase::default();
            let mut subscriber = Subscriber::default();
            subscriber.topic.push(topic.to_string());

            check.start_visit_subscriber(&subscriber);
            assert!(check.is_ok(), "Should have accepted valid topic name: '{}'", topic);
        }
    }
}
