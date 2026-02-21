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
//! Converts the case of channel names in the AST

use crate::impl_traverse_catalog_mut;
use crate::visitors::visitor_mut::{AstNodeMut, VsidlVisitorMut};
use vsidl_v1::syntax::service_bundle::{Publisher, Subscriber};

pub enum ChannelCase {
    Upper,
    Lower,
}

pub struct ConvertChannelCase {
    pub case: ChannelCase,
}

impl_traverse_catalog_mut!(ConvertChannelCase);

impl Default for ConvertChannelCase {
    fn default() -> Self {
        Self { case: ChannelCase::Upper }
    }
}

impl VsidlVisitorMut for ConvertChannelCase {
    fn start_visit_publisher(&mut self, node: &mut Publisher) {
        for channel in &mut node.channel {
            match self.case {
                ChannelCase::Upper => *channel = channel.to_uppercase(),
                ChannelCase::Lower => *channel = channel.to_lowercase(),
            }
        }
    }

    fn start_visit_subscriber(&mut self, node: &mut Subscriber) {
        for channel in &mut node.channel {
            match self.case {
                ChannelCase::Upper => *channel = channel.to_uppercase(),
                ChannelCase::Lower => *channel = channel.to_lowercase(),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_data;
    use crate::Catalog;

    #[test]
    fn test_should_convert_channel_to_uppercase() {
        // GIVEN A catalog with a publisher using lowercase channel names
        let mut catalog = Catalog::new();
        catalog.catalog_vsidl_files.push(test_data::VSIDL_TIRE_PRESSURE_PUBLISHER.clone());

        // PRE-CONDITION: channel is "test"
        assert_eq!(
            catalog.catalog_vsidl_files[0].1.service_bundle[0].publisher[0].channel[0],
            "test"
        );

        // WHEN executing the visitor with Upper case
        let mut visitor = ConvertChannelCase { case: ChannelCase::Upper };
        catalog.accept(&mut visitor);

        // THEN channel name is converted to "TEST"
        assert_eq!(
            catalog.catalog_vsidl_files[0].1.service_bundle[0].publisher[0].channel[0],
            "TEST"
        );
    }

    #[test]
    fn test_should_convert_channel_to_lowercase() {
        // GIVEN A catalog with a publisher using uppercase channel names
        let mut catalog = Catalog::new();
        catalog.catalog_vsidl_files.push(test_data::VSIDL_ALL_AST_NODE_TYPES.clone());

        // PRE-CONDITION: channel is "ONE"
        assert_eq!(
            catalog.catalog_vsidl_files[0].1.service_bundle[0].publisher[0].channel[0],
            "ONE"
        );

        // WHEN executing the visitor with Lower case
        let mut visitor = ConvertChannelCase { case: ChannelCase::Lower };
        catalog.accept(&mut visitor);

        // THEN channel name is converted to "one"
        assert_eq!(
            catalog.catalog_vsidl_files[0].1.service_bundle[0].publisher[0].channel[0],
            "one"
        );
    }
}
