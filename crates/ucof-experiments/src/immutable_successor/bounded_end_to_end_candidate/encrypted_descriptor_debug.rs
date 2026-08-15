impl std::fmt::Debug for DescriptorEncryptionSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DescriptorEncryptionSession")
            .field("key", &"<redacted>")
            .field("nonce_prefix", &self.nonce_prefix)
            .field("operation_id", &self.operation_id)
            .field("journal_generation", &self.journal_generation)
            .field("remaining_nonces", &self.remaining())
            .finish()
    }
}

impl Drop for DescriptorEncryptionSession {
    fn drop(&mut self) {
        // Best-effort overwrite only; this is not a formal compiler-resistant
        // zeroization guarantee or a production key-lifecycle claim.
        self.key.fill(0);
    }
}

#[test]
fn descriptor_encryption_session_debug_redacts_key_material() {
    let mut authority = DescriptorNonceAuthority::initial();
    let session = authority
        .activate_session(
            [0xde; 32],
            [1, 2, 3, 4],
            [5; 16],
            4,
            8,
            true,
        )
        .expect("create descriptor session for redacted Debug test");
    let rendered = format!("{session:?}");
    assert!(rendered.contains("<redacted>"));
    assert!(!rendered.contains("222, 222, 222"));
}
