#[cfg(test)]
mod persistent_source_mixed_regression_tests {
    use super::*;

    struct VersionedSource {
        bytes: Vec<u8>,
        version: PersistentSourceVersion,
    }

    impl ImmutableReadAt for VersionedSource {
        fn len(&mut self) -> Result<u64, ImmutableSourceError> {
            u64::try_from(self.bytes.len()).map_err(|_| ImmutableSourceError::Limit("length"))
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            buffer: &mut [u8],
        ) -> Result<(), ImmutableSourceError> {
            let start = usize::try_from(offset).map_err(|_| ImmutableSourceError::Io("offset"))?;
            let end = start
                .checked_add(buffer.len())
                .ok_or(ImmutableSourceError::Io("range"))?;
            buffer.copy_from_slice(
                self.bytes
                    .get(start..end)
                    .ok_or(ImmutableSourceError::Io("range"))?,
            );
            Ok(())
        }
    }

    impl PersistentVersionedReadAt for VersionedSource {
        fn version_token(&mut self) -> Result<PersistentSourceVersion, ImmutableSourceError> {
            Ok(self.version)
        }
    }

    fn object(object_id: u64, seed: u8, payload_len: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::from(seed % 31 + 1),
            vec![seed; payload_len],
        )
    }

    #[test]
    fn reused_pages_do_not_inflate_source_footer_current_page_count() {
        let format = ImmutableLimits {
            max_file_bytes: 64 * 1024 * 1024,
            max_output_bytes: 64 * 1024 * 1024,
            max_allocation_bytes: 64 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let objects: Vec<_> = (0..400)
            .map(|index| {
                object(
                    u64::try_from((index + 1) * 2).expect("object id"),
                    10,
                    11,
                )
            })
            .collect();
        let base = build_genesis(&objects, format).expect("canonical base");
        let operations = [
            ImmutableBatchOperation::Delete(2),
            ImmutableBatchOperation::Put(object(3, 231, 21)),
            ImmutableBatchOperation::Put(object(800, 232, 11)),
        ];
        let owned =
            append_persistent_mixed_batch(&base, &operations, format).expect("owned mixed");
        assert!(owned.pages_reused > 0);

        let mut source = VersionedSource {
            bytes: base.clone(),
            version: PersistentSourceVersion([111; 32]),
        };
        let plan = plan_persistent_mixed_tail_at(
            &mut source,
            &operations,
            ImmutableSourceLimits {
                format,
                max_total_bytes_read: u64::try_from(base.len() * 16).expect("read budget"),
                max_read_operations: 2_000_000,
                max_read_request_bytes: 128,
                hash_block_bytes: 132,
            },
        )
        .expect("source mixed plan");

        assert_eq!(plan.tail, owned.bytes[base.len()..]);
        assert_eq!(plan.report, owned.report);
        assert_eq!(plan.pages_written, owned.pages_written);
        assert_eq!(plan.pages_reused, owned.pages_reused);
        assert_eq!(
            plan.report.page_count,
            plan.pages_written
                .checked_add(plan.pages_reused)
                .expect("active pages")
        );
        let footer = parse_footer(&plan.tail, plan.tail.len() - FOOTER_LEN).expect("footer");
        assert_eq!(
            footer.page_count_current,
            u64::try_from(plan.pages_written).expect("current pages")
        );
    }
}
