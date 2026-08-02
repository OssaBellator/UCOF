impl From<ImmutableError> for PersistentSourceInsertionError {
    fn from(error: ImmutableError) -> Self {
        Self::Writer(error)
    }
}
