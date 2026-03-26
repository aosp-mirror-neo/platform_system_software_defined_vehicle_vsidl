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
use vsidl_v1::someip::some_ip_mapping::SdvTypeMapping;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct SomeIpMappingTypeConflict {
    finding_collector: FindingCollector,
}
impl_context_condition!(SomeIpMappingTypeConflict);

impl VsidlVisitor for SomeIpMappingTypeConflict {
    fn start_visit_someip_sdv_type_mapping(&mut self, node: &SdvTypeMapping) {
        if !node.service_unit_mapping.is_empty() && node.service_oriented_mapping.is_some() {
            self.finding_collector.error(
                FindingId::SomeIpMappingTypeConflict,
                format!(
                    "SOME/IP service mapping for unit type '{}' defines both 'service_oriented_mapping' and 'service_unit_mapping'. These cannot be set together.",
                    node.unit_type.type_name
                ),
                None, vec![
                    node.service_oriented_mapping_span.clone().into_option(),
                    node.service_unit_mapping_span.first().cloned()
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_condition::ContextCondition;
    use crate::FindingSeverity;
    use vsidl_v1::someip::some_ip_mapping::{
        sdv_type_mapping::SdvServiceUnitMapping, sdv_type_mapping::ServiceOrientedMapping, UnitType,
    };
    use vsidl_v1::syntax::VsidlEntry;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_mixed_mapping_types() {
        // GIVEN a SdvTypeMapping with both mapping types
        let mut sdv_type_mapping = SdvTypeMapping::new();
        let mut unit_type = UnitType::new();
        unit_type.type_name = "MyType".to_string();
        sdv_type_mapping.unit_type = protobuf::MessageField::some(unit_type);

        sdv_type_mapping.service_unit_mapping.push(SdvServiceUnitMapping::new());
        sdv_type_mapping.service_oriented_mapping =
            protobuf::MessageField::some(ServiceOrientedMapping::new());

        let mut catalog = Catalog::new();
        let mut someip_mapping = vsidl_v1::someip::SomeIpMapping::new();
        someip_mapping.set_sdv_type_mapping(sdv_type_mapping);
        let mut vsidl_entry = VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        // WHEN executing context condition
        let mut check = SomeIpMappingTypeConflict::default();
        check.check_catalog(&catalog);

        // THEN an error should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "defines both 'service_oriented_mapping' and 'service_unit_mapping'"
        ));
    }

    #[test]
    fn test_valid_service_unit_mapping() {
        // GIVEN a SdvTypeMapping with only service_unit_mapping
        let mut sdv_type_mapping = SdvTypeMapping::new();
        sdv_type_mapping.service_unit_mapping.push(SdvServiceUnitMapping::new());

        let mut catalog = Catalog::new();
        let mut someip_mapping = vsidl_v1::someip::SomeIpMapping::new();
        someip_mapping.set_sdv_type_mapping(sdv_type_mapping);
        let mut vsidl_entry = VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        // WHEN executing context condition
        let mut check = SomeIpMappingTypeConflict::default();
        check.check_catalog(&catalog);

        // THEN no findings should be reported
        assert!(check.is_ok());
    }

    #[test]
    fn test_valid_service_oriented_mapping() {
        // GIVEN a SdvTypeMapping with only service_oriented_mapping
        let mut sdv_type_mapping = SdvTypeMapping::new();
        sdv_type_mapping.service_oriented_mapping =
            protobuf::MessageField::some(ServiceOrientedMapping::new());

        let mut catalog = Catalog::new();
        let mut someip_mapping = vsidl_v1::someip::SomeIpMapping::new();
        someip_mapping.set_sdv_type_mapping(sdv_type_mapping);
        let mut vsidl_entry = VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        // WHEN executing context condition
        let mut check = SomeIpMappingTypeConflict::default();
        check.check_catalog(&catalog);

        // THEN no findings should be reported
        assert!(check.is_ok());
    }
}
