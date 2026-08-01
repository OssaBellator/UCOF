impl From<ImmutableError> for ImmutableSourceInventoryError {
    fn from(error: ImmutableError) -> Self {
        Self::Source(ImmutableSourceError::Format(error))
    }
}
