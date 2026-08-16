/// Recovery report for one native async strong-version source operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncImmutableSourceRecoveryReport {
    pub source_version: StrongVersionToken,
    pub source_length: u64,
    pub recovery: ImmutableSourceRecoveryReport,
}

/// Historical-prefix source view with independent attempted-read accounting.
///
/// This mirrors synchronous `PrefixSource`: parser authority ends at `prefix_length`, while every
/// underlying conditional read remains bound to the real complete object length and strong version.
/// The outer stats survive a format-validation failure, which is required for cumulative recovery
/// budgets across many false footer-magic hits.
struct AsyncTrackedPrefixStrongVersionSource<'a, S> {
    inner: &'a mut S,
    version: StrongVersionToken,
    source_length: u64,
    prefix_length: u64,
    limits: ImmutableSourceLimits,
    stats: ImmutableSourceStats,
}

impl<S: AsyncStrongVersionReadAt + Send> AsyncStrongVersionReadAt
    for AsyncTrackedPrefixStrongVersionSource<'_, S>
{
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
                .ok_or(ConditionalSourceError::Protocol("recovery prefix range"))?;
            if expected != &version
                || total_length != prefix_length
                || end > prefix_length
                || length > self.limits.max_read_request_bytes
            {
                return Err(ConditionalSourceError::Protocol("recovery prefix range"));
            }
            let next_operations = self
                .stats
                .read_operations
                .checked_add(1)
                .ok_or(ConditionalSourceError::Limit("read operations"))?;
            let next_bytes = self
                .stats
                .bytes_read
                .checked_add(length_u64)
                .ok_or(ConditionalSourceError::Limit("read bytes"))?;
            if next_operations > self.limits.max_read_operations {
                return Err(ConditionalSourceError::Limit("read operations"));
            }
            if next_bytes > self.limits.max_total_bytes_read {
                return Err(ConditionalSourceError::Limit("read bytes"));
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
                return Err(ConditionalSourceError::Protocol("recovery source range"));
            }
            self.stats.read_operations = next_operations;
            self.stats.bytes_read = next_bytes;
            Ok(ConditionalRangeResponse {
                version: response.version,
                offset: response.offset,
                total_length: prefix_length,
                body: response.body,
            })
        })
    }
}

async fn validate_async_recovery_prefix<S: AsyncStrongVersionReadAt + Send>(
    source: &mut S,
    version: &StrongVersionToken,
    source_length: u64,
    prefix_length: u64,
    limits: ImmutableSourceLimits,
    stats: &mut ImmutableSourceStats,
) -> Result<ImmutableSourceStrictReport, AsyncImmutableSourceError> {
    let call_limits = remaining_source_limits(limits, *stats)?;
    let mut prefix = AsyncTrackedPrefixStrongVersionSource {
        inner: source,
        version: version.clone(),
        source_length,
        prefix_length,
        limits: call_limits,
        stats: ImmutableSourceStats::default(),
    };
    let result = validate_source_at_async(&mut prefix, call_limits).await;
    let attempted = prefix.stats;
    match result {
        Ok(mut report) => {
            let addition = ImmutableSourceStats {
                read_operations: attempted.read_operations,
                bytes_read: attempted.bytes_read,
                bytes_hashed: report.strict.stats.bytes_hashed,
                largest_allocation: report
                    .strict
                    .stats
                    .largest_allocation
                    .max(attempted.largest_allocation),
            };
            add_source_stats(stats, addition)?;
            report.strict.stats = addition;
            Ok(report.strict)
        }
        Err(error) => {
            add_source_stats(stats, attempted)?;
            Err(error)
        }
    }
}

/// Scans a bounded suffix and reports strictly validated candidate prefixes over one native async
/// strong-version source view. The result is evidence only; no candidate is selected.
pub async fn scan_source_recovery_async<S: AsyncStrongVersionReadAt + Send>(
    source: &mut S,
    limits: ImmutableSourceLimits,
) -> Result<AsyncImmutableSourceRecoveryReport, AsyncImmutableSourceError> {
    let metadata = source.metadata_async().await?;
    let source_version = StrongVersionToken::parse(metadata.version)?;
    let source_length = metadata.length;
    let source_length_usize = usize::try_from(source_length)
        .map_err(|_| ImmutableSourceError::Limit("length"))?;
    if source_length_usize > limits.format.max_file_bytes {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit("file size")).into());
    }

    let scan_len = usize::try_from(
        source_length.min(
            u64::try_from(limits.format.max_recovery_scan_bytes)
                .map_err(|_| ImmutableSourceError::Limit("recovery scan"))?,
        ),
    )
    .map_err(|_| ImmutableSourceError::Limit("recovery scan"))?;
    if scan_len > limits.format.max_allocation_bytes {
        return Err(ImmutableSourceError::Format(ImmutableError::Limit("allocation")).into());
    }
    let scan_start = source_length
        .checked_sub(
            u64::try_from(scan_len).map_err(|_| ImmutableSourceError::Limit("recovery scan"))?,
        )
        .ok_or(ImmutableSourceError::Limit("recovery scan"))?;

    let mut stats = ImmutableSourceStats::default();
    let call_limits = remaining_source_limits(limits, stats)?;
    let mut full_view = AsyncPrefixStrongVersionSource {
        inner: source,
        version: source_version.clone(),
        source_length,
        prefix_length: source_length,
    };
    let mut reader = AsyncFullSourceReader::new(&mut full_view, call_limits).await?;
    let suffix = reader
        .read_vec(
            usize::try_from(scan_start).map_err(|_| ImmutableSourceError::Limit("offset"))?,
            scan_len,
            "recovery scan",
        )
        .await?;
    add_source_stats(&mut stats, reader.stats)?;

    let mut offsets = Vec::new();
    if suffix.len() >= FOOTER_MAGIC.len() {
        for index in 0..=suffix.len() - FOOTER_MAGIC.len() {
            if &suffix[index..index + FOOTER_MAGIC.len()] == FOOTER_MAGIC {
                offsets.push(
                    scan_start
                        .checked_add(
                            u64::try_from(index)
                                .map_err(|_| ImmutableSourceError::Limit("offset"))?,
                        )
                        .ok_or(ImmutableSourceError::Limit("offset"))?,
                );
            }
        }
    }
    offsets.reverse();

    let mut attempted_footers = 0_usize;
    let mut attempts_truncated = false;
    let mut candidates_truncated = false;
    let mut candidates = Vec::new();
    for footer_offset in offsets {
        if attempted_footers >= limits.format.max_recovery_attempts {
            attempts_truncated = true;
            break;
        }
        attempted_footers += 1;
        let prefix_length = match footer_offset
            .checked_add(u64::try_from(FOOTER_LEN).expect("footer length"))
        {
            Some(value) if value <= source_length => value,
            _ => continue,
        };
        match validate_async_recovery_prefix(
            source,
            &source_version,
            source_length,
            prefix_length,
            limits,
            &mut stats,
        )
        .await
        {
            Ok(strict) => {
                if candidates.len() >= limits.format.max_recovery_candidates {
                    candidates_truncated = true;
                    break;
                }
                allocation_check::<ImmutableRecoveryCandidate>(
                    candidates.len() + 1,
                    limits.format,
                )?;
                candidates.push(ImmutableRecoveryCandidate {
                    footer_offset,
                    prefix_len: prefix_length,
                    report: strict.report,
                });
            }
            Err(AsyncImmutableSourceError::Source(ImmutableSourceError::Format(_))) => {}
            Err(error) => return Err(error),
        }
    }

    Ok(AsyncImmutableSourceRecoveryReport {
        source_version,
        source_length,
        recovery: ImmutableSourceRecoveryReport {
            recovery: ImmutableRecoveryReport {
                scan_start,
                scanned_bytes: scan_len,
                attempted_footers,
                attempts_truncated,
                candidates_truncated,
                candidates,
            },
            stats,
        },
    })
}

#[cfg(test)]
mod conditional_async_source_recovery_tests {
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
            max_read_request_bytes: 128 * 1024,
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

    fn interrupted_history() -> Vec<u8> {
        let genesis = build_genesis(
            &[
                ImmutableObjectInput::new(1, 1, b"alpha".to_vec()),
                ImmutableObjectInput::new(2, 1, b"bravo".to_vec()),
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
        let mut bytes = second.bytes;
        bytes.extend_from_slice(b"interrupted-tail-without-footer");
        bytes
    }

    #[tokio::test]
    async fn real_http_recovery_matches_synchronous_candidates_and_stats() {
        let bytes = interrupted_history();
        let source_limits = limits();
        let mut sync_source = ImmutableSliceSource::new(&bytes);
        let sync = scan_source_recovery(&mut sync_source, source_limits)
            .expect("synchronous recovery scan");
        assert!(!sync.recovery.candidates.is_empty());

        let (url, counts, shutdown, server) = serve_object(bytes.clone(), None).await;
        let mut client = http_client(&url);
        let async_report = scan_source_recovery_async(&mut client, source_limits)
            .await
            .expect("async recovery scan");
        assert_eq!(async_report.recovery, sync);
        assert_eq!(async_report.source_length, bytes.len() as u64);
        assert_eq!(async_report.source_version.as_str(), "\"v1\"");
        {
            let observed = counts.lock().expect("counts");
        assert_eq!(observed.head, 1);
        assert_eq!(
            observed.ranges as u64,
            async_report.recovery.stats.read_operations
        );
        }
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn version_change_after_suffix_scan_aborts_candidate_validation() {
        let bytes = interrupted_history();
        let (url, counts, shutdown, server) = serve_object(bytes, Some(1)).await;
        let mut client = http_client(&url);
        assert_eq!(
            scan_source_recovery_async(&mut client, limits()).await,
            Err(AsyncImmutableSourceError::Conditional(
                ConditionalSourceError::VersionChanged
            ))
        );
        {
            let observed = counts.lock().expect("counts");
        assert_eq!(observed.head, 1);
        assert_eq!(observed.ranges, 2);
        }
        let _ = shutdown.send(());
        server.await.expect("server");
    }

    #[tokio::test]
    async fn recovery_preserves_candidate_limit_truncation() {
        let bytes = interrupted_history();
        let constrained = ImmutableSourceLimits {
            format: ImmutableLimits {
                max_recovery_candidates: 1,
                ..ImmutableLimits::default()
            },
            ..limits()
        };
        let mut sync_source = ImmutableSliceSource::new(&bytes);
        let sync = scan_source_recovery(&mut sync_source, constrained)
            .expect("synchronous constrained recovery");
        assert!(sync.recovery.candidates_truncated);

        let (url, _, shutdown, server) = serve_object(bytes, None).await;
        let mut client = http_client(&url);
        let async_report = scan_source_recovery_async(&mut client, constrained)
            .await
            .expect("async constrained recovery");
        assert_eq!(async_report.recovery, sync);
        let _ = shutdown.send(());
        server.await.expect("server");
    }
}
