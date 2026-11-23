//! Endpoint abstraction and query parameter machinery for the Jenkins REST
//! API: the [Endpoint] trait implemented by every API endpoint type, the
//! marker trait [ReturnsJsonResponse] used to select the correct client method
//! at compile time, the [QueryParams]/[ParamValue] machinery used to build URL
//! query parameters and the [Jenkins] (blocking) and [JenkinsAsync] client
//! objects used to actually talk to a Jenkins controller.
//!
//! # Why there is no pagination machinery here
//!
//! Unlike the Redmine and Centreon APIs this crate's siblings talk to, the
//! Jenkins JSON API has no page/limit pagination. What it has instead is the
//! `tree=` parameter, which both selects which fields are returned and, via a
//! `{from,to}` range suffix on a list-valued field, how many elements of that
//! list to include. That is a property of an individual request rather than a
//! sequence of them, so it is modelled as an ordinary endpoint parameter and
//! there is deliberately no `Pageable`/`ResponsePage` equivalent.

pub mod build;
pub mod computer;
pub mod job;
pub mod queue;
pub mod view;

use std::borrow::Cow;
use std::str::from_utf8;
use std::sync::{Arc, PoisonError, RwLock};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use reqwest::Method;
use serde::de::DeserializeOwned;
use url::Url;

use crate::api::build::{
    BuildPermalink, ConsoleText, GetBuild, GetLastBuild, GetPermalinkBuild, ProgressiveConsoleText,
    StopBuild, StopMode,
};
use crate::api::computer::ListComputers;
use crate::api::job::{GetJob, GetJobConfig, ListBuilds, ListJobs, TriggerBuild};
use crate::api::queue::{CancelQueueItem, GetQueueItem, ListQueue, WhoAmI as WhoAmIEndpoint};
use crate::api::view::{GetView, ListViews};
use crate::config::{JenkinsAuth, JenkinsInstance};
use crate::job_config::JobConfig;
use crate::types::{
    Build, BuildNumber, BuildsWrapper, Computer, ComputersWrapper, ConsoleChunk, Crumb, Job,
    JobName, JobsWrapper, QueueItem, QueueItemId, QueueItemsWrapper, Revision, RunningBuild, View,
    ViewName, ViewsWrapper, WhoAmI, running_builds,
};

/// the characters that have to be percent-encoded inside a URL path segment
///
/// Jenkins job names routinely contain spaces (`saltation - foo - Check Only
/// Rust Package - bookworm`), so encoding path segments is not optional.
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'#')
    .add(b'?')
    .add(b'{')
    .add(b'}')
    .add(b'%')
    .add(b'/');

/// percent-encode a single URL path segment, e.g. a job name
#[must_use]
pub fn encode_path_segment(segment: &str) -> String {
    utf8_percent_encode(segment, PATH_SEGMENT).to_string()
}

/// A trait representing a parameter value.
pub trait ParamValue<'a> {
    /// The parameter value as a string.
    fn as_value(&self) -> Cow<'a, str>;
}

impl ParamValue<'static> for bool {
    fn as_value(&self) -> Cow<'static, str> {
        if *self { "true".into() } else { "false".into() }
    }
}

impl<'a> ParamValue<'a> for &'a str {
    fn as_value(&self) -> Cow<'a, str> {
        (*self).into()
    }
}

impl ParamValue<'static> for String {
    fn as_value(&self) -> Cow<'static, str> {
        self.clone().into()
    }
}

impl<'a> ParamValue<'a> for &'a String {
    fn as_value(&self) -> Cow<'a, str> {
        (*self).into()
    }
}

/// serialize a [`Vec<T>`] where T implements [ToString] as a string
/// of comma-separated values
impl<T> ParamValue<'static> for Vec<T>
where
    T: ToString,
{
    fn as_value(&self) -> Cow<'static, str> {
        self.iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
            .into()
    }
}

impl<'a> ParamValue<'a> for Cow<'a, str> {
    fn as_value(&self) -> Self {
        self.clone()
    }
}

impl ParamValue<'static> for u64 {
    fn as_value(&self) -> Cow<'static, str> {
        format!("{self}").into()
    }
}

/// A structure for query parameters.
#[derive(Debug, Default, Clone)]
pub struct QueryParams<'a> {
    /// the actual parameters
    params: Vec<(Cow<'a, str>, Cow<'a, str>)>,
}

impl<'a> QueryParams<'a> {
    /// Push a single parameter.
    pub fn push<'b, K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: Into<Cow<'a, str>>,
        V: ParamValue<'b>,
        'b: 'a,
    {
        self.params.push((key.into(), value.as_value()));
        self
    }

    /// Push a single parameter if it has a value.
    pub fn push_opt<'b, K, V>(&mut self, key: K, value: Option<V>) -> &mut Self
    where
        K: Into<Cow<'a, str>>,
        V: ParamValue<'b>,
        'b: 'a,
    {
        if let Some(value) = value {
            self.params.push((key.into(), value.as_value()));
        }
        self
    }

    /// Push a set of parameters.
    pub fn extend<'b, I, K, V>(&mut self, iter: I) -> &mut Self
    where
        I: Iterator<Item = (K, V)>,
        K: Into<Cow<'a, str>>,
        V: ParamValue<'b>,
        'b: 'a,
    {
        self.params
            .extend(iter.map(|(key, value)| (key.into(), value.as_value())));
        self
    }

    /// Add the parameters to a URL.
    ///
    /// Leaves the URL untouched if there are no parameters so parameterless
    /// request URLs do not end in a trailing `?`.
    pub fn add_to_url(&self, url: &mut Url) {
        if self.params.is_empty() {
            return;
        }
        let mut pairs = url.query_pairs_mut();
        pairs.extend_pairs(self.params.iter());
    }
}

/// A trait for providing the necessary information for a single REST API endpoint.
pub trait Endpoint {
    /// The HTTP method to use for the endpoint.
    fn method(&self) -> Method;
    /// The path to the endpoint, relative to the Jenkins base URL.
    ///
    /// Implementations that embed a job name must percent-encode it with
    /// [`encode_path_segment`].
    fn endpoint(&self) -> Cow<'static, str>;

    /// Query parameters for the endpoint.
    fn parameters(&self) -> QueryParams<'_> {
        QueryParams::default()
    }

    /// The body for the endpoint.
    ///
    /// Returns the `Content-Type` header value for the data as well as the
    /// data itself.
    ///
    /// # Errors
    ///
    /// The default implementation will never return an error but
    /// implementations that serialize a body may
    fn body(&self) -> Result<Option<(&'static str, Vec<u8>)>, crate::Error> {
        Ok(None)
    }
}

/// A trait to indicate that an endpoint is expected to return a JSON result
#[diagnostic::on_unimplemented(
    message = "{Self} is an endpoint that does not return JSON, use `.text_response_body(&endpoint)` or `.ignore_response_body(&endpoint)` instead of `.json_response_body(&endpoint)`"
)]
pub trait ReturnsJsonResponse {}

/// parse and normalise the base URL from a [JenkinsInstance]
///
/// A trailing slash is appended when missing, because [`Url::join`] discards
/// the last path segment of a base URL that does not end in one.
fn base_url_from_instance_config(config: &JenkinsInstance) -> Result<Url, crate::Error> {
    let raw = if config.url.ends_with('/') {
        config.url.clone()
    } else {
        format!("{}/", config.url)
    };
    Url::parse(&raw).map_err(crate::Error::CouldNotParseUrlInConfig)
}

/// log a request body at trace level, decoding it as UTF-8 if possible
fn log_request_body(mime: &str, data: &[u8]) {
    if let Ok(request_body) = from_utf8(data) {
        tracing::trace!("Request body (Content-Type: {mime}):\n{request_body}");
    } else {
        tracing::trace!(
            "Request body (Content-Type: {mime}) could not be parsed as UTF-8:\n{data:?}"
        );
    }
}

/// log a response body at trace level, decoding it as UTF-8 if possible
fn log_response_body(response_body: &[u8]) {
    if let Ok(response_body) = from_utf8(response_body) {
        tracing::trace!("Response body:\n{response_body}");
    } else {
        tracing::trace!("Response body could not be parsed as UTF-8:\n{response_body:?}");
    }
}

/// the longest error message we carry through from a response body
///
/// Jenkins error bodies are not bounded in any useful way, and an unbounded
/// message ends up printed at the user by every consumer of this crate.
const MAX_ERROR_MESSAGE_LEN: usize = 512;

/// summarise a non-2xx response body into something worth showing a user
///
/// Jenkins does not use a consistent machine-readable error body: it answers
/// with a full HTML error page for a 404, a bare status line for some errors
/// and a plain text explanation for a rejected crumb. Carrying an HTML page
/// through verbatim buries the useful part in kilobytes of markup, so an HTML
/// body is reduced to its `<title>` (which is where Jenkins puts the actual
/// message) and anything else is trimmed and capped.
fn error_message(status: reqwest::StatusCode, response_body: &[u8]) -> String {
    /// the fallback used when the body carries nothing worth showing
    fn reason(status: reqwest::StatusCode) -> String {
        status
            .canonical_reason()
            .unwrap_or("unknown error")
            .to_owned()
    }

    if response_body.is_empty() {
        return reason(status);
    }
    let body = String::from_utf8_lossy(response_body);
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return reason(status);
    }
    let looks_like_html = trimmed.starts_with('<')
        || trimmed
            .get(..256)
            .unwrap_or(trimmed)
            .to_lowercase()
            .contains("<html");
    if looks_like_html {
        let title = extract_html_title(trimmed);
        return title.unwrap_or_else(|| reason(status));
    }
    let mut message: String = trimmed.chars().take(MAX_ERROR_MESSAGE_LEN).collect();
    if trimmed.chars().count() > MAX_ERROR_MESSAGE_LEN {
        message.push('…');
    }
    message
}

/// pull the text of the first `<title>` element out of an HTML document
fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let open = lower.find("<title")?;
    let after_open = lower
        .get(open..)?
        .find('>')?
        .checked_add(open)?
        .checked_add(1)?;
    let rest = html.get(after_open..)?;
    let close = rest.to_lowercase().find("</title>")?;
    let title = rest.get(..close)?.trim();
    if title.is_empty() {
        return None;
    }
    Some(title.chars().take(MAX_ERROR_MESSAGE_LEN).collect())
}

/// turn a non-2xx response status and body into [`crate::Error::ApiError`]
fn error_response(status: reqwest::StatusCode, response_body: &[u8]) -> crate::Error {
    crate::Error::ApiError {
        status,
        message: error_message(status, response_body),
    }
}

/// whether a failed response looks like Jenkins rejecting our CSRF crumb
///
/// Jenkins answers a request carrying a stale or missing crumb with 403 and a
/// body mentioning the crumb, which is what distinguishes it from a 403 caused
/// by the credentials themselves being wrong or insufficient.
fn is_crumb_rejection(error: &crate::Error) -> bool {
    match *error {
        crate::Error::ApiError {
            status,
            ref message,
        } => status == reqwest::StatusCode::FORBIDDEN && message.to_lowercase().contains("crumb"),
        _ => false,
    }
}

/// deserialize a 2xx JSON response body via [serde_path_to_error] and log the
/// offending fragment of the response on failure
pub(crate) fn parse_json_response_body<R>(response_body: &[u8]) -> Result<R, crate::Error>
where
    R: DeserializeOwned + std::fmt::Debug,
{
    let jd = &mut serde_json::Deserializer::from_slice(response_body);
    match serde_path_to_error::deserialize(jd) {
        Ok(parsed_response_body) => {
            tracing::trace!("Parsed response body:\n{parsed_response_body:#?}");
            Ok(parsed_response_body)
        }
        Err(e) => {
            let path = e.path();
            tracing::error!("Parsing failed at path {path}: {}", e.inner());
            if let Ok(mut fragment) = serde_json::from_slice::<serde_json::Value>(response_body) {
                for segment in path {
                    let next = match (&fragment, segment) {
                        (
                            serde_json::Value::Array(values),
                            serde_path_to_error::Segment::Seq { index },
                        ) => values.get(*index).cloned(),
                        (
                            serde_json::Value::Object(members),
                            serde_path_to_error::Segment::Map { key },
                        ) => members.get(key).cloned(),
                        _ => None,
                    };
                    if let Some(next) = next {
                        fragment = next;
                    } else {
                        // if we can not find the element serde_path_to_error
                        // references fall back to just returning the error
                        return Err(e.into());
                    }
                }
                tracing::error!("Value in the location the path references is: {fragment}");
            }
            Err(e.into())
        }
    }
}

/// extract the queue item id from the `Location` header Jenkins returns for a
/// build trigger, which looks like `https://jenkins.example.com/queue/item/1234/`
fn queue_item_id_from_location(location: &str) -> Option<QueueItemId> {
    let mut segments = location.split('/').filter(|segment| !segment.is_empty());
    while let Some(segment) = segments.next() {
        if segment == "item" {
            return segments
                .next()
                .and_then(|id| id.parse::<u64>().ok())
                .map(QueueItemId);
        }
    }
    None
}

/// turn a progressive console response into a [`ConsoleChunk`]
///
/// Falls back to advancing by the size of the body if Jenkins sent no usable
/// `X-Text-Size` header, which keeps a follower from re-reading the same bytes.
fn console_chunk(start: u64, headers: &reqwest::header::HeaderMap, body: &[u8]) -> ConsoleChunk {
    let next_start = headers
        .get("X-Text-Size")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| start.saturating_add(u64::try_from(body.len()).unwrap_or(u64::MAX)));
    let more_data = headers
        .get("X-More-Data")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    ConsoleChunk {
        data: body.to_vec(),
        next_start,
        more_data,
    }
}

/// the `tree` specification selecting the build fields this crate models
///
/// Kept in one place because the job endpoint that lists a job's builds and
/// the endpoints that fetch a single build have to agree on it, or a build
/// fetched one way would be missing fields it has when fetched the other way.
pub(crate) const BUILD_TREE: &str = "number,result,building,timestamp,duration,url,\
actions[_class,lastBuiltRevision[SHA1,branch[SHA1,name]],remoteUrls,\
causes[_class,shortDescription,upstreamProject,upstreamBuild]]";

/// the `tree` specification selecting a job's SCM configuration
pub(crate) const SCM_TREE: &str =
    "scm[_class,branches[name],userRemoteConfigs[url,name,credentialsId,refspec]]";

/// the endpoint used to fetch a CSRF crumb
const CRUMB_ENDPOINT: &str = "crumbIssuer/api/json";

/// generate the inherent impl shared by the blocking and async clients
///
/// The two clients differ only in the reqwest types they use, whether their
/// methods are `async`, and how they sleep between polls, so the whole body is
/// written once here and instantiated twice rather than kept in sync by hand.
macro_rules! client_impl {
    (
        $client:ident,
        $reqwest_client:ty,
        $sleep:expr,
        [$($async:tt)*],
        [$($await:tt)*]
    ) => {
        impl $client {
            /// create a new client from a caller-supplied reqwest client and
            /// a config that was either manually created or previously loaded
            /// via [`JenkinsInstance::from_config_file`]
            ///
            /// This crate takes no opinion on the TLS backend, so building the
            /// reqwest client is the caller's job: add reqwest as your own
            /// dependency, enable the TLS features you want there and pass the
            /// resulting client in.
            ///
            /// Two settings on that client matter here:
            ///
            /// - Build it with `.cookie_store(true)` if you intend to trigger
            ///   builds. Jenkins' `StrictCrumbIssuer` ties a CSRF crumb to the
            ///   session it was issued for, so without a cookie jar every
            ///   state-changing request gets a fresh session and the crumb is
            ///   rejected. Read-only use does not need it.
            /// - Set a `.user_agent(..)` so your traffic is identifiable in the
            ///   controller's access log.
            ///
            /// # Errors
            ///
            /// this fails if the URL in the configuration can not be parsed
            pub fn new(
                client: $reqwest_client,
                config: &JenkinsInstance,
            ) -> Result<Self, crate::Error> {
                Ok(Self {
                    client,
                    base_url: base_url_from_instance_config(config)?,
                    auth: config.auth.clone(),
                    crumb: Arc::new(RwLock::new(None)),
                })
            }

            /// create a new client from a caller-supplied reqwest client and a
            /// TOML config file
            ///
            /// See [`Self::new`] for what the reqwest client has to provide.
            ///
            /// # Errors
            ///
            /// this fails if the configuration file can not be found or
            /// parsed, or the URL in the configuration can not be parsed
            pub fn from_config_file(
                client: $reqwest_client,
                path: &std::path::Path,
            ) -> Result<Self, crate::Error> {
                let config = JenkinsInstance::from_config_file(path)?;
                Self::new(client, &config)
            }

            /// the base URL of the Jenkins controller this client talks to
            #[must_use]
            pub const fn base_url(&self) -> &Url {
                &self.base_url
            }

            /// perform a single HTTP request without consulting the crumb
            /// cache and without any refresh or retry logic
            $($async)* fn rest_single_attempt(
                &self,
                method: Method,
                endpoint: &str,
                parameters: &QueryParams<'_>,
                mime_type_and_body: Option<(&'static str, Vec<u8>)>,
                crumb: Option<&Crumb>,
            ) -> Result<(reqwest::StatusCode, reqwest::header::HeaderMap, bytes::Bytes), crate::Error> {
                let mut url = self
                    .base_url
                    .join(endpoint)
                    .map_err(crate::Error::CouldNotParseUrlFragment)?;
                parameters.add_to_url(&mut url);
                tracing::debug!(%url, %method, "Calling Jenkins");
                let req = self
                    .client
                    .request(method.clone(), url.clone())
                    .header(reqwest::header::ACCEPT, "application/json");
                let req = match self.auth {
                    JenkinsAuth::UserApiToken {
                        ref username,
                        ref api_token,
                    } => req.basic_auth(username, Some(api_token)),
                    JenkinsAuth::Anonymous { .. } => req,
                };
                let req = if let Some(crumb) = crumb {
                    req.header(crumb.crumb_request_field.as_str(), crumb.crumb.as_str())
                } else {
                    req
                };
                let req = if let Some((mime, data)) = mime_type_and_body {
                    log_request_body(mime, &data);
                    req.header("Content-Type", mime).body(data)
                } else {
                    req
                };
                let result = req.send() $($await)*;
                if let Err(ref e) = result {
                    tracing::error!(%url, %method, "Jenkins send error: {e:?}");
                }
                let result = result?;
                let status = result.status();
                let headers = result.headers().clone();
                let response_body = result.bytes() $($await)*?;
                log_response_body(&response_body);
                if !status.is_success() {
                    tracing::error!(%url, %method, "Jenkins status error: {status:?}");
                    return Err(error_response(status, &response_body));
                }
                Ok((status, headers, response_body))
            }

            /// the common choke point all REST API calls go through
            ///
            /// Read-only `GET` requests need no CSRF crumb and are sent
            /// straight through. State-changing requests get the cached crumb
            /// (lazily fetching one if the cache is empty) and, because a
            /// crumb is only valid for as long as its session, are retried
            /// exactly once with a freshly fetched crumb if Jenkins rejects
            /// the first attempt with a crumb-specific 403.
            $($async)* fn rest(
                &self,
                method: Method,
                endpoint: &str,
                parameters: QueryParams<'_>,
                mime_type_and_body: Option<(&'static str, Vec<u8>)>,
            ) -> Result<(reqwest::StatusCode, reqwest::header::HeaderMap, bytes::Bytes), crate::Error> {
                if method == Method::GET {
                    return self
                        .rest_single_attempt(method, endpoint, &parameters, mime_type_and_body, None)
                        $($await)*;
                }
                let crumb = self.ensure_crumb() $($await)*?;
                let first_attempt = self
                    .rest_single_attempt(
                        method.clone(),
                        endpoint,
                        &parameters,
                        mime_type_and_body.clone(),
                        crumb.as_ref(),
                    )
                    $($await)*;
                match first_attempt {
                    Err(ref e) if is_crumb_rejection(e) => {
                        tracing::debug!(
                            "Jenkins rejected the CSRF crumb, fetching a fresh one and retrying the request once"
                        );
                        {
                            let mut cache =
                                self.crumb.write().unwrap_or_else(PoisonError::into_inner);
                            // only invalidate the crumb we used, another clone
                            // of this client might have refreshed it already
                            if cache.as_ref().map(|c| c.crumb.as_str())
                                == crumb.as_ref().map(|c| c.crumb.as_str())
                            {
                                *cache = None;
                            }
                        }
                        let crumb = self
                            .fetch_and_cache_crumb()
                            $($await)*
                            .map_err(|e| crate::Error::CrumbRefreshFailed(Box::new(e)))?;
                        self.rest_single_attempt(
                            method,
                            endpoint,
                            &parameters,
                            mime_type_and_body,
                            crumb.as_ref(),
                        )
                        $($await)*
                    }
                    other => other,
                }
            }

            /// return the cached CSRF crumb, lazily fetching one to populate
            /// the cache if it is empty
            ///
            /// Returns `Ok(None)` on controllers that have CSRF protection
            /// disabled, where the crumb issuer answers with 404.
            $($async)* fn ensure_crumb(&self) -> Result<Option<Crumb>, crate::Error> {
                let cached = self
                    .crumb
                    .read()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                if let Some(crumb) = cached {
                    Ok(Some(crumb))
                } else {
                    self.fetch_and_cache_crumb() $($await)*
                }
            }

            /// fetch a CSRF crumb from the crumb issuer and store it in the
            /// cache shared between clones of this client
            $($async)* fn fetch_and_cache_crumb(&self) -> Result<Option<Crumb>, crate::Error> {
                let result = self
                    .rest_single_attempt(
                        Method::GET,
                        CRUMB_ENDPOINT,
                        &QueryParams::default(),
                        None,
                        None,
                    )
                    $($await)*;
                let (_status, _headers, response_body) = match result {
                    Ok(ok) => ok,
                    Err(crate::Error::ApiError { status, .. })
                        if status == reqwest::StatusCode::NOT_FOUND =>
                    {
                        tracing::debug!(
                            "the Jenkins crumb issuer returned 404, assuming CSRF protection is disabled"
                        );
                        return Ok(None);
                    }
                    Err(e) => return Err(e),
                };
                let crumb: Crumb = parse_json_response_body(&response_body)?;
                {
                    let mut cache = self.crumb.write().unwrap_or_else(PoisonError::into_inner);
                    *cache = Some(crumb.clone());
                }
                Ok(Some(crumb))
            }

            /// use this with endpoints that have no interesting response body
            ///
            /// # Errors
            ///
            /// this can return an error if the endpoint returns an error when
            /// creating the request body, when the web request fails or when
            /// the Jenkins API returns an error response
            pub $($async)* fn ignore_response_body<E>(&self, endpoint: &E) -> Result<(), crate::Error>
            where
                E: Endpoint + Sync,
            {
                let method = endpoint.method();
                let path = endpoint.endpoint();
                let parameters = endpoint.parameters();
                let mime_type_and_body = endpoint.body()?;
                self.rest(method, &path, parameters, mime_type_and_body) $($await)*?;
                Ok(())
            }

            /// use this with endpoints which return a JSON response
            ///
            /// # Errors
            ///
            /// this can return an error if the endpoint returns an error when
            /// creating the request body, when the web request fails, when the
            /// Jenkins API returns an error response or when the response can
            /// not be parsed as JSON into the result type
            pub $($async)* fn json_response_body<E, R>(&self, endpoint: &E) -> Result<R, crate::Error>
            where
                E: Endpoint + ReturnsJsonResponse + Sync,
                R: DeserializeOwned + std::fmt::Debug + Send,
            {
                let method = endpoint.method();
                let path = endpoint.endpoint();
                let parameters = endpoint.parameters();
                let mime_type_and_body = endpoint.body()?;
                let (status, _headers, response_body) =
                    self.rest(method, &path, parameters, mime_type_and_body) $($await)*?;
                if response_body.is_empty() {
                    Err(crate::Error::EmptyResponseBody(status))
                } else {
                    parse_json_response_body(&response_body)
                }
            }

            /// use this with endpoints which return a plain text response,
            /// e.g. a build's console log
            ///
            /// # Errors
            ///
            /// this can return an error if the endpoint returns an error when
            /// creating the request body, when the web request fails or when
            /// the Jenkins API returns an error response
            ///
            /// The response body is decoded lossily, because a console log
            /// contains whatever bytes the build wrote and is not guaranteed
            /// to be valid UTF-8.
            pub $($async)* fn text_response_body<E>(&self, endpoint: &E) -> Result<String, crate::Error>
            where
                E: Endpoint + Sync,
            {
                let method = endpoint.method();
                let path = endpoint.endpoint();
                let parameters = endpoint.parameters();
                let mime_type_and_body = endpoint.body()?;
                let (_status, _headers, response_body) =
                    self.rest(method, &path, parameters, mime_type_and_body) $($await)*?;
                Ok(String::from_utf8_lossy(&response_body).into_owned())
            }

            /// report which user Jenkins resolves this client's credentials to
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn whoami(&self) -> Result<WhoAmI, crate::Error> {
                let endpoint = WhoAmIEndpoint::builder().build()?;
                self.json_response_body(&endpoint) $($await)*
            }


            /// list the controller's views as a tree
            ///
            /// With the nested-view plugin a view can contain other views, so
            /// the result is a forest of top-level views rather than a flat
            /// list. `depth` bounds how far the nesting is followed; `None`
            /// uses [`ListViews::DEFAULT_DEPTH`].
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn views(&self, depth: Option<u8>) -> Result<Vec<View>, crate::Error> {
                let mut builder = ListViews::builder();
                if let Some(depth) = depth {
                    builder.depth(depth);
                }
                let endpoint = builder.build()?;
                let wrapper: ViewsWrapper<View> = self.json_response_body(&endpoint) $($await)*?;
                Ok(wrapper.views)
            }

            /// fetch one view by its path of names from the root
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 404 when no view
            /// exists at that path
            pub $($async)* fn view(
                &self,
                path: &[ViewName],
                depth: Option<u8>,
            ) -> Result<View, crate::Error> {
                let mut builder = GetView::builder();
                builder.path(path.to_vec());
                if let Some(depth) = depth {
                    builder.depth(depth);
                }
                let endpoint = builder.build()?;
                self.json_response_body(&endpoint) $($await)*
            }

            /// resolve a view given either a full `A/B/C` path or a bare name
            ///
            /// A bare name is looked for anywhere in the tree, because that is
            /// what a human types. Ambiguity is an error rather than a guess:
            /// picking one of several same-named views silently would quietly
            /// filter against the wrong set of jobs.
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails, no view of
            /// that name exists, or the name matches more than one view
            pub $($async)* fn resolve_view(
                &self,
                query: &str,
            ) -> Result<(Vec<ViewName>, View), crate::Error> {
                let roots = self.views(None) $($await)*?;
                let root = View {
                    views: roots,
                    ..View::default()
                };
                let path: Vec<ViewName> = query
                    .split('/')
                    .filter(|segment| !segment.is_empty())
                    .map(|segment| ViewName(segment.to_owned()))
                    .collect();
                if path.len() > 1
                    && let Some(view) = root.find_path(&path)
                {
                    return Ok((path, view.clone()));
                }
                let Some(wanted) = path.last() else {
                    return Err(crate::Error::ViewNotFound {
                        query: query.to_owned(),
                    });
                };
                let mut matches = root.find_by_name(wanted);
                match matches.len() {
                    0 => Err(crate::Error::ViewNotFound {
                        query: query.to_owned(),
                    }),
                    1 => {
                        let (found_path, view) = matches.remove(0);
                        Ok((found_path, view.clone()))
                    }
                    _ => Err(crate::Error::AmbiguousView {
                        query: query.to_owned(),
                        candidates: matches
                            .iter()
                            .map(|(path, _)| {
                                path.iter()
                                    .map(|name| name.0.as_str())
                                    .collect::<Vec<_>>()
                                    .join("/")
                            })
                            .collect(),
                    }),
                }
            }

            /// list every job on the controller together with its SCM
            /// configuration
            ///
            /// This is one request regardless of how many jobs the controller
            /// has, and it is the intended way to work out which jobs belong
            /// to a given git repository: match on
            /// [`crate::types::UserRemoteConfig::url`] rather than on the job
            /// naming convention, which is not reliable.
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn jobs_with_scm(&self) -> Result<Vec<Job>, crate::Error> {
                let endpoint = ListJobs::builder().with_scm(true).build()?;
                let wrapper: JobsWrapper<Job> = self.json_response_body(&endpoint) $($await)*?;
                Ok(wrapper.jobs)
            }

            /// list every job on the controller, without SCM configuration
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn jobs(&self) -> Result<Vec<Job>, crate::Error> {
                let endpoint = ListJobs::builder().build()?;
                let wrapper: JobsWrapper<Job> = self.json_response_body(&endpoint) $($await)*?;
                Ok(wrapper.jobs)
            }

            /// fetch a single job, including its SCM configuration and its
            /// upstream and downstream jobs
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 404 if no job of
            /// that name exists
            pub $($async)* fn job(&self, name: &JobName) -> Result<Job, crate::Error> {
                let endpoint = GetJob::builder().name(name.clone()).build()?;
                self.json_response_body(&endpoint) $($await)*
            }

            /// fetch a job's raw `config.xml`
            ///
            /// Prefer [`Self::job_config`] unless you need a part of the
            /// document it does not parse.
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 403 if the
            /// credentials lack the `Job/ExtendedRead` permission
            pub $($async)* fn job_config_xml(&self, name: &JobName) -> Result<String, crate::Error> {
                let endpoint = GetJobConfig::builder().name(name.clone()).build()?;
                self.text_response_body(&endpoint) $($await)*
            }

            /// fetch and parse a job's `config.xml`, most usefully for its
            /// triggers, which the JSON API does not report
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 403 if the
            /// credentials lack the `Job/ExtendedRead` permission, or if the
            /// document is not well-formed XML
            pub $($async)* fn job_config(&self, name: &JobName) -> Result<JobConfig, crate::Error> {
                let xml = self.job_config_xml(name) $($await)*?;
                JobConfig::parse(&xml)
            }

            /// the jobs this job triggers when it succeeds
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn downstream_projects(
                &self,
                name: &JobName,
            ) -> Result<Vec<JobName>, crate::Error> {
                let job = self.job(name) $($await)*?;
                Ok(job
                    .downstream_projects
                    .into_iter()
                    .filter_map(|downstream| downstream.name)
                    .collect())
            }

            /// list the most recent builds of a job, newest first
            ///
            /// `limit` defaults to [`ListBuilds::DEFAULT_LIMIT`].
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn builds(
                &self,
                name: &JobName,
                limit: Option<u64>,
            ) -> Result<Vec<Build>, crate::Error> {
                let mut builder = ListBuilds::builder();
                builder.name(name.clone());
                if let Some(limit) = limit {
                    builder.limit(limit);
                }
                let endpoint = builder.build()?;
                let wrapper: BuildsWrapper<Build> = self.json_response_body(&endpoint) $($await)*?;
                Ok(wrapper.builds)
            }

            /// fetch one specific build of a job
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 404 if the job has
            /// no build with that number
            pub $($async)* fn build(
                &self,
                name: &JobName,
                number: BuildNumber,
            ) -> Result<Build, crate::Error> {
                let endpoint = GetBuild::builder().name(name.clone()).number(number).build()?;
                self.json_response_body(&endpoint) $($await)*
            }

            /// fetch the build a job most recently started
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 404 if the job has
            /// never been built
            pub $($async)* fn last_build(&self, name: &JobName) -> Result<Build, crate::Error> {
                let endpoint = GetLastBuild::builder().name(name.clone()).build()?;
                self.json_response_body(&endpoint) $($await)*
            }

            /// fetch the console log of a build
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn console_text(
                &self,
                name: &JobName,
                number: BuildNumber,
            ) -> Result<String, crate::Error> {
                let endpoint = ConsoleText::builder()
                    .name(name.clone())
                    .number(number)
                    .build()?;
                self.text_response_body(&endpoint) $($await)*
            }

            /// fetch the build one of a job's permalinks currently points at
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 404 if the job has
            /// no build matching the permalink
            pub $($async)* fn permalink_build(
                &self,
                name: &JobName,
                permalink: BuildPermalink,
            ) -> Result<Build, crate::Error> {
                let endpoint = GetPermalinkBuild::builder()
                    .name(name.clone())
                    .permalink(permalink)
                    .build()?;
                self.json_response_body(&endpoint) $($await)*
            }

            /// read a build's console log from a byte offset onwards
            ///
            /// Call this again with [`ConsoleChunk::next_start`] for as long
            /// as [`ConsoleChunk::more_data`] is set to follow a running
            /// build.
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn console_progressive(
                &self,
                name: &JobName,
                number: BuildNumber,
                start: u64,
            ) -> Result<ConsoleChunk, crate::Error> {
                let endpoint = ProgressiveConsoleText::builder()
                    .name(name.clone())
                    .number(number)
                    .start(start)
                    .build()?;
                let method = endpoint.method();
                let path = endpoint.endpoint();
                let query = endpoint.parameters();
                let (_status, headers, response_body) =
                    self.rest(method, &path, query, None) $($await)*?;
                Ok(console_chunk(start, &headers, &response_body))
            }

            /// stop a running build
            ///
            /// Jenkins only acknowledges the request, so the build can keep
            /// running for a moment afterwards. Use [`Self::wait_for_build`]
            /// to see it finish.
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 404 for
            /// [`StopMode::Terminate`] and [`StopMode::Kill`] on a build that
            /// is not a Pipeline build
            pub $($async)* fn stop_build(
                &self,
                name: &JobName,
                number: BuildNumber,
                mode: StopMode,
            ) -> Result<(), crate::Error> {
                let endpoint = StopBuild::builder()
                    .name(name.clone())
                    .number(number)
                    .mode(mode)
                    .build()?;
                self.ignore_response_body(&endpoint) $($await)*
            }

            /// list the nodes of the controller with their executors
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn computers(&self) -> Result<Vec<Computer>, crate::Error> {
                let endpoint = ListComputers::builder().build()?;
                let wrapper: ComputersWrapper<Computer> =
                    self.json_response_body(&endpoint) $($await)*?;
                Ok(wrapper.computer)
            }

            /// list the builds currently executing on the controller, oldest
            /// first
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn running_builds(&self) -> Result<Vec<RunningBuild>, crate::Error> {
                let computers = self.computers() $($await)*?;
                Ok(running_builds(&computers))
            }

            /// cancel an item that is still waiting in the build queue
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 404 if the item is
            /// no longer in the queue
            pub $($async)* fn cancel_queue_item(
                &self,
                id: QueueItemId,
            ) -> Result<(), crate::Error> {
                let endpoint = CancelQueueItem::builder().id(id).build()?;
                self.ignore_response_body(&endpoint) $($await)*
            }

            /// everything currently waiting in the controller's build queue
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn queue(&self) -> Result<Vec<QueueItem>, crate::Error> {
                let endpoint = ListQueue::builder().build()?;
                let wrapper: QueueItemsWrapper<QueueItem> =
                    self.json_response_body(&endpoint) $($await)*?;
                Ok(wrapper.items)
            }

            /// fetch one specific queue item
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response, in particular a 404 once Jenkins has
            /// forgotten the item, which happens a few minutes after it turned
            /// into a build
            pub $($async)* fn queue_item(
                &self,
                id: QueueItemId,
            ) -> Result<QueueItem, crate::Error> {
                let endpoint = GetQueueItem::builder().id(id).build()?;
                self.json_response_body(&endpoint) $($await)*
            }

            /// trigger a build of a job and return the queue item it produced
            ///
            /// Jenkins does not answer with a build number: the trigger only
            /// enqueues the build, and the number is assigned when an executor
            /// picks it up. Feed the returned id to
            /// [`Self::wait_for_queue_item`] to get the build.
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails, Jenkins
            /// returns an error response, or the response does not carry a
            /// usable `Location` header
            pub $($async)* fn trigger_build(
                &self,
                name: &JobName,
                parameters: Vec<(String, String)>,
            ) -> Result<QueueItemId, crate::Error> {
                let endpoint = TriggerBuild::builder()
                    .name(name.clone())
                    .parameters(parameters)
                    .build()?;
                let method = endpoint.method();
                let path = endpoint.endpoint();
                let query = endpoint.parameters();
                let mime_type_and_body = endpoint.body()?;
                let (_status, headers, _response_body) =
                    self.rest(method, &path, query, mime_type_and_body) $($await)*?;
                let location = headers
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| {
                        crate::Error::NoQueueItemLocation(
                            "the response had no Location header".to_owned(),
                        )
                    })?;
                queue_item_id_from_location(location).ok_or_else(|| {
                    crate::Error::NoQueueItemLocation(location.to_owned())
                })
            }

            /// find the most recent build of a job that checked out a specific
            /// git revision
            ///
            /// Only works for jobs bound to an SCM; builds triggered by an
            /// upstream job carry no git data at all, so use
            /// [`Self::find_build_for_upstream`] for those.
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn find_build_for_revision(
                &self,
                name: &JobName,
                revision: &Revision,
                limit: Option<u64>,
            ) -> Result<Option<Build>, crate::Error> {
                let builds = self.builds(name, limit) $($await)*?;
                Ok(builds
                    .into_iter()
                    .find(|build| build.built_revision() == Some(revision)))
            }

            /// find the most recent build of a job that was triggered by a
            /// specific build of a specific upstream job
            ///
            /// This is how downstream jobs are correlated to a release:
            /// Jenkins records no git data for them, but it does record an
            /// upstream cause naming the job and build that triggered them.
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails or Jenkins
            /// returns an error response
            pub $($async)* fn find_build_for_upstream(
                &self,
                name: &JobName,
                upstream_job: &JobName,
                upstream_build: BuildNumber,
                limit: Option<u64>,
            ) -> Result<Option<Build>, crate::Error> {
                let builds = self.builds(name, limit) $($await)*?;
                Ok(builds.into_iter().find(|build| {
                    build.upstream_cause()
                        == Some((upstream_job, upstream_build))
                }))
            }

            /// poll a build until it reaches a terminal state
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails, Jenkins
            /// returns an error response, or the build has not finished within
            /// `timeout`
            pub $($async)* fn wait_for_build(
                &self,
                name: &JobName,
                number: BuildNumber,
                poll_interval: std::time::Duration,
                timeout: std::time::Duration,
            ) -> Result<Build, crate::Error> {
                let started = std::time::Instant::now();
                loop {
                    let build = self.build(name, number) $($await)*?;
                    if build.is_finished() {
                        return Ok(build);
                    }
                    let waited = started.elapsed();
                    if waited >= timeout {
                        return Err(crate::Error::Timeout {
                            what: format!("build {number} of job {name} to finish"),
                            waited,
                        });
                    }
                    ($sleep)(poll_interval) $($await)*;
                }
            }

            /// poll a queue item until Jenkins turns it into a build
            ///
            /// # Errors
            ///
            /// this can return an error if the web request fails, Jenkins
            /// returns an error response, the item was cancelled before it
            /// started, or it has not started within `timeout`
            pub $($async)* fn wait_for_queue_item(
                &self,
                id: QueueItemId,
                poll_interval: std::time::Duration,
                timeout: std::time::Duration,
            ) -> Result<BuildNumber, crate::Error> {
                let started = std::time::Instant::now();
                loop {
                    let item = self.queue_item(id) $($await)*?;
                    if item.cancelled == Some(true) {
                        return Err(crate::Error::NoQueueItemLocation(format!(
                            "queue item {id} was cancelled before it started"
                        )));
                    }
                    if let Some(executable) = item.executable
                        && let Some(number) = executable.number
                    {
                        return Ok(number);
                    }
                    let waited = started.elapsed();
                    if waited >= timeout {
                        return Err(crate::Error::Timeout {
                            what: format!("queue item {id} to start building"),
                            waited,
                        });
                    }
                    ($sleep)(poll_interval) $($await)*;
                }
            }
        }
    };
}

/// the runtime client object for a Jenkins controller (blocking variant)
///
/// Read-only `GET` requests are sent straight through. State-changing requests
/// carry a CSRF crumb, which is fetched lazily, cached and shared between
/// clones of this client, and refreshed once if Jenkins rejects it.
#[derive(derive_more::Debug, Clone)]
pub struct Jenkins {
    /// the HTTP client to use
    client: reqwest::blocking::Client,
    /// the base URL of the Jenkins controller
    base_url: Url,
    /// the authentication information from the instance configuration
    #[debug(skip)]
    auth: JenkinsAuth,
    /// the cached CSRF crumb, shared between clones of this client
    ///
    /// this is a std [RwLock] (not an async-aware one) and its guard is never
    /// held across an await
    #[debug(skip)]
    crumb: Arc<RwLock<Option<Crumb>>>,
}

/// the runtime client object for a Jenkins controller (async variant)
///
/// Read-only `GET` requests are sent straight through. State-changing requests
/// carry a CSRF crumb, which is fetched lazily, cached and shared between
/// clones of this client, and refreshed once if Jenkins rejects it.
#[derive(derive_more::Debug, Clone)]
pub struct JenkinsAsync {
    /// the HTTP client to use
    client: reqwest::Client,
    /// the base URL of the Jenkins controller
    base_url: Url,
    /// the authentication information from the instance configuration
    #[debug(skip)]
    auth: JenkinsAuth,
    /// the cached CSRF crumb, shared between clones of this client
    ///
    /// this is a std [RwLock] (not an async-aware one) and its guard is never
    /// held across an await
    #[debug(skip)]
    crumb: Arc<RwLock<Option<Crumb>>>,
}

client_impl!(
    JenkinsAsync,
    reqwest::Client,
    |d: std::time::Duration| tokio::time::sleep(d),
    [async],
    [.await]
);

client_impl!(
    Jenkins,
    reqwest::blocking::Client,
    |d: std::time::Duration| std::thread::sleep(d),
    [],
    []
);

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_console_chunk_uses_the_jenkins_headers() -> Result<(), Box<dyn std::error::Error>> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Text-Size", "150".parse()?);
        headers.insert("X-More-Data", "true".parse()?);
        let chunk = console_chunk(100, &headers, b"hello");
        assert_eq!(chunk.next_start, 150);
        assert!(chunk.more_data);
        assert_eq!(chunk.data, b"hello");
        Ok(())
    }

    #[test]
    fn test_console_chunk_without_headers_is_final_and_advances() {
        let chunk = console_chunk(100, &reqwest::header::HeaderMap::new(), b"hello");
        assert_eq!(chunk.next_start, 105);
        assert!(!chunk.more_data);
    }

    #[test]
    fn test_encode_path_segment_handles_spaces() {
        assert_eq!(
            encode_path_segment("saltation - redmine_api - Check Only Rust Package - bookworm"),
            "saltation%20-%20redmine_api%20-%20Check%20Only%20Rust%20Package%20-%20bookworm"
        );
    }

    #[test]
    fn test_encode_path_segment_leaves_underscore_names_alone() {
        assert_eq!(
            encode_path_segment(
                "saltation___sync_ldap_subtrees___Debian_Rust_Package___bookworm___amd64"
            ),
            "saltation___sync_ldap_subtrees___Debian_Rust_Package___bookworm___amd64"
        );
    }

    #[test]
    fn test_encode_path_segment_escapes_slashes() {
        // a slash inside a job name must not turn into a path separator
        assert_eq!(encode_path_segment("some/job"), "some%2Fjob");
    }

    #[test]
    fn test_error_message_reduces_an_html_page_to_its_title() {
        // this is the shape Jenkins returns for a 404 on an unknown job
        let html = b"<html><head><meta http-equiv=\'refresh\'><title>Error 404 Not Found</title>\
</head><body><h2>HTTP ERROR 404 Not Found</h2><table><tr><th>URI:</th></tr></table>\
<footer>lots and lots of markup</footer></body></html>";
        let message = error_message(reqwest::StatusCode::NOT_FOUND, html);
        assert_eq!(message, "Error 404 Not Found");
    }

    #[test]
    fn test_error_message_falls_back_to_the_status_reason() {
        assert_eq!(
            error_message(reqwest::StatusCode::NOT_FOUND, b""),
            "Not Found"
        );
        assert_eq!(
            error_message(reqwest::StatusCode::NOT_FOUND, b"   \n  "),
            "Not Found"
        );
        // HTML with no title has nothing useful to show either
        assert_eq!(
            error_message(
                reqwest::StatusCode::NOT_FOUND,
                b"<html><body>x</body></html>"
            ),
            "Not Found"
        );
    }

    #[test]
    fn test_error_message_keeps_a_plain_text_body() {
        // a rejected crumb is plain text and is exactly the case where the
        // body is the useful part
        let message = error_message(
            reqwest::StatusCode::FORBIDDEN,
            b"No valid crumb was included in the request",
        );
        assert_eq!(message, "No valid crumb was included in the request");
    }

    #[test]
    fn test_error_message_caps_a_huge_plain_text_body() {
        let huge = "x".repeat(5000);
        let message = error_message(reqwest::StatusCode::BAD_GATEWAY, huge.as_bytes());
        assert_eq!(message.chars().count(), MAX_ERROR_MESSAGE_LEN + 1);
        assert!(message.ends_with('…'));
    }

    #[test]
    fn test_crumb_rejection_still_detected_after_summarising() {
        // the crumb check runs on the summarised message, so the two have to
        // keep working together
        let error = error_response(
            reqwest::StatusCode::FORBIDDEN,
            b"No valid crumb was included in the request",
        );
        assert!(is_crumb_rejection(&error));
    }

    #[test]
    fn test_is_crumb_rejection() {
        let crumb_403 = crate::Error::ApiError {
            status: reqwest::StatusCode::FORBIDDEN,
            message: "No valid crumb was included in the request".to_owned(),
        };
        assert!(is_crumb_rejection(&crumb_403));
        let plain_403 = crate::Error::ApiError {
            status: reqwest::StatusCode::FORBIDDEN,
            message: "user is missing the Job/Build permission".to_owned(),
        };
        assert!(!is_crumb_rejection(&plain_403));
        let not_found = crate::Error::ApiError {
            status: reqwest::StatusCode::NOT_FOUND,
            message: "crumb".to_owned(),
        };
        assert!(!is_crumb_rejection(&not_found));
    }
}
