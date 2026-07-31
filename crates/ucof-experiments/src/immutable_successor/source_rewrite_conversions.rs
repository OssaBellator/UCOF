impl From<ImmutableError> for ImmutableSourceCompactionError {
    fn from(error: ImmutableError) -> Self {
        Self::Compaction(ImmutableCompactionError::Format(error))
    }
}
