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
use sdv_comms_id_validator::topic_name_validation::TOPIC_NAME_MAX_LENGTH_BYTES;
use vsidl_v1::syntax::service_bundle::{Publisher, Subscriber};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct TopicNameTooLong {
    finding_collector: FindingCollector,
}

impl_context_condition!(TopicNameTooLong);

impl VsidlVisitor for TopicNameTooLong {
    fn start_visit_publisher(&mut self, node: &Publisher) {
        for (index, topic) in node.topic.iter().enumerate() {
            if topic.len() > TOPIC_NAME_MAX_LENGTH_BYTES {
                self.finding_collector.error(
                    FindingId::TopicNameTooLong,
                    format!(
                        "Topic '{}' exceeds the length limit of {} characters",
                        topic, TOPIC_NAME_MAX_LENGTH_BYTES
                    ),
                    vec![node.topic_span.get(index).cloned()],
                );
            }
        }
    }

    fn start_visit_subscriber(&mut self, node: &Subscriber) {
        for (index, topic) in node.topic.iter().enumerate() {
            if topic.len() > TOPIC_NAME_MAX_LENGTH_BYTES {
                self.finding_collector.error(
                    FindingId::TopicNameTooLong,
                    format!(
                        "Topic '{}' exceeds the length limit of {} characters",
                        topic, TOPIC_NAME_MAX_LENGTH_BYTES
                    ),
                    vec![node.topic_span.get(index).cloned()],
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
    fn test_should_reject_long_topic_names() {
        let long_topic = "a".repeat(128);
        let mut check = TopicNameTooLong::default();
        let mut publisher = Publisher::default();
        publisher.topic.push(long_topic.clone());

        check.start_visit_publisher(&publisher);
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            &format!("Topic '{}' exceeds the length limit of 127 characters", long_topic)
        ));

        let mut check = TopicNameTooLong::default();
        let mut subscriber = Subscriber::default();
        subscriber.topic.push(long_topic.clone());

        check.start_visit_subscriber(&subscriber);
        assert!(check.is_err());
    }

    #[test]
    fn test_should_accept_short_topic_names() {
        let short_topic = "a".repeat(127);
        let mut check = TopicNameTooLong::default();
        let mut publisher = Publisher::default();
        publisher.topic.push(short_topic.clone());

        check.start_visit_publisher(&publisher);
        assert!(check.is_ok());

        let mut check = TopicNameTooLong::default();
        let mut subscriber = Subscriber::default();
        subscriber.topic.push(short_topic.clone());

        check.start_visit_subscriber(&subscriber);
        assert!(check.is_ok());
    }
}
