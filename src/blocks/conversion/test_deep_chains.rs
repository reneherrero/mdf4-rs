#[cfg(test)]
mod tests {
    use crate::blocks::common::{BlockHeader, BlockParse};
    use crate::blocks::conversion::base::ConversionBlock;
    use crate::blocks::conversion::types::ConversionType;

    #[test]
    fn test_deep_conversion_chain_resolution() {
        // This test verifies that deep conversion chains are properly resolved
        // We'll simulate: ValueToText -> references another ValueToText -> references final Text

        let mut file_data = Vec::new();

        // Block 1: Root conversion (ValueToText with 2 values + 1 default = 3 total references)
        // The first 2 references are for the values, the 3rd is default (for out-of-range values)
        let root_conversion = create_test_conversion_block(
            ConversionType::ValueToText,
            vec![1.0, 2.0],      // Two value mappings
            vec![100, 200, 300], // Two regular references + one default reference
        );
        file_data.extend_from_slice(&root_conversion);

        // Block 2: First referenced conversion (starts at offset 100)
        while file_data.len() < 100 {
            file_data.push(0);
        }
        let first_ref_conversion = create_test_conversion_block(
            ConversionType::ValueToText,
            vec![1.0], // Single value mapping
            vec![400], // Reference to final text
        );
        file_data.extend_from_slice(&first_ref_conversion);

        // Block 3: Second referenced conversion (starts at offset 200)
        while file_data.len() < 200 {
            file_data.push(0);
        }
        let second_ref_conversion = create_test_conversion_block(
            ConversionType::ValueToText,
            vec![2.0], // Single value mapping
            vec![500], // Reference to another final text
        );
        file_data.extend_from_slice(&second_ref_conversion);

        // Block 4: Default conversion (starts at offset 300) - this will be the default
        while file_data.len() < 300 {
            file_data.push(0);
        }
        let default_conversion = create_test_conversion_block(
            ConversionType::ValueToText,
            vec![999.0], // Default value mapping
            vec![600],   // Reference to default text
        );
        file_data.extend_from_slice(&default_conversion);

        // Add the final text blocks
        for (offset, text) in [(400, "Text_1"), (500, "Text_2"), (600, "Default_Text")] {
            while file_data.len() < offset {
                file_data.push(0);
            }
            let text_block = create_test_text_block(text);
            file_data.extend_from_slice(&text_block);
        }

        // Create and resolve the root conversion
        let mut root_conv = ConversionBlock::from_bytes(&file_data[0..]).unwrap();

        // This should resolve the entire chain
        let result = root_conv.resolve_all_dependencies(&file_data);
        println!("Resolution result: {:?}", result);
        assert!(result.is_ok(), "Deep chain resolution should succeed");

        // Debug output
        println!("Root conversion type: {:?}", root_conv.conversion_type);
        println!("Root refs: {:?}", root_conv.refs);
        println!("Root values: {:?}", root_conv.values);
        println!("Resolved texts: {:?}", root_conv.resolved_texts.is_some());
        println!(
            "Resolved conversions: {:?}",
            root_conv.resolved_conversions.is_some()
        );
        println!(
            "Default conversion: {:?}",
            root_conv.default_conversion.is_some()
        );

        // For ValueToText with multiple refs, we should have both resolved conversions and default
        assert!(
            root_conv.resolved_conversions.is_some() || root_conv.default_conversion.is_some(),
            "Should have resolved conversions or default conversion"
        );

        // Check depth of resolution
        if let Some(resolved_conversions) = &root_conv.resolved_conversions {
            for (idx, conv) in resolved_conversions.iter() {
                println!(
                    "Resolved conversion {}: has texts={:?}, has conversions={:?}",
                    idx,
                    conv.resolved_texts.is_some(),
                    conv.resolved_conversions.is_some()
                );
                // Each resolved conversion should have resolved its own dependencies
                assert!(
                    conv.resolved_texts.is_some()
                        || conv.resolved_conversions.is_some()
                        || conv.default_conversion.is_some(),
                    "Nested conversion should have resolved dependencies"
                );
            }
        }

        if let Some(default_conv) = &root_conv.default_conversion {
            println!(
                "Default conversion: has texts={:?}, has conversions={:?}",
                default_conv.resolved_texts.is_some(),
                default_conv.resolved_conversions.is_some()
            );
            // Default conversion should also have resolved its dependencies
            assert!(
                default_conv.resolved_texts.is_some()
                    || default_conv.resolved_conversions.is_some()
                    || default_conv.default_conversion.is_some(),
                "Default conversion should have resolved dependencies"
            );
        }
    }
    #[test]
    fn test_conversion_chain_cycle_detection() {
        // Test that cycles in conversion chains are detected and prevented
        let mut file_data = Vec::new();

        // Block 1: Refers to Block 2 (offset 200) - creating first part of cycle
        // Start at offset 100 to avoid using address 0 (which is treated as null)
        while file_data.len() < 100 {
            file_data.push(0);
        }
        let conv1 = create_test_conversion_block(
            ConversionType::BitfieldText,
            vec![1.0], // Single mask
            vec![200], // Reference to second conversion
        );
        file_data.extend_from_slice(&conv1);

        // Block 2: Refers back to Block 1 (offset 100) - creating the cycle
        while file_data.len() < 200 {
            file_data.push(0);
        }
        let conv2 = create_test_conversion_block(
            ConversionType::BitfieldText,
            vec![1.0], // Single mask
            vec![100], // Points back to first block - this creates the cycle
        );
        file_data.extend_from_slice(&conv2);

        let mut root_conv = ConversionBlock::from_bytes(&file_data[100..]).unwrap();

        // This should detect the cycle and return an error
        // Pass the address 100 as the starting address of the root conversion
        let result = root_conv.resolve_all_dependencies_with_address(&file_data, 100);
        assert!(
            result.is_err(),
            "Cycle detection should catch the circular reference"
        );

        if let Err(crate::error::Error::ConversionChainCycle { address }) = result {
            assert_eq!(address, 100, "Should identify the cyclic address");
        } else {
            panic!("Should return ConversionChainCycle error");
        }
    }

    #[test]
    fn test_conversion_chain_depth_limit() {
        // Test that excessively deep chains are rejected
        let mut file_data = Vec::new();
        let mut current_offset = 0;

        // Create a chain of 25 conversions (exceeding MAX_DEPTH of 20)
        for i in 0..25 {
            while file_data.len() < current_offset {
                file_data.push(0);
            }

            let next_offset = if i < 24 { current_offset + 100 } else { 0 };
            let conv = create_test_conversion_block(
                ConversionType::ValueToText,
                vec![1.0],
                if next_offset > 0 {
                    vec![next_offset]
                } else {
                    vec![]
                },
            );
            file_data.extend_from_slice(&conv);
            current_offset += 100;
        }

        let mut root_conv = ConversionBlock::from_bytes(&file_data[0..]).unwrap();

        // This should hit the depth limit
        let result = root_conv.resolve_all_dependencies(&file_data);
        assert!(result.is_err(), "Depth limit should be enforced");

        if let Err(crate::error::Error::ConversionChainTooDeep { max_depth }) = result {
            assert_eq!(max_depth, 20, "Should report the correct maximum depth");
        } else {
            panic!("Should return ConversionChainTooDeep error");
        }
    }

    #[test]
    fn test_default_conversion_resolution() {
        // Test that default conversions are properly identified and resolved
        let mut file_data = Vec::new();

        // Create a conversion with multiple references where we expect some to be regular refs
        // and potentially one default (for ValueToText, the behavior depends on our logic)
        let conv = create_test_conversion_block(
            ConversionType::ValueToText,
            vec![1.0, 2.0],      // Two value mappings
            vec![100, 200, 300], // Three references
        );
        file_data.extend_from_slice(&conv);

        // Add referenced blocks
        for offset in [100, 200, 300] {
            while file_data.len() < offset {
                file_data.push(0);
            }
            let text_block = create_test_text_block(&format!("Text_{}", offset));
            file_data.extend_from_slice(&text_block);
        }

        let mut root_conv = ConversionBlock::from_bytes(&file_data[0..]).unwrap();
        let result = root_conv.resolve_all_dependencies(&file_data);
        assert!(
            result.is_ok(),
            "Resolution with default conversion should succeed"
        );

        // For ValueToText, the last reference might be treated as default
        // This depends on the specific conversion type implementation
        assert!(
            root_conv.resolved_texts.is_some() || root_conv.default_conversion.is_some(),
            "Should have resolved either texts or default conversion"
        );
    }

    // Helper function to create a test conversion block
    fn create_test_conversion_block(
        conv_type: ConversionType,
        values: Vec<f64>,
        refs: Vec<usize>,
    ) -> Vec<u8> {
        let mut block = Vec::new();

        // Block header (24 bytes)
        let header = BlockHeader {
            id: "##CC".to_string(),
            reserved: 0,
            length: (24 + 4 * 8 + refs.len() * 8 + 8 + values.len() * 8) as u64,
            link_count: (4 + refs.len()) as u64,
        };
        block.extend_from_slice(&header.to_bytes().unwrap());

        // Links section (4 fixed + refs.len() variable)
        for _ in 0..4 {
            block.extend_from_slice(&0u64.to_le_bytes()); // Fixed links
        }
        for &ref_addr in &refs {
            block.extend_from_slice(&(ref_addr as u64).to_le_bytes());
        }

        // Data section
        block.push(conv_type.to_u8()); // conversion_type
        block.push(0); // precision
        block.extend_from_slice(&0u16.to_le_bytes()); // flags
        block.extend_from_slice(&(refs.len() as u16).to_le_bytes()); // ref_count
        block.extend_from_slice(&(values.len() as u16).to_le_bytes()); // value_count

        // values
        for val in values {
            block.extend_from_slice(&val.to_le_bytes());
        }

        block
    }

    // Helper function to create a test text block
    fn create_test_text_block(text: &str) -> Vec<u8> {
        let mut block = Vec::new();

        // Text block header
        let header = BlockHeader {
            id: "##TX".to_string(),
            reserved: 0,
            length: (24 + text.len()) as u64,
            link_count: 0,
        };
        block.extend_from_slice(&header.to_bytes().unwrap());

        // Text content
        block.extend_from_slice(text.as_bytes());

        block
    }
}
