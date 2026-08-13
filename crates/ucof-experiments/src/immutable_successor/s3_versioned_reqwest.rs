use sha2::Digest as ShaDigest;

const S3_VERSION_HEADER: reqwest::header::HeaderName =
    reqwest::header::HeaderName::from_static("x-amz-version-id");
const S3_DELETE_MARKER_HEADER: reqwest::header::HeaderName =
    reqwest::header::HeaderName::from_static("x-amz-delete-marker");
const S3_CONTENT_SHA256_HEADER: reqwest::header::HeaderName =
    reqwest::header::HeaderName::from_static("x-amz-content-sha256");
const S3_DATE_HEADER: reqwest::header::HeaderName =
    reqwest::header::HeaderName::from_static("x-amz-date");
const S3_SECURITY_TOKEN_HEADER: reqwest::header::HeaderName =
    reqwest::header::HeaderName::from_static("x-amz-security-token");
const S3_EMPTY_SHA256_HEX: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const S3_TOKEN_PREFIX: &str = "s3v1:";

/// Whether a versioned S3 adapter may use plaintext HTTP.
///
/// Production callers should use `HttpsOnly`. `AllowHttpEmulator` exists only so local S3-compatible
/// emulators can qualify exact request/version semantics without requiring a test PKI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum S3EndpointPolicy {
    HttpsOnly,
    AllowHttpEmulator,
}

/// Static SigV4 credentials for one qualification client.
///
/// Debug output is deliberately redacted. Temporary credentials may carry an opaque security token.
pub struct S3SigV4Credentials {
    access_key_id: String,
    secret_access_key: Vec<u8>,
    session_token: Option<HeaderValue>,
}

impl std::fmt::Debug for S3SigV4Credentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("S3SigV4Credentials")
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl Clone for S3SigV4Credentials {
    fn clone(&self) -> Self {
        Self {
            access_key_id: self.access_key_id.clone(),
            secret_access_key: self.secret_access_key.clone(),
            session_token: self.session_token.clone(),
        }
    }
}

impl S3SigV4Credentials {
    pub fn new(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<Vec<u8>>,
        session_token: Option<&str>,
    ) -> Result<Self, ConditionalSourceError> {
        let access_key_id = access_key_id.into();
        let secret_access_key = secret_access_key.into();
        if access_key_id.is_empty()
            || access_key_id.contains(['\r', '\n'])
            || secret_access_key.is_empty()
        {
            return Err(ConditionalSourceError::Client("s3 credentials"));
        }
        let session_token = session_token
            .map(|value| {
                let mut header = HeaderValue::from_str(value)
                    .map_err(|_| ConditionalSourceError::Client("s3 session token"))?;
                header.set_sensitive(true);
                Ok(header)
            })
            .transpose()?;
        Ok(Self {
            access_key_id,
            secret_access_key,
            session_token,
        })
    }
}

#[derive(Clone, Debug)]
struct S3SigningTimestamp {
    date: String,
    datetime: String,
}

impl S3SigningTimestamp {
    fn now() -> Result<Self, ConditionalSourceError> {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| ConditionalSourceError::Client("system clock before unix epoch"))?
            .as_secs();
        Self::from_unix_seconds(seconds)
    }

    fn from_unix_seconds(seconds: u64) -> Result<Self, ConditionalSourceError> {
        let days = i64::try_from(seconds / 86_400)
            .map_err(|_| ConditionalSourceError::Limit("s3 signing time"))?;
        let seconds_of_day = seconds % 86_400;
        let (year, month, day) = civil_from_unix_days(days)?;
        let hour = seconds_of_day / 3_600;
        let minute = (seconds_of_day % 3_600) / 60;
        let second = seconds_of_day % 60;
        Ok(Self {
            date: format!("{year:04}{month:02}{day:02}"),
            datetime: format!(
                "{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z"
            ),
        })
    }

    #[cfg(test)]
    fn parse_fixed(datetime: &str) -> Result<Self, ConditionalSourceError> {
        if datetime.len() != 16
            || !datetime.ends_with('Z')
            || datetime.as_bytes().get(8) != Some(&b'T')
            || !datetime
                .bytes()
                .enumerate()
                .all(|(index, byte)| index == 8 || index == 15 || byte.is_ascii_digit())
        {
            return Err(ConditionalSourceError::Client("s3 signing timestamp"));
        }
        Ok(Self {
            date: datetime[..8].to_owned(),
            datetime: datetime.to_owned(),
        })
    }
}

fn civil_from_unix_days(days: i64) -> Result<(i64, u64, u64), ConditionalSourceError> {
    let z = days
        .checked_add(719_468)
        .ok_or(ConditionalSourceError::Limit("s3 signing date"))?;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(ConditionalSourceError::Limit("s3 signing date"));
    }
    Ok((
        year,
        u64::try_from(month).map_err(|_| ConditionalSourceError::Limit("s3 signing date"))?,
        u64::try_from(day).map_err(|_| ConditionalSourceError::Limit("s3 signing date"))?,
    ))
}

fn s3_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn s3_hex_decode(value: &str) -> Result<Vec<u8>, ConditionalSourceError> {
    if value.len() % 2 != 0 {
        return Err(ConditionalSourceError::InvalidVersionToken);
    }
    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = nibble(pair[0]).ok_or(ConditionalSourceError::InvalidVersionToken)?;
        let low = nibble(pair[1]).ok_or(ConditionalSourceError::InvalidVersionToken)?;
        output.push((high << 4) | low);
    }
    Ok(output)
}

fn s3_version_token_from_header(
    value: &HeaderValue,
) -> Result<StrongVersionToken, ConditionalSourceError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes == b"null" || bytes.len() > 1_024 {
        return Err(ConditionalSourceError::InvalidVersionToken);
    }
    StrongVersionToken::parse(format!("\"{S3_TOKEN_PREFIX}{}\"", s3_hex(bytes)))
}

fn s3_version_bytes_from_token(
    token: &StrongVersionToken,
) -> Result<Vec<u8>, ConditionalSourceError> {
    let raw = token.as_str();
    let inner = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .and_then(|value| value.strip_prefix(S3_TOKEN_PREFIX))
        .ok_or(ConditionalSourceError::InvalidVersionToken)?;
    let bytes = s3_hex_decode(inner)?;
    if bytes.is_empty() || bytes == b"null" || bytes.len() > 1_024 {
        return Err(ConditionalSourceError::InvalidVersionToken);
    }
    Ok(bytes)
}

fn aws_uri_encode_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(*byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

fn s3_canonical_host(url: &reqwest::Url) -> Result<String, ConditionalSourceError> {
    let host = url
        .host_str()
        .ok_or(ConditionalSourceError::Client("s3 endpoint host"))?;
    let include_port = match (url.scheme(), url.port()) {
        ("http", Some(80)) | ("https", Some(443)) | (_, None) => false,
        (_, Some(_)) => true,
    };
    Ok(if include_port {
        format!(
            "{host}:{}",
            url.port()
                .ok_or(ConditionalSourceError::Client("s3 endpoint port"))?
        )
    } else {
        host.to_owned()
    })
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut normalized = [0_u8; BLOCK];
    if key.len() > BLOCK {
        let digest = sha2::Sha256::digest(key);
        normalized[..digest.len()].copy_from_slice(&digest);
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner_key = [0_u8; BLOCK];
    let mut outer_key = [0_u8; BLOCK];
    for index in 0..BLOCK {
        inner_key[index] = normalized[index] ^ 0x36;
        outer_key[index] = normalized[index] ^ 0x5c;
    }
    let mut inner = sha2::Sha256::new();
    inner.update(inner_key);
    inner.update(data);
    let inner_digest = inner.finalize();
    let mut outer = sha2::Sha256::new();
    outer.update(outer_key);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn s3_signing_key(secret: &[u8], date: &str, region: &str) -> [u8; 32] {
    let mut initial = Vec::with_capacity(4 + secret.len());
    initial.extend_from_slice(b"AWS4");
    initial.extend_from_slice(secret);
    let date_key = hmac_sha256(&initial, date.as_bytes());
    let region_key = hmac_sha256(&date_key, region.as_bytes());
    let service_key = hmac_sha256(&region_key, b"s3");
    hmac_sha256(&service_key, b"aws4_request")
}

fn s3_sigv4_headers_at(
    credentials: &S3SigV4Credentials,
    region: &str,
    method: &str,
    url: &reqwest::Url,
    range: Option<&str>,
    timestamp: &S3SigningTimestamp,
) -> Result<HeaderMap, ConditionalSourceError> {
    if region.is_empty() || !region.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
        return Err(ConditionalSourceError::Client("s3 region"));
    }
    let host = s3_canonical_host(url)?;
    let canonical_uri = url.path();
    if canonical_uri.is_empty() || !canonical_uri.starts_with('/') {
        return Err(ConditionalSourceError::Client("s3 object path"));
    }
    let canonical_query = url.query().unwrap_or("");

    let mut canonical_headers = String::new();
    let mut signed_headers = Vec::new();
    canonical_headers.push_str("host:");
    canonical_headers.push_str(&host);
    canonical_headers.push('\n');
    signed_headers.push("host");
    if let Some(range) = range {
        canonical_headers.push_str("range:");
        canonical_headers.push_str(range.trim());
        canonical_headers.push('\n');
        signed_headers.push("range");
    }
    canonical_headers.push_str("x-amz-content-sha256:");
    canonical_headers.push_str(S3_EMPTY_SHA256_HEX);
    canonical_headers.push('\n');
    signed_headers.push("x-amz-content-sha256");
    canonical_headers.push_str("x-amz-date:");
    canonical_headers.push_str(&timestamp.datetime);
    canonical_headers.push('\n');
    signed_headers.push("x-amz-date");
    if let Some(token) = &credentials.session_token {
        let token = token
            .to_str()
            .map_err(|_| ConditionalSourceError::Client("s3 session token"))?;
        canonical_headers.push_str("x-amz-security-token:");
        canonical_headers.push_str(token.trim());
        canonical_headers.push('\n');
        signed_headers.push("x-amz-security-token");
    }
    let signed_headers = signed_headers.join(";");
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{S3_EMPTY_SHA256_HEX}"
    );
    let canonical_digest = sha2::Sha256::digest(canonical_request.as_bytes());
    let scope = format!("{}/{region}/s3/aws4_request", timestamp.date);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
        timestamp.datetime,
        s3_hex(&canonical_digest)
    );
    let signing_key = s3_signing_key(&credentials.secret_access_key, &timestamp.date, region);
    let signature = hmac_sha256(&signing_key, string_to_sign.as_bytes());
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope},SignedHeaders={signed_headers},Signature={}",
        credentials.access_key_id,
        s3_hex(&signature)
    );

    let mut output = HeaderMap::new();
    let mut authorization = HeaderValue::from_str(&authorization)
        .map_err(|_| ConditionalSourceError::Client("s3 authorization header"))?;
    authorization.set_sensitive(true);
    output.insert(reqwest::header::AUTHORIZATION, authorization);
    output.insert(
        S3_CONTENT_SHA256_HEADER,
        HeaderValue::from_static(S3_EMPTY_SHA256_HEX),
    );
    output.insert(
        S3_DATE_HEADER,
        HeaderValue::from_str(&timestamp.datetime)
            .map_err(|_| ConditionalSourceError::Client("s3 date header"))?,
    );
    if let Some(token) = &credentials.session_token {
        output.insert(S3_SECURITY_TOKEN_HEADER, token.clone());
    }
    Ok(output)
}

fn s3_sigv4_headers(
    credentials: &S3SigV4Credentials,
    region: &str,
    method: &str,
    url: &reqwest::Url,
    range: Option<&str>,
) -> Result<HeaderMap, ConditionalSourceError> {
    let timestamp = S3SigningTimestamp::now()?;
    s3_sigv4_headers_at(credentials, region, method, url, range, &timestamp)
}

#[derive(Clone, Debug)]
pub struct S3VersionedReqwestClient {
    client: reqwest::Client,
    object_url: reqwest::Url,
    control: ImmutableOperationControl,
    credentials: S3SigV4Credentials,
    region: String,
    retry_policy: ConditionalRetryPolicy,
    backoff: ConditionalBackoffBudget,
    transport_attempts: u64,
}

impl S3VersionedReqwestClient {
    pub fn new(
        object_url: &str,
        region: impl Into<String>,
        credentials: S3SigV4Credentials,
        control: ImmutableOperationControl,
        retry_policy: ConditionalRetryPolicy,
        backoff_policy: ConditionalBackoffPolicy,
        endpoint_policy: S3EndpointPolicy,
    ) -> Result<Self, ConditionalSourceError> {
        let object_url = reqwest::Url::parse(object_url)
            .map_err(|_| ConditionalSourceError::Client("invalid s3 object URL"))?;
        if object_url.host_str().is_none()
            || object_url.fragment().is_some()
            || object_url.query().is_some()
            || object_url.path().is_empty()
        {
            return Err(ConditionalSourceError::Client("s3 object URL"));
        }
        match (endpoint_policy, object_url.scheme()) {
            (S3EndpointPolicy::HttpsOnly, "https")
            | (S3EndpointPolicy::AllowHttpEmulator, "https" | "http") => {}
            _ => return Err(ConditionalSourceError::Client("s3 endpoint scheme")),
        }
        let region = region.into();
        if region.is_empty() {
            return Err(ConditionalSourceError::Client("s3 region"));
        }
        let host = object_url
            .host_str()
            .ok_or(ConditionalSourceError::Client("s3 endpoint host"))?
            .to_owned();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .no_gzip()
            .no_brotli()
            .no_zstd()
            .no_deflate()
            .referer(false)
            .retry(reqwest::retry::for_host(host).max_retries_per_request(0))
            .build()
            .map_err(|_| ConditionalSourceError::Client("s3 reqwest client build"))?;
        Ok(Self {
            client,
            object_url,
            control,
            credentials,
            region,
            retry_policy,
            backoff: ConditionalBackoffBudget::new(backoff_policy),
            transport_attempts: 0,
        })
    }

    #[must_use]
    pub fn transport_attempts(&self) -> u64 {
        self.transport_attempts
    }

    fn begin_attempt(&mut self) -> Result<(), ConditionalSourceError> {
        self.control.check()?;
        if self.transport_attempts >= self.retry_policy.max_transport_attempts() {
            return Err(ConditionalSourceError::Limit("transport attempts"));
        }
        self.transport_attempts = self
            .transport_attempts
            .checked_add(1)
            .ok_or(ConditionalSourceError::Limit("transport attempts"))?;
        Ok(())
    }

    async fn retry_after_status(
        &mut self,
        error: ConditionalSourceError,
        server_minimum_millis: Option<u64>,
    ) -> Result<(), ConditionalSourceError> {
        if self.transport_attempts >= self.retry_policy.max_transport_attempts() {
            return Err(ConditionalSourceError::Limit("transport attempts"));
        }
        let decision = ConditionalHttpDecision::Retry {
            error,
            server_minimum_millis,
        };
        let remaining = remaining_deadline_millis(&self.control)?;
        let wait = plan_conditional_http_retry(&decision, &mut self.backoff, remaining)?;
        await_controlled_retry_delay(&self.control, wait.delay_millis).await
    }

    async fn retry_after_transport_error(
        &mut self,
        error: ConditionalSourceError,
    ) -> Result<(), ConditionalSourceError> {
        match error {
            ConditionalSourceError::RetryableClient(_) => {
                self.retry_after_status(error, None).await
            }
            other => Err(other),
        }
    }

    fn classify_non_success(
        status: u16,
        headers: &HeaderMap,
    ) -> Result<ReqwestConditionalAttempt<()>, ConditionalSourceError> {
        if matches!(status, 429 | 500 | 502 | 503 | 504) {
            return Ok(ReqwestConditionalAttempt::Retry {
                error: ConditionalSourceError::RetryableClient("s3 http status"),
                server_minimum_millis: optional_retry_after_millis(headers)?,
            });
        }
        match status {
            401 | 403 => Err(ConditionalSourceError::Client("s3 authentication")),
            404 | 405 => Err(ConditionalSourceError::Client("s3 object version unavailable")),
            412 => Err(ConditionalSourceError::VersionChanged),
            _ => Err(ConditionalSourceError::Protocol("s3 http status")),
        }
    }

    async fn metadata_attempt(
        &self,
    ) -> Result<ReqwestConditionalAttempt<ConditionalObjectMetadata>, ConditionalSourceError> {
        self.control.check()?;
        let signed = s3_sigv4_headers(
            &self.credentials,
            &self.region,
            "HEAD",
            &self.object_url,
            None,
        )?;
        let request = self
            .client
            .head(self.object_url.clone())
            .header(ACCEPT_ENCODING, HeaderValue::from_static("identity"))
            .headers(signed);
        let response = await_reqwest_controlled(&self.control, request.send()).await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        reject_non_identity_encoding(&headers)?;
        if status != 200 {
            return match Self::classify_non_success(status, &headers)? {
                ReqwestConditionalAttempt::Retry {
                    error,
                    server_minimum_millis,
                } => Ok(ReqwestConditionalAttempt::Retry {
                    error,
                    server_minimum_millis,
                }),
                _ => Err(ConditionalSourceError::Protocol("s3 metadata status")),
            };
        }
        if optional_header_string(&headers, S3_DELETE_MARKER_HEADER)?
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        {
            return Err(ConditionalSourceError::Client("s3 current version is delete marker"));
        }
        let length = optional_header_u64(&headers, CONTENT_LENGTH)?
            .ok_or(ConditionalSourceError::Protocol("s3 content length"))?;
        let version_header = headers
            .get(S3_VERSION_HEADER)
            .ok_or(ConditionalSourceError::InvalidVersionToken)?;
        let version = s3_version_token_from_header(version_header)?;
        Ok(ReqwestConditionalAttempt::Accepted(
            ConditionalObjectMetadata {
                length,
                version: version.as_str().to_owned(),
            },
        ))
    }

    async fn range_attempt(
        &self,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    ) -> Result<ReqwestConditionalAttempt<ConditionalRangeResponse>, ConditionalSourceError> {
        self.control.check()?;
        if length == 0 {
            return Err(ConditionalSourceError::Protocol("zero range"));
        }
        let length_u64 =
            u64::try_from(length).map_err(|_| ConditionalSourceError::Limit("range length"))?;
        let end_exclusive = offset
            .checked_add(length_u64)
            .ok_or(ConditionalSourceError::Protocol("range overflow"))?;
        if end_exclusive > total_length {
            return Err(ConditionalSourceError::Protocol("range outside object"));
        }
        let end_inclusive = end_exclusive
            .checked_sub(1)
            .ok_or(ConditionalSourceError::Protocol("zero range"))?;
        let version_bytes = s3_version_bytes_from_token(expected)?;
        let query = format!("versionId={}", aws_uri_encode_bytes(&version_bytes));
        let mut url = self.object_url.clone();
        url.set_query(Some(&query));
        let range_text = format!("bytes={offset}-{end_inclusive}");
        let range_header = HeaderValue::from_str(&range_text)
            .map_err(|_| ConditionalSourceError::Protocol("range header"))?;
        let signed = s3_sigv4_headers(
            &self.credentials,
            &self.region,
            "GET",
            &url,
            Some(&range_text),
        )?;
        let request = self
            .client
            .get(url)
            .header(ACCEPT_ENCODING, HeaderValue::from_static("identity"))
            .header(RANGE, range_header)
            .headers(signed);
        let response = await_reqwest_controlled(&self.control, request.send()).await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        reject_non_identity_encoding(&headers)?;
        if status != 206 {
            return match Self::classify_non_success(status, &headers)? {
                ReqwestConditionalAttempt::Retry {
                    error,
                    server_minimum_millis,
                } => Ok(ReqwestConditionalAttempt::Retry {
                    error,
                    server_minimum_millis,
                }),
                _ => Err(ConditionalSourceError::Protocol("s3 range status")),
            };
        }
        let response_token = s3_version_token_from_header(
            headers
                .get(S3_VERSION_HEADER)
                .ok_or(ConditionalSourceError::InvalidVersionToken)?,
        )?;
        if &response_token != expected {
            return Err(ConditionalSourceError::VersionChanged);
        }
        let content_length = optional_header_u64(&headers, CONTENT_LENGTH)?
            .ok_or(ConditionalSourceError::Protocol("s3 range content length"))?;
        if content_length != length_u64 {
            return Err(ConditionalSourceError::Protocol("s3 range content length"));
        }
        let content_range = optional_content_range(&headers)?
            .ok_or(ConditionalSourceError::Protocol("s3 content range"))?;
        if content_range.start != offset
            || content_range.end_inclusive != end_inclusive
            || content_range.total_length != total_length
        {
            return Err(ConditionalSourceError::Protocol("s3 content range"));
        }
        let body = await_reqwest_controlled(&self.control, response.bytes()).await?;
        if body.len() != length {
            return Err(ConditionalSourceError::Protocol("s3 range body length"));
        }
        Ok(ReqwestConditionalAttempt::Accepted(
            ConditionalRangeResponse {
                version: response_token.as_str().to_owned(),
                offset,
                total_length,
                body: body.to_vec(),
            },
        ))
    }

    pub async fn metadata(&mut self) -> Result<ConditionalObjectMetadata, ConditionalSourceError> {
        loop {
            self.begin_attempt()?;
            match self.metadata_attempt().await {
                Ok(ReqwestConditionalAttempt::Accepted(metadata)) => return Ok(metadata),
                Ok(ReqwestConditionalAttempt::Retry {
                    error,
                    server_minimum_millis,
                }) => self.retry_after_status(error, server_minimum_millis).await?,
                Ok(ReqwestConditionalAttempt::RefreshAuthentication) => {
                    return Err(ConditionalSourceError::Client("s3 authentication refresh"));
                }
                Err(error) => self.retry_after_transport_error(error).await?,
            }
        }
    }

    pub async fn read_range_versioned(
        &mut self,
        expected: &StrongVersionToken,
        offset: u64,
        length: usize,
        total_length: u64,
    ) -> Result<ConditionalRangeResponse, ConditionalSourceError> {
        loop {
            self.begin_attempt()?;
            match self
                .range_attempt(expected, offset, length, total_length)
                .await
            {
                Ok(ReqwestConditionalAttempt::Accepted(response)) => return Ok(response),
                Ok(ReqwestConditionalAttempt::Retry {
                    error,
                    server_minimum_millis,
                }) => self.retry_after_status(error, server_minimum_millis).await?,
                Ok(ReqwestConditionalAttempt::RefreshAuthentication) => {
                    return Err(ConditionalSourceError::Client("s3 authentication refresh"));
                }
                Err(error) => self.retry_after_transport_error(error).await?,
            }
        }
    }
}

impl AsyncStrongVersionReadAt for S3VersionedReqwestClient {
    fn metadata_async<'a>(
        &'a mut self,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ConditionalObjectMetadata, ConditionalSourceError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { self.metadata().await })
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
        Box::pin(async move {
            self.read_range_versioned(expected, offset, length, total_length)
                .await
        })
    }
}

#[cfg(test)]
mod s3_versioned_reqwest_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    fn credentials() -> S3SigV4Credentials {
        S3SigV4Credentials::new(
            "AKIAIOSFODNN7EXAMPLE",
            b"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_vec(),
            None,
        )
        .expect("credentials")
    }

    #[test]
    fn sigv4_matches_aws_range_get_example() {
        let url = reqwest::Url::parse("https://examplebucket.s3.amazonaws.com/test.txt")
            .expect("url");
        let timestamp = S3SigningTimestamp::parse_fixed("20130524T000000Z").expect("timestamp");
        let headers = s3_sigv4_headers_at(
            &credentials(),
            "us-east-1",
            "GET",
            &url,
            Some("bytes=0-9"),
            &timestamp,
        )
        .expect("headers");
        let authorization = headers
            .get(reqwest::header::AUTHORIZATION)
            .expect("authorization")
            .to_str()
            .expect("text");
        assert_eq!(
            authorization,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request,SignedHeaders=host;range;x-amz-content-sha256;x-amz-date,Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn version_token_round_trips_opaque_header_bytes() {
        let header = HeaderValue::from_bytes(b"3/L4kqt+opaque==").expect("header");
        let token = s3_version_token_from_header(&header).expect("token");
        assert_eq!(
            s3_version_bytes_from_token(&token).expect("bytes"),
            b"3/L4kqt+opaque=="
        );
        assert!(s3_version_token_from_header(&HeaderValue::from_static("null")).is_err());
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

    async fn serve_versioned_object(
        bytes: Vec<u8>,
    ) -> (
        String,
        Arc<Mutex<Vec<String>>>,
        oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
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
                observed.lock().expect("requests").push(request.clone());
                let first = request.lines().next().unwrap_or_default();
                assert!(header_value(&request, "authorization")
                    .is_some_and(|value| value.starts_with("AWS4-HMAC-SHA256 ")));
                assert_eq!(header_value(&request, "accept-encoding"), Some("identity"));
                if first.starts_with("HEAD /object ") {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nx-amz-version-id: version+1/opaque\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        shared.len()
                    );
                    socket.write_all(response.as_bytes()).await.expect("head");
                } else if first.starts_with("GET /object?versionId=version%2B1%2Fopaque ") {
                    let range = header_value(&request, "range").expect("range");
                    let range = range.strip_prefix("bytes=").expect("range prefix");
                    let (start, end) = range.split_once('-').expect("range bounds");
                    let start: usize = start.parse().expect("start");
                    let end: usize = end.parse().expect("end");
                    let body = &shared[start..=end];
                    let response = format!(
                        "HTTP/1.1 206 Partial Content\r\nx-amz-version-id: version+1/opaque\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
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
            requests,
            shutdown_tx,
            server,
        )
    }

    fn s3_client(url: &str) -> S3VersionedReqwestClient {
        S3VersionedReqwestClient::new(
            url,
            "us-east-1",
            credentials(),
            ImmutableOperationControl::unlimited(),
            ConditionalRetryPolicy::new(4).expect("retry"),
            ConditionalBackoffPolicy::new(1, 10, 100).expect("backoff"),
            S3EndpointPolicy::AllowHttpEmulator,
        )
        .expect("s3 client")
    }

    #[tokio::test]
    async fn s3_version_id_binds_all_assurance_operations() {
        let genesis = build_genesis(
            &[
                ImmutableObjectInput::new(1, 1, b"alpha".to_vec()),
                ImmutableObjectInput::new(2, 1, b"bravo".to_vec()),
            ],
            ImmutableLimits::default(),
        )
        .expect("genesis");
        let appended = append_persistent_batch(
            &genesis,
            &[ImmutableBatchOperation::Put(ImmutableObjectInput::new(
                2,
                1,
                b"bravo-two".to_vec(),
            ))],
            ImmutableLimits::default(),
        )
        .expect("append")
        .bytes;
        let (url, requests, shutdown, server) = serve_versioned_object(appended.clone()).await;

        let mut lookup_client = s3_client(&url);
        let lookup = lookup_at_async(&mut lookup_client, 2, ImmutableSourceLimits::default())
            .await
            .expect("lookup");
        assert!(lookup.lookup.lookup.found.is_some());

        let mut full_client = s3_client(&url);
        let full = validate_source_at_async(&mut full_client, ImmutableSourceLimits::default())
            .await
            .expect("full");
        assert_eq!(full.strict.report.sequence, 1);

        let mut history_client = s3_client(&url);
        let history = validate_source_history_async(
            &mut history_client,
            ImmutableSourceLimits::default(),
        )
        .await
        .expect("history");
        assert_eq!(history.history.history.entries.len(), 2);

        let mut recovery_bytes = appended;
        recovery_bytes.extend_from_slice(b"interrupted");
        // The server holds the original bytes, so recovery is exercised separately below with a
        // second immutable provider object.
        let _ = shutdown.send(());
        server.await.expect("server");

        let captured = requests.lock().expect("requests");
        assert!(captured.iter().any(|request| request.starts_with("HEAD /object ")));
        assert!(captured.iter().any(|request| request
            .starts_with("GET /object?versionId=version%2B1%2Fopaque ")));
        assert!(captured.iter().filter(|request| request.starts_with("GET ")).all(|request| {
            header_value(request, "authorization")
                .is_some_and(|value| value.contains("SignedHeaders=host;range;x-amz-content-sha256;x-amz-date"))
        }));
        drop(captured);

        let (recovery_url, _, recovery_shutdown, recovery_server) =
            serve_versioned_object(recovery_bytes).await;
        let mut recovery_client = s3_client(&recovery_url);
        let recovery = scan_source_recovery_async(
            &mut recovery_client,
            ImmutableSourceLimits::default(),
        )
        .await
        .expect("recovery");
        assert!(!recovery.recovery.recovery.candidates.is_empty());
        let _ = recovery_shutdown.send(());
        recovery_server.await.expect("recovery server");
    }
}
