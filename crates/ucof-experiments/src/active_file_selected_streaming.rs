use std::io::Write;

use crate::immutable_successor::{
    active_file_payload_sources, write_genesis_sources_to, ImmutableActiveFilePayload,
    ImmutableError, ImmutableLimits, ImmutableReport, ImmutableSourceStreamingWriteError,
    ImmutableSourceStreamingWriteOptions, ImmutableSourceStreamingWriteReport,
    ImmutableStreamingPayloadSource,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImmutableSelectedActiveStreamingReport {
    pub source: ImmutableReport,
    pub output: ImmutableSourceStreamingWriteReport,
    pub retained_object_ids: Vec<u64>,
    pub largest_payload_read_request: usize,
}

/// Reissues caller-selected active objects from a strictly validated file through the bounded
/// canonical sink without cloning payload bytes.
///
/// Selection is canonicalized and completely matched against the authenticated active inventory
/// before the first output write. The resulting output is a new genesis file and therefore does not
/// preserve historical, offset, snapshot, commit, or signature identity.
pub fn rewrite_selected_active_file_to<W: Write>(
    writer: &mut W,
    data: &[u8],
    object_ids: &[u64],
    options: ImmutableSourceStreamingWriteOptions,
    limits: ImmutableLimits,
) -> Result<ImmutableSelectedActiveStreamingReport, ImmutableSourceStreamingWriteError> {
    if object_ids.is_empty() || object_ids.len() > limits.max_objects {
        return Err(ImmutableError::Invalid("rewrite selection").into());
    }
    let mut retained_object_ids = object_ids.to_vec();
    retained_object_ids.sort_unstable();
    if retained_object_ids
        .windows(2)
        .any(|pair| pair[0] == pair[1])
    {
        return Err(ImmutableError::Invalid("rewrite selection").into());
    }

    let (source, payloads) = active_file_payload_sources(data, limits)?;
    let mut requested = retained_object_ids.iter().copied().peekable();
    let mut selected: Vec<ImmutableActiveFilePayload<'_>> =
        Vec::with_capacity(retained_object_ids.len());
    for payload in payloads {
        let Some(next) = requested.peek().copied() else {
            break;
        };
        let object_id = payload.object_id();
        if object_id < next {
            continue;
        }
        if object_id > next {
            return Err(ImmutableError::MissingObject(next).into());
        }
        selected.push(payload);
        requested.next();
    }
    if let Some(missing) = requested.next() {
        return Err(ImmutableError::MissingObject(missing).into());
    }
    if selected.len() != retained_object_ids.len() {
        return Err(ImmutableError::Invalid("rewrite selection").into());
    }

    let output = write_genesis_sources_to(writer, &mut selected, options, limits)?;
    let largest_payload_read_request = selected
        .iter()
        .map(ImmutableActiveFilePayload::largest_read_request)
        .max()
        .unwrap_or(0);
    Ok(ImmutableSelectedActiveStreamingReport {
        source,
        output,
        retained_object_ids,
        largest_payload_read_request,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::immutable_successor::{
        append_replacement, build_genesis, rewrite_selected, ImmutableObjectInput,
        ImmutableStreamingWriteOptions,
    };

    fn object(object_id: u64, payload_len: usize) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 29).expect("kind"),
            vec![u8::try_from(object_id % 251).expect("seed"); payload_len],
        )
    }

    #[test]
    fn selected_streaming_matches_existing_selected_rewrite() {
        let limits = ImmutableLimits::default();
        let inputs: Vec<_> = (1..=400_u64).map(|id| object(id, 257)).collect();
        let genesis = build_genesis(&inputs, limits).expect("genesis");
        let source = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(200, 88, b"replacement-two-hundred".to_vec()),
            limits,
        )
        .expect("replacement");
        let selected_ids = [399, 1, 200];
        let expected = rewrite_selected(&source, &selected_ids, limits).expect("slice rewrite");

        let mut actual = Vec::new();
        let report = rewrite_selected_active_file_to(
            &mut actual,
            &source,
            &selected_ids,
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions {
                    max_write_request_bytes: 113,
                },
                max_source_read_bytes: 31,
            },
            limits,
        )
        .expect("selected streaming");
        assert_eq!(actual, expected.bytes);
        assert_eq!(report.source, expected.source);
        assert_eq!(report.output.output.report, expected.output);
        assert_eq!(report.retained_object_ids, vec![1, 200, 399]);
        assert_eq!(report.largest_payload_read_request, 31);
    }

    #[test]
    fn selection_errors_leave_sink_untouched() {
        let limits = ImmutableLimits::default();
        let source = build_genesis(&[object(1, 8), object(2, 8)], limits).expect("genesis");
        for selection in [&[][..], &[1, 1][..], &[3][..]] {
            let mut sink = Vec::new();
            assert!(rewrite_selected_active_file_to(
                &mut sink,
                &source,
                selection,
                ImmutableSourceStreamingWriteOptions::default(),
                limits,
            )
            .is_err());
            assert!(sink.is_empty());
        }
    }

    #[test]
    fn unselected_active_and_historical_payloads_are_not_read() {
        let limits = ImmutableLimits::default();
        let genesis = build_genesis(&[object(1, 4_096), object(2, 2_048), object(3, 17)], limits)
            .expect("genesis");
        let source = append_replacement(
            &genesis,
            &ImmutableObjectInput::new(1, 7, b"small-active".to_vec()),
            limits,
        )
        .expect("replacement");
        let mut sink = Vec::new();
        let report = rewrite_selected_active_file_to(
            &mut sink,
            &source,
            &[1],
            ImmutableSourceStreamingWriteOptions {
                output: ImmutableStreamingWriteOptions::default(),
                max_source_read_bytes: 64,
            },
            limits,
        )
        .expect("selected streaming");
        assert_eq!(report.output.source_bytes_read, 12);
        assert_eq!(report.output.output.report.object_count, 1);
    }
}
