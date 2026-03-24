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

#[derive(Default)]
pub struct UserDefinedNamesOnlyOneTopic {
    finding_collector: FindingCollector,
}
impl_context_condition!(UserDefinedNamesOnlyOneTopic);

// Checks that a publisher that serves multiple topics cannot use a user-defined service unit name
impl VsidlVisitor for UserDefinedNamesOnlyOneTopic {
    fn start_visit_publisher(&mut self, node: &Publisher) {
        if node.topic.len() > 1 && !&node.service_unit_name.is_empty() {
            self.finding_collector.error(
                FindingId::UserDefinedNamesOnlyOneChannel,
                format!("Publisher '{}' must not serve multiple topic names and have a user-defined name", node.service_unit_name.clone()),
                vec![node.service_unit_name_span.clone().into_option()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::user_defined_names_only_one_topic::UserDefinedNamesOnlyOneTopic;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_should_reject_publisher_with_two_topics_and_user_defined_name() {
        // Given: A catalog with a publisher that has two topics and a user-defined name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_PUBLISHER_WITH_TWO_CHANNELS_AND_USER_DEFINED_NAME.clone(),
        );

        // When: Executing CoCo
        let mut coco = UserDefinedNamesOnlyOneTopic::default();
        coco.check_catalog(&catalog);

        // Then: CoCo should fail
        assert!(coco.is_err());

        // Then: Error message contains the duplicated service
        assert!(coco.any_finding_contains(FindingSeverity::Error, "my-pub"));
    }
}
