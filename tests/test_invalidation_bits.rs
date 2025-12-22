use mdf4_rs::blocks::{BlockHeader, ChannelBlock};
use mdf4_rs::parsing::decoder::{check_value_validity, decode_channel_value_with_validity};
use mdf4_rs::{DataType, DecodedValue};

fn create_test_channel(flags: u32, pos_invalidation_bit: u32) -> ChannelBlock {
    ChannelBlock {
        header: BlockHeader {
            id: "##CN".to_string(),
            reserved: 0,
            length: 160,
            link_count: 8,
        },
        next_ch_addr: 0,
        component_addr: 0,
        name_addr: 0,
        source_addr: 0,
        conversion_addr: 0,
        data_addr: 0,
        unit_addr: 0,
        comment_addr: 0,
        channel_type: 0,
        sync_type: 0,
        data_type: DataType::UnsignedIntegerLE,
        bit_offset: 0,
        byte_offset: 0,
        bit_count: 16,
        flags,
        pos_invalidation_bit,
        precision: 0,
        reserved1: 0,
        attachment_count: 0,
        min_raw_value: 0.0,
        max_raw_value: 0.0,
        lower_limit: 0.0,
        upper_limit: 0.0,
        lower_ext_limit: 0.0,
        upper_ext_limit: 0.0,
        name: None,
        conversion: None,
    }
}

#[test]
fn test_all_values_invalid_flag() {
    let channel = create_test_channel(0x01, 0);
    let record = vec![0xFF, 0x12, 0x34, 0x00, 0x00, 0x00];
    let is_valid = check_value_validity(&record, 1, 4, &channel);
    assert_eq!(
        is_valid, false,
        "When cn_flags bit 0 is set, all values should be invalid"
    );
}

#[test]
fn test_all_values_valid_flag() {
    let channel = create_test_channel(0x00, 0);
    let record = vec![0xFF, 0x12, 0x34, 0x00, 0x00, 0xFF];
    let is_valid = check_value_validity(&record, 1, 4, &channel);
    assert_eq!(
        is_valid, true,
        "When cn_flags bits 0 and 1 are clear, all values should be valid"
    );
}

#[test]
fn test_invalidation_bit_position_0_set() {
    let channel = create_test_channel(0x02, 0);
    let record = vec![0xFF, 0x12, 0x34, 0x00, 0x00, 0x01];
    let is_valid = check_value_validity(&record, 1, 4, &channel);
    assert_eq!(
        is_valid, false,
        "When invalidation bit is set, value should be invalid"
    );
}

#[test]
fn test_invalidation_bit_position_0_clear() {
    let channel = create_test_channel(0x02, 0);
    let record = vec![0xFF, 0x12, 0x34, 0x00, 0x00, 0x00];
    let is_valid = check_value_validity(&record, 1, 4, &channel);
    assert_eq!(
        is_valid, true,
        "When invalidation bit is clear, value should be valid"
    );
}

#[test]
fn test_invalidation_bit_position_5() {
    let channel = create_test_channel(0x02, 5);
    let record = vec![0xFF, 0x12, 0x34, 0x00, 0x00, 0x20];
    let is_valid = check_value_validity(&record, 1, 4, &channel);
    assert_eq!(
        is_valid, false,
        "When invalidation bit 5 is set, value should be invalid"
    );
}

#[test]
fn test_invalidation_bit_position_in_second_byte() {
    let channel = create_test_channel(0x02, 10);
    let record = vec![0xFF, 0x12, 0x34, 0x00, 0x00, 0x00, 0x04];
    let is_valid = check_value_validity(&record, 1, 4, &channel);
    assert_eq!(
        is_valid, false,
        "When invalidation bit in second byte is set, value should be invalid"
    );
}

#[test]
fn test_decode_with_validity_valid_sample() {
    let mut channel = create_test_channel(0x02, 0);
    channel.byte_offset = 0;
    channel.bit_offset = 0;
    channel.bit_count = 16;
    channel.data_type = DataType::UnsignedIntegerLE;

    let record = vec![0xFF, 0x12, 0x34, 0x00];
    let result = decode_channel_value_with_validity(&record, 1, 2, &channel);

    assert!(result.is_some());
    let decoded = result.unwrap();
    assert_eq!(decoded.is_valid, true);
    assert_eq!(decoded.value, DecodedValue::UnsignedInteger(0x3412));
}

#[test]
fn test_decode_with_validity_invalid_sample() {
    let mut channel = create_test_channel(0x02, 0);
    channel.byte_offset = 0;
    channel.bit_offset = 0;
    channel.bit_count = 16;
    channel.data_type = DataType::UnsignedIntegerLE;

    let record = vec![0xFF, 0x12, 0x34, 0x01];
    let result = decode_channel_value_with_validity(&record, 1, 2, &channel);

    assert!(result.is_some());
    let decoded = result.unwrap();
    assert_eq!(
        decoded.is_valid, false,
        "Sample should be marked as invalid"
    );
    assert_eq!(decoded.value, DecodedValue::UnsignedInteger(0x3412));
}

#[test]
fn test_no_invalidation_bytes_available() {
    let channel = create_test_channel(0x02, 0);
    let record = vec![0xFF, 0x12, 0x34];
    let is_valid = check_value_validity(&record, 1, 4, &channel);
    assert_eq!(
        is_valid, true,
        "When invalidation bytes are not available, should assume valid"
    );
}

#[test]
fn test_sorted_data_no_record_id() {
    let channel = create_test_channel(0x02, 0);
    let record = vec![0x12, 0x34, 0x00, 0x00, 0x00];
    let is_valid = check_value_validity(&record, 0, 4, &channel);
    assert_eq!(
        is_valid, true,
        "Should work correctly with sorted data (no record ID)"
    );
}

#[test]
fn test_multiple_invalidation_bits() {
    let channel1 = create_test_channel(0x02, 0);
    let channel2 = create_test_channel(0x02, 1);
    let record = vec![0xFF, 0x12, 0x34, 0x00, 0x00, 0x01];

    let is_valid1 = check_value_validity(&record, 1, 4, &channel1);
    let is_valid2 = check_value_validity(&record, 1, 4, &channel2);

    assert_eq!(is_valid1, false, "Channel 1 (bit 0) should be invalid");
    assert_eq!(is_valid2, true, "Channel 2 (bit 1) should be valid");
}

#[test]
fn test_flag_priority_over_bits() {
    let channel = create_test_channel(0x01, 0);
    let record = vec![0xFF, 0x12, 0x34, 0x00, 0x00, 0x00];
    let is_valid = check_value_validity(&record, 1, 4, &channel);
    assert_eq!(
        is_valid, false,
        "Flag should take priority: all values invalid"
    );
}
