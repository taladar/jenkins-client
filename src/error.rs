//! The error type for the library

use thiserror::Error;

/// Error type for jenkins_api
#[derive(Debug, Error)]
pub enum Error {
    /// could not read config file
    #[error("could not read config file: {0}")]
    CouldNotReadConfigFile(std::io::Error),
    /// could not parse config
    #[error("could not parse config: {0}")]
    CouldNotParseConfig(toml::de::Error),
    /// could not parse URL in config
    #[error("could not parse URL in config: {0}")]
    CouldNotParseUrlInConfig(url::ParseError),
    /// could not parse URL fragment
    #[error("could not parse URL fragment: {0}")]
    CouldNotParseUrlFragment(url::ParseError),
    /// An error occurred in the reqwest library (HTTP)
    #[error("reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    /// An error occurred when serializing/deserializing JSON
    #[error("error in json serialization/deserialization: {0}")]
    SerdeJsonError(#[from] serde_path_to_error::Error<serde_json::Error>),
    /// Response body was empty so we can not deserialize it as JSON
    #[error("empty response body with status: {0}")]
    EmptyResponseBody(reqwest::StatusCode),
    /// uninitialized field in builder
    #[error("uninitialized field in builder: {0}")]
    UninitializedFieldInBuilder(#[from] derive_builder::UninitializedFieldError),
    /// the Jenkins API returned an error response
    #[error("Jenkins API error (HTTP status {status}): {message}")]
    ApiError {
        /// the HTTP status code of the response
        status: reqwest::StatusCode,
        /// the response body, or the status canonical reason for empty bodies
        message: String,
    },
    /// Jenkins rejected our credentials
    ///
    /// Unlike a CSRF crumb rejection this is not retried, because there is no
    /// second thing to try: the username and API token in the configuration
    /// are simply not accepted.
    #[error("Jenkins rejected the configured credentials (status {0})")]
    AuthenticationRejected(reqwest::StatusCode),
    /// fetching a fresh CSRF crumb after the cached one was rejected failed
    #[error("could not fetch a fresh CSRF crumb after the cached one was rejected: {0}")]
    CrumbRefreshFailed(Box<Self>),
    /// a build was expected to be queued but Jenkins returned no queue item
    ///
    /// Jenkins answers a build trigger with `201 Created` and a `Location`
    /// header pointing at the queue item; this is returned when that header is
    /// missing or does not have the expected shape.
    #[error("Jenkins did not return a usable queue item location: {0}")]
    NoQueueItemLocation(String),
    /// waiting for a build to reach a terminal state timed out
    #[error("timed out after {waited:?} waiting for {what}")]
    Timeout {
        /// what we were waiting for, e.g. a build of a specific job
        what: String,
        /// how long we waited before giving up
        waited: std::time::Duration,
    },
}
