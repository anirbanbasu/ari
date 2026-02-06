// Test CDAP message serialization/deserialization
use ari::cdap::{CdapMessage, CdapOpCode};
use ari::rib::RibValue;

#[test]
fn test_cdap_message_bincode_roundtrip() {
    let msg = CdapMessage::new_request(
        CdapOpCode::Create,
        "test_object".to_string(),
        Some("test_class".to_string()),
        Some(RibValue::String("test_value".to_string())),
        42,
    );

    // Serialize with bincode
    let serialized = bincode::serialize(&msg).expect("Serialization should succeed");

    // Deserialize with bincode
    let deserialized: CdapMessage =
        bincode::deserialize(&serialized).expect("Deserialization should succeed");

    // Verify fields
    assert_eq!(deserialized.op_code, msg.op_code);
    assert_eq!(deserialized.obj_name, msg.obj_name);
    assert_eq!(deserialized.invoke_id, msg.invoke_id);
    assert!(deserialized.sync_request.is_none());
    assert!(deserialized.sync_response.is_none());

    println!("✓ CDAP message bincode roundtrip successful");
}
