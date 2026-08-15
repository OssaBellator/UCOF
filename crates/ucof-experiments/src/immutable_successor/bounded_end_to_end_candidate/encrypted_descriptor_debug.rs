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
