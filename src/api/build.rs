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

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::error::Error;

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
