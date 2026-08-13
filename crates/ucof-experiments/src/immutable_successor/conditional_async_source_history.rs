/// Linked-history report for one native async strong-version source operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncImmutableSourceHistoryReport {
    pub source_version: StrongVersionToken,
    pub source_length: u64,
    pub history: ImmutableSourceHistoryReport,
}

/// Presents one exact historical prefix while preserving the real remote object's strong version
/// and complete length on every underlying conditional range request.
///
/// The prefix length is parser authority only. The underlying HTTP/cloud source still sees the
/// complete immutable object length, so a historical prefix never pretends that the remote object
/// itself became shorter.
struct AsyncPrefixStrongVersionSource<'a, S> {
    inner: &'a mut S,
    version: StrongVersionToken,
    source_length: u64,
    prefix_length: u64,
}

impl<S: AsyncStrongVersionReadAt> AsyncStrongVersionReadAt for AsyncPrefixStrongVersionSource<'_, S> {
    fn metadata_async<'a>(
        &'a mut self,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ConditionalObjectMetadata, ConditionalSourceError>>
                + Send
                + 'a,
        >,
    > {
        let length = self.prefix_length;
        let version = self.version.as_str().to_owned();
        Box::pin(async move { Ok(ConditionalObjectMetadata { length, version }) })
    }

    fn read_range_if_match_async<'a>(
        &'a mut self,
        expected: &'a StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ConditionalRangeResponse, ConditionalSourceError>>
                + Send
                + 'a,
        >,
    > {
        let version = self.version.clone();
        let source_length = self.source_length;
        let prefix_length = self.prefix_length;
        Box::pin(async move {
            let length_u64 = u64::try_from(length)
                .map_err(|_| ConditionalSourceError::Limit("range length"))?;
            let end = offset
                .checked_add(length_u64)
                .ok_or(ConditionalSourceError::Protocol("history prefix range"))?;
            if expected != &version || total_length != prefix_length || end > prefix_length {
                return Err(ConditionalSourceError::Protocol("history prefix range"));
            }
            let response = self
                .inner
                .read_range_if_match_async(&version, offset, length, source_length)
                .await?;
            let response_version = StrongVersionToken::parse(response.version.clone())?;
            if response_version != version
                || response.offset != offset
                || response.total_length != source_length
                || response.body.len() != length
            {
                return Err(ConditionalSourceError::Protocol("history source range"));
            }
            Ok(ConditionalRangeResponse {
                version: response.version,
                offset: response.offset,
                total_length: prefix_length,
                body: response.body,
            })
        })
    }
}

async fn async_history_footer_and_parent<S: AsyncStrongVersionReadAt>(
    source: &mut S,
    version: &StrongVersionToken,
    source_length: u64,
    prefix_length: u64,
    limits: ImmutableSourceLimits,
) -> Result<(Footer, [u8; 32], ImmutableSourceStats), AsyncImmutableSourceError> {
    let mut prefix = AsyncPrefixStrongVersionSource {
        inner: source,
        version: version.clone(),
        source_length,
        prefix_length,
    };
    let mut reader = AsyncFullSourceReader::new(&mut prefix, limits).await?;
    let footer_offset = reader
        .length
        .checked_sub(FOOTER_LEN)
        .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid("file length")))?;
    let footer_raw = reader.read_vec(footer_offset, FOOTER_LEN, "footer").await?;
    let footer = parse_footer(&footer_raw, 0)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot_len = usize_from_u64(footer.snapshot_len, "snapshot range")?;
    if snapshot_len != SNAPSHOT_LEN
        || snapshot_offset
            .checked_add(snapshot_len)
            .is_none_or(|end| end != footer_offset)
    {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("snapshot range")).into(),
        );
    }
    let snapshot = reader
        .read_vec(snapshot_offset, SNAPSHOT_LEN, "snapshot")
        .await?;
    reader.stats.bytes_hashed = reader
        .stats
        .bytes_hashed
        .checked_add(
            u64::try_from(snapshot.len())
                .map_err(|_| ImmutableSourceError::Limit("hashed bytes"))?,
        )
        .ok_or(ImmutableSourceError::Limit("hashed bytes"))?;
    if digest(&[SNAPSHOT_DOMAIN, &snapshot]) != footer.snapshot_digest {
        return Err(
            ImmutableSourceError::Format(ImmutableError::Invalid("snapshot digest")).into(),
        );
    }
    Ok((
        footer,
        array(&snapshot, 64, "snapshot parent")?,
        reader.stats,
    ))
}

/// Revalidates every linked exact prefix through one native async strong-version source view.
///
/// Metadata is acquired exactly once from the real source. Historical prefixes are parser views;
/// every underlying range remains conditioned on the original complete-object version and length.
/// Entries are newest first, strict validation never invokes recovery, and source read limits are
/// cumulative across the complete history operation.
pub async fn validate_source_history_async<S: AsyncStrongVersionReadAt>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<AsyncImmutableSourceHistoryReport, AsyncImmutableSourceError> {
    let metadata = source.metadata_async().await?;
    let source_version = StrongVersionToken::parse(metadata.version)?;
    let source_length = metadata.length;
    let mut prefix_length = source_length;
    let mut stats = ImmutableSourceStats::default();
    let mut entries = Vec::new();
    let mut expected: Option<(u64, [u8; 32])> = None;

    loop {
        if entries.len() >= limits.format.max_history_entries {
            return Err(
                ImmutableSourceError::Format(ImmutableError::Limit("history entries")).into(),
            );
        }
        allocation_check::<ImmutableHistoryEntry>(entries.len() + 1, limits.format)?;

        let call_limits = remaining_source_limits(limits, stats)?;
        let mut prefix = AsyncPrefixStrongVersionSource {
            inner: source,
            version: source_version.clone(),
            source_length,
            prefix_length,
        };
        let strict = validate_source_at_async(&mut prefix, call_limits).await?;
        add_source_stats(&mut stats, strict.strict.stats)?;
        if let Some((sequence, snapshot_digest)) = expected {
            if strict.strict.report.sequence != sequence
                || strict.strict.report.snapshot_digest != snapshot_digest
            {
                return Err(
                    ImmutableSourceError::Format(ImmutableError::Invalid("parent linkage")).into(),
                );
            }
        }

        let footer_offset = prefix_length
            .checked_sub(u64::try_from(FOOTER_LEN).expect("footer length"))
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid("file length")))?;
        let call_limits = remaining_source_limits(limits, stats)?;
        let (footer, parent_digest, footer_stats) = async_history_footer_and_parent(
            source,
            &source_version,
            source_length,
            prefix_length,
            call_limits,
        )
        .await?;
        add_source_stats(&mut stats, footer_stats)?;
        entries.push(ImmutableHistoryEntry {
            footer_offset,
            report: strict.strict.report,
        });

        if footer.previous_footer_offset == ABSENT_OFFSET {
            if footer.sequence != 0 || parent_digest.iter().any(|byte| *byte != 0) {
                return Err(
                    ImmutableSourceError::Format(ImmutableError::Invalid("genesis linkage")).into(),
                );
            }
            break;
        }
        if footer.sequence == 0 || footer.previous_footer_offset >= footer_offset {
            return Err(
                ImmutableSourceError::Format(ImmutableError::Invalid("previous footer")).into(),
            );
        }
        expected = Some((footer.sequence - 1, parent_digest));
        prefix_length = footer
            .previous_footer_offset
            .checked_add(u64::try_from(FOOTER_LEN).expect("footer length"))
            .ok_or(ImmutableSourceError::Format(ImmutableError::Invalid("previous footer")))?;
    }

    Ok(AsyncImmutableSourceHistoryReport {
        source_version,
        source_length,
        history: ImmutableSourceHistoryReport {
            history: ImmutableHistoryReport { entries },
            stats,
        },
    })
}

#[cfg(test)]
mod conditional_async_source_history_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    struct NoAuthentication;

    impl AsyncConditionalAuthenticationRefresher for NoAuthentication {
        fn current_authorization(
            &self,
        ) -> Result<Option<ReqwestAuthorizationHeader>, ConditionalSourceError> {
            Ok(None)
        }

        fn refresh_authentication<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), ConditionalSourceError>> + Send + 'a>> {
            Box::pin(async {
                Err(ConditionalSourceError::Client(
                    "unexpected authentication refresh",
                ))
            })
        }
    }

    #[derive(Default)]
    struct Counts {
        head: usize,
        ranges: usize,
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 2048];
        loop {
            let read = socket.read(&mut chunk).await.expect("read request");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(bytes).expect("request text")
    }

    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then_some(value.trim())
        })
    }

    fn parse_range(request: &str) -> Option<(usize, usize)> {
        let value = header_value(request, "range")?;
        let value = value.strip_prefix("bytes=")?;
        let (start, end) = value.split_once('-')?;
        Some((start.parse().ok()?, end.parse().ok()?))
    }

    async fn serve_object(
        bytes: Vec<u8>,
        mutate_after_ranges: Option<usize>,
    ) -> (
        String,
        Arc<Mutex<Counts>>,
        oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let counts = Arc::new(Mutex::new(Counts::default()));
        let observed = Arc::clone(&counts);
        let shared = Arc::new(bytes);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    _ = &mut shutdown_rx => break,
                    result = listener.accept() => result,
                };
                let (mut socket, _) = accepted.expect("accept");
                let request = read_request(&mut socket).await;
                let first = request.lines().next().unwrap_or_default();
                if first.starts_with("HEAD ") {
                    observed.lock().expect("counts").head += 1;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        shared.len()
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("head response");
                } else if first.starts_with("GET ") {
                    let range_number = {
                        let mut counts = observed.lock().expect("counts");
                        counts.ranges += 1;
                        counts.ranges
                    };
                    if mutate_after_ranges.is_some_and(|limit| range_number > limit) {
                        socket
                            .write_all(
                                b"HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            )
                            .await
                            .expect("412 response");
                        continue;
                    }
                    assert_eq!(header_value(&request, "if-match"), Some("\"v1\""));
                    assert_eq!(header_value(&request, "accept-encoding"), Some("identity"));
                    let (start, end) = parse_range(&request).expect("range");
                    assert!(start <= end && end < shared.len());
                    let body = &shared[start..=end];
                    let response = format!(
                        "HTTP/1.1 206 Partial Content\r\nETag: \"v1\"\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                        body.len(), start, end, shared.len()
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("range headers");
                    socket.write_all(body).await.expect("range body");
                } else {
                    panic!("unexpected request: {first}");
                }
                socket.shutdown().await.expect("shutdown");
            }
        });
        (
            format!("http://{address}/object"),
            counts,
            shutdown_tx,
            server,
        )
    }

    fn limits() -> ImmutableSourceLimits {
        ImmutableSourceLimits {
            max_read_request_bytes: 4 * 1024,
            hash_block_bytes: 4 * 1024,
            ..ImmutableSourceLimits::default()
        }
    }

    fn http_client(
        url: &str,
    ) -> AsyncAuthenticatedReqwestConditionalClient<NoAuthentication> {
        let transport = ReqwestConditionalRangeClient::new(
            url,
            ImmutableOperationControl::unlimited(),
        )
        .expect("transport");
        let retrying = AsyncRetryingReqwestConditionalClient::new(
            transport,
            ConditionalRetryPolicy::new(1024).expect("retry policy"),
            ConditionalBackoffPolicy::new(1, 10, 100).expect("backoff policy"),
        );
        AsyncAuthenticatedReqwestConditionalClient::new(retrying, NoAuthentication)
    }

    fn three_commit_history() -> Vec<u8> {
        let genesis = build_genesis(
            &[
                ImmutableObjectInput::new(1, 1, b"alpha".to_vec()),
                ImmutableObjectInput::new(2, 1, b"bravo".to_vec()),
                ImmutableObjectInput::new(3, 1, b"charlie".to_vec()),
            ],
            ImmutableLimits::default(),
        )
        .expect("genesis");
        let second = append_persistent_batch(
            &genesis,
            &[ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                2,
                1,
                b"bravo-2".to_vec(),
            ))],
            ImmutableLimits::default(),
        )
        .expect("replacement");
        append_persistent_batch(
            &second.bytes,
            &[ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                9,
                1,
                b"delta".to_vec(),
            ))],
            ImmutableLimits::default(),
        )
        .expect("insertion")
        .bytes
    }

    #[tokio::test]
    async fn real_http_linked_history_matches_synchronous_history_and_stats() {
        let bytes = three_commit_history();
        let source_limits = limits();
        let mut sync_source = ImmutableSliceSource::new(&bytes);
        let sync = validate_source_history(&mut sync_source, source_limits)
            .expect("synchronous linked history");
        assert_eq!(sync.history.entries.len(), 3);

        let (url, counts, shutdown, server) = serve_object(bytes.clone(), None).await;
        let mut client = http_client(&url);
        let async_report = validate_source_history_async(&mut client, source_limits)
            .await
            .expect("async linked history");
        assert_eq!(async_report.history, sync);
        assert_eq!(async_report.source_length, bytes.len() as u64);
        assert_eq!(async_report.source_version.as_str(), "\"v1\"");
        let observed = counts.lock().expect("counts");
        assert_eq!(observed.head, 1);
        assert_eq!(
            observed.ranges as u64,
            async_report.history.stats.read_operations
        );
        drop(observed);
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn version_change_during_linked_history_fails_before_mixing_versions() {
        let bytes = three_commit_history();
        let (url, counts, shutdown, server) = serve_object(bytes, Some(12)).await;
        let mut client = http_client(&url);
        assert_eq!(
            validate_source_history_async(&mut client, limits()).await,
            Err(AsyncImmutableSourceError::Conditional(
                ConditionalSourceError::VersionChanged
            ))
        );
        let observed = counts.lock().expect("counts");
        assert_eq!(observed.head, 1);
        assert_eq!(observed.ranges, 13);
        drop(observed);
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn linked_history_preserves_history_entry_limit_class() {
        let bytes = three_commit_history();
        let constrained = ImmutableSourceLimits {
            format: ImmutableLimits {
                max_history_entries: 1,
                ..ImmutableLimits::default()
            },
            ..limits()
        };
        let (url, _, shutdown, server) = serve_object(bytes, None).await;
        let mut client = http_client(&url);
        assert_eq!(
            validate_source_history_async(&mut client, constrained).await,
            Err(AsyncImmutableSourceError::Source(
                ImmutableSourceError::Format(ImmutableError::Limit("history entries"))
            ))
        );
        let _ = shutdown.send(());
        server.await.expect("server");
    }
}
