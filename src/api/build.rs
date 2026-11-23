//! Build Rest API Endpoint definitions
//!
//! Builds are addressed underneath their job (`/job/<name>/<number>/`), so
//! these endpoints all take a job name as well as a build selector.
//!
//! [Jenkins Remote Access API documentation](https://www.jenkins.io/doc/book/using/remote-access-api/)

use std::borrow::Cow;

use derive_builder::Builder;
use reqwest::Method;

use crate::api::{BUILD_TREE, Endpoint, QueryParams, ReturnsJsonResponse, encode_path_segment};
use crate::types::{BuildNumber, JobName};

/// The endpoint for the build a job most recently started
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct GetLastBuild {
    /// the name of the job
    #[builder(setter(into))]
    name: JobName,
}

impl ReturnsJsonResponse for GetLastBuild {}

impl GetLastBuild {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> GetLastBuildBuilder {
        GetLastBuildBuilder::default()
    }
}

impl Endpoint for GetLastBuild {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        format!(
            "job/{}/lastBuild/api/json",
            encode_path_segment(&self.name.0)
        )
        .into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.push("tree", BUILD_TREE);
        params
    }
}

/// The endpoint for one specific build of a job
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct GetBuild {
    /// the name of the job
    #[builder(setter(into))]
    name: JobName,
    /// the number of the build within that job
    number: BuildNumber,
}

impl ReturnsJsonResponse for GetBuild {}

impl GetBuild {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> GetBuildBuilder {
        GetBuildBuilder::default()
    }
}

impl Endpoint for GetBuild {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        format!(
            "job/{}/{}/api/json",
            encode_path_segment(&self.name.0),
            self.number.0
        )
        .into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.push("tree", BUILD_TREE);
        params
    }
}

/// The endpoint returning the plain text console log of a build
///
/// This is the one endpoint in this crate that does not return JSON, so it has
/// to be used with `text_response_body` rather than `json_response_body`.
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct ConsoleText {
    /// the name of the job
    #[builder(setter(into))]
    name: JobName,
    /// the number of the build within that job
    number: BuildNumber,
}

impl ConsoleText {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> ConsoleTextBuilder {
        ConsoleTextBuilder::default()
    }
}

impl Endpoint for ConsoleText {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        format!(
            "job/{}/{}/consoleText",
            encode_path_segment(&self.name.0),
            self.number.0
        )
        .into()
    }
}

/// One of the permalinks Jenkins maintains for a job's notable builds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildPermalink {
    /// the last build that succeeded or was unstable
    LastSuccessful,
    /// the last build that succeeded without being unstable
    LastStable,
    /// the last build that failed
    LastFailed,
    /// the last build that was unstable
    LastUnstable,
    /// the last build that did not succeed, for whatever reason
    LastUnsuccessful,
    /// the last build that finished, whatever its result
    LastCompleted,
}

impl BuildPermalink {
    /// the URL path segment Jenkins uses for this permalink
    #[must_use]
    pub const fn path_segment(self) -> &'static str {
        match self {
            Self::LastSuccessful => "lastSuccessfulBuild",
            Self::LastStable => "lastStableBuild",
            Self::LastFailed => "lastFailedBuild",
            Self::LastUnstable => "lastUnstableBuild",
            Self::LastUnsuccessful => "lastUnsuccessfulBuild",
            Self::LastCompleted => "lastCompletedBuild",
        }
    }
}

/// The endpoint for the build a job's permalink currently points at
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct GetPermalinkBuild {
    /// the name of the job
    #[builder(setter(into))]
    name: JobName,
    /// which permalink to follow
    permalink: BuildPermalink,
}

impl ReturnsJsonResponse for GetPermalinkBuild {}

impl GetPermalinkBuild {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> GetPermalinkBuildBuilder {
        GetPermalinkBuildBuilder::default()
    }
}

impl Endpoint for GetPermalinkBuild {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        format!(
            "job/{}/{}/api/json",
            encode_path_segment(&self.name.0),
            self.permalink.path_segment()
        )
        .into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.push("tree", BUILD_TREE);
        params
    }
}

/// How forcefully to stop a running build
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StopMode {
    /// ask the build to abort, the same as the abort button in the web UI
    #[default]
    Abort,
    /// forcibly terminate a Pipeline build that ignored an abort
    ///
    /// Jenkins only offers this for Pipeline builds and answers 404 for any
    /// other job type.
    Terminate,
    /// hard-kill a Pipeline build without any cleanup
    ///
    /// This is a last resort that can leave the build's resources behind.
    /// Jenkins only offers it for Pipeline builds and answers 404 for any
    /// other job type.
    Kill,
}

impl StopMode {
    /// the URL path segment Jenkins uses for this way of stopping a build
    #[must_use]
    pub const fn path_segment(self) -> &'static str {
        match self {
            Self::Abort => "stop",
            Self::Terminate => "term",
            Self::Kill => "kill",
        }
    }
}

/// The endpoint stopping a running build
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct StopBuild {
    /// the name of the job
    #[builder(setter(into))]
    name: JobName,
    /// the number of the build within that job
    number: BuildNumber,
    /// how forcefully to stop it
    #[builder(default)]
    mode: StopMode,
}

impl StopBuild {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> StopBuildBuilder {
        StopBuildBuilder::default()
    }
}

impl Endpoint for StopBuild {
    fn method(&self) -> Method {
        Method::POST
    }

    fn endpoint(&self) -> Cow<'static, str> {
        format!(
            "job/{}/{}/{}",
            encode_path_segment(&self.name.0),
            self.number.0,
            self.mode.path_segment()
        )
        .into()
    }
}

/// The endpoint returning a build's console log from a byte offset onwards
///
/// Jenkins reports the offset to continue from in the `X-Text-Size` response
/// header and sets `X-More-Data: true` while the log can still grow.
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct ProgressiveConsoleText {
    /// the name of the job
    #[builder(setter(into))]
    name: JobName,
    /// the number of the build within that job
    number: BuildNumber,
    /// the byte offset to start reading from
    #[builder(default)]
    start: u64,
}

impl ProgressiveConsoleText {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> ProgressiveConsoleTextBuilder {
        ProgressiveConsoleTextBuilder::default()
    }
}

impl Endpoint for ProgressiveConsoleText {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        format!(
            "job/{}/{}/logText/progressiveText",
            encode_path_segment(&self.name.0),
            self.number.0
        )
        .into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.push("start", self.start);
        params
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::error::Error;

    #[test]
    fn test_get_permalink_build_endpoint() -> Result<(), Box<dyn Error>> {
        let endpoint = GetPermalinkBuild::builder()
            .name("some job")
            .permalink(BuildPermalink::LastSuccessful)
            .build()?;
        assert_eq!(
            endpoint.endpoint(),
            "job/some%20job/lastSuccessfulBuild/api/json"
        );
        Ok(())
    }

    #[test]
    fn test_stop_build_endpoint_per_mode() -> Result<(), Box<dyn Error>> {
        for (mode, expected) in [
            (StopMode::Abort, "job/some%20job/44/stop"),
            (StopMode::Terminate, "job/some%20job/44/term"),
            (StopMode::Kill, "job/some%20job/44/kill"),
        ] {
            let endpoint = StopBuild::builder()
                .name("some job")
                .number(BuildNumber(44))
                .mode(mode)
                .build()?;
            assert_eq!(endpoint.method(), Method::POST);
            assert_eq!(endpoint.endpoint(), expected);
        }
        Ok(())
    }

    #[test]
    fn test_stop_build_defaults_to_a_plain_abort() -> Result<(), Box<dyn Error>> {
        let endpoint = StopBuild::builder()
            .name("some-job")
            .number(BuildNumber(44))
            .build()?;
        assert_eq!(endpoint.endpoint(), "job/some-job/44/stop");
        Ok(())
    }

    #[test]
    fn test_progressive_console_text_endpoint() -> Result<(), Box<dyn Error>> {
        let endpoint = ProgressiveConsoleText::builder()
            .name("some-job")
            .number(BuildNumber(44))
            .start(1024_u64)
            .build()?;
        assert_eq!(
            endpoint.endpoint(),
            "job/some-job/44/logText/progressiveText"
        );
        let mut url = url::Url::parse("https://jenkins.example.com/")?;
        endpoint.parameters().add_to_url(&mut url);
        assert_eq!(url.query(), Some("start=1024"));
        Ok(())
    }

    #[test]
    fn test_get_build_endpoint() -> Result<(), Box<dyn Error>> {
        let endpoint = GetBuild::builder()
            .name("some-job")
            .number(BuildNumber(44))
            .build()?;
        assert_eq!(endpoint.endpoint(), "job/some-job/44/api/json");
        Ok(())
    }

    #[test]
    fn test_get_last_build_endpoint() -> Result<(), Box<dyn Error>> {
        let endpoint = GetLastBuild::builder().name("some-job").build()?;
        assert_eq!(endpoint.endpoint(), "job/some-job/lastBuild/api/json");
        Ok(())
    }

    #[test]
    fn test_console_text_endpoint_encodes_job_name() -> Result<(), Box<dyn Error>> {
        let endpoint = ConsoleText::builder()
            .name("saltation - redmine_api - Check Only Rust Package - bookworm")
            .number(BuildNumber(44))
            .build()?;
        assert_eq!(
            endpoint.endpoint(),
            "job/saltation%20-%20redmine_api%20-%20Check%20Only%20Rust%20Package%20-%20bookworm/44/consoleText"
        );
        Ok(())
    }
}
