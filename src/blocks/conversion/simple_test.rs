#[cfg(test)]
mod simple_conversion_tests {
    use crate::blocks::common::BlockHeader;
    use crate::blocks::conversion::base::ConversionBlock;
    use crate::blocks::conversion::types::ConversionType;

    #[test]
    fn test_simple_linear_conversion_no_references() {
        // Test that simple linear conversions with no references work exactly as before
        let mut conversion = ConversionBlock {
            header: BlockHeader {
                id: "##CC".to_string(),
                reserved: 0,
                length: 160,
                link_count: 4,
            },
            name_addr: None,
            unit_addr: None,
            comment_addr: None,
            inverse_addr: None,
            refs: vec![], // No references - simple conversion
            conversion_type: ConversionType::Linear,
            precision: 0,
            flags: 0,
            ref_count: 0,
            value_count: 2,
            phys_range_min: None,
            phys_range_max: None,
            values: vec![2.0, 3.0], // Linear: phys = 2.0 + 3.0 * raw
            formula: None,
            resolved_texts: None,
            resolved_conversions: None,
            default_conversion: None,
        };

        // Resolution should work without any issues
        let empty_file_data = vec![0u8; 1024];
        let result = conversion.resolve_all_dependencies(&empty_file_data);
        println!("Resolution result: {:?}", result);
        assert!(
            result.is_ok(),
            "Simple conversion resolution should succeed"
        );

        // No resolved data should be created for simple conversions
        assert!(
            conversion.resolved_texts.is_none(),
            "Should have no resolved texts"
        );
        assert!(
            conversion.resolved_conversions.is_none(),
            "Should have no resolved conversions"
        );
        assert!(
            conversion.default_conversion.is_none(),
            "Should have no default conversion"
        );
    }

    #[test]
    fn test_identity_conversion_no_references() {
        // Test identity conversion (most common case)
        let mut conversion = ConversionBlock {
            header: BlockHeader {
                id: "##CC".to_string(),
                reserved: 0,
                length: 160,
                link_count: 4,
            },
            name_addr: None,
            unit_addr: None,
            comment_addr: None,
            inverse_addr: None,
            refs: vec![], // No references
            conversion_type: ConversionType::Identity,
            precision: 0,
            flags: 0,
            ref_count: 0,
            value_count: 0,
            phys_range_min: None,
            phys_range_max: None,
            values: vec![], // No conversion values needed for identity
            formula: None,
            resolved_texts: None,
            resolved_conversions: None,
            default_conversion: None,
        };

        let empty_file_data = vec![0u8; 1024];
        let result = conversion.resolve_all_dependencies(&empty_file_data);
        println!("Identity conversion resolution result: {:?}", result);
        assert!(
            result.is_ok(),
            "Identity conversion resolution should succeed"
        );

        // No resolved data should be created
        assert!(
            conversion.resolved_texts.is_none(),
            "Should have no resolved texts"
        );
        assert!(
            conversion.resolved_conversions.is_none(),
            "Should have no resolved conversions"
        );
        assert!(
            conversion.default_conversion.is_none(),
            "Should have no default conversion"
        );
    }

    #[test]
    fn test_value_to_text_with_non_zero_addresses() {
        // Test ValueToText conversion with non-zero addresses (avoiding null link issue)
        let mut file_data = Vec::new();

        // Create text blocks at non-zero addresses
        let text1_addr = 100u64;
        let text2_addr = 200u64;

        // Text block 1 at address 100
        let text1_content = b"Option_A";
        file_data.resize(100, 0); // Pad to address 100

        // TX block header
        file_data.extend_from_slice(b"##TX"); // id (4 bytes)
        file_data.extend_from_slice(&[0u8; 4]); // reserved (4 bytes)
        file_data.extend_from_slice(&((24 + text1_content.len()) as u64).to_le_bytes()); // block_len (8 bytes)
        file_data.extend_from_slice(&0u64.to_le_bytes()); // links_nr (8 bytes)
        file_data.extend_from_slice(text1_content); // text content

        // Text block 2 at address 200
        file_data.resize(200, 0); // Pad to address 200
        let text2_content = b"Option_B";
        file_data.extend_from_slice(b"##TX"); // id (4 bytes)
        file_data.extend_from_slice(&[0u8; 4]); // reserved (4 bytes)
        file_data.extend_from_slice(&((24 + text2_content.len()) as u64).to_le_bytes()); // block_len (8 bytes)
        file_data.extend_from_slice(&0u64.to_le_bytes()); // links_nr (8 bytes)
        file_data.extend_from_slice(text2_content); // text content

        let mut conversion = ConversionBlock {
            header: BlockHeader {
                id: "##CC".to_string(),
                reserved: 0,
                length: 200,
                link_count: 6, // 4 fixed + 2 references
            },
            name_addr: None,
            unit_addr: None,
            comment_addr: None,
            inverse_addr: None,
            refs: vec![text1_addr, text2_addr], // Two text references
            conversion_type: ConversionType::ValueToText,
            precision: 0,
            flags: 0,
            ref_count: 2,
            value_count: 2,
            phys_range_min: None,
            phys_range_max: None,
            values: vec![1.0, 2.0], // Maps 1.0->text1, 2.0->text2
            formula: None,
            resolved_texts: None,
            resolved_conversions: None,
            default_conversion: None,
        };

        // Resolution should work and create resolved texts
        let result = conversion.resolve_all_dependencies(&file_data);
        println!("ValueToText resolution result: {:?}", result);
        println!("Resolved texts: {:?}", conversion.resolved_texts);
        println!(
            "Resolved conversions: {:?}",
            conversion.resolved_conversions.is_some()
        );
        println!(
            "Default conversion: {:?}",
            conversion.default_conversion.is_some()
        );

        assert!(result.is_ok(), "Text conversion resolution should succeed");

        // Should have resolved texts but no conversions
        assert!(
            conversion.resolved_texts.is_some(),
            "Should have resolved texts"
        );
        assert!(
            conversion.resolved_conversions.is_none(),
            "Should have no resolved conversions for simple text refs"
        );
        assert!(
            conversion.default_conversion.is_none(),
            "Should have no default conversion"
        );

        // Check the resolved text content
        let resolved_texts = conversion.resolved_texts.as_ref().unwrap();
        println!("Resolved text contents: {:?}", resolved_texts);
        assert_eq!(resolved_texts.get(&0), Some(&"Option_A".to_string()));
        assert_eq!(resolved_texts.get(&1), Some(&"Option_B".to_string()));
    }
}
