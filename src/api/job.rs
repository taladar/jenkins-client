//! Job Rest API Endpoint definitions
//!
//! [Jenkins Remote Access API documentation](https://www.jenkins.io/doc/book/using/remote-access-api/)

use std::borrow::Cow;

use derive_builder::Builder;
use reqwest::Method;

use crate::api::{
    BUILD_TREE, Endpoint, QueryParams, ReturnsJsonResponse, SCM_TREE, encode_path_segment,
};
use crate::types::JobName;

/// The endpoint listing all jobs on the controller
///
/// This is the cheap bulk lookup that makes job discovery practical: asking
/// for every job's SCM configuration in one request returns the whole
/// job-to-repository map at once, which is far more reliable than trying to
/// parse the job naming convention.
///
/// ```
/// use jenkins_client::api::job::ListJobs;
/// # fn main() -> Result<(), jenkins_client::Error> {
/// let endpoint = ListJobs::builder().with_scm(true).build()?;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Default, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct ListJobs {
    /// whether to include each job's SCM configuration (git remote URLs and
    /// branches) in the response
    #[builder(default)]
    with_scm: Option<bool>,
}

impl ReturnsJsonResponse for ListJobs {}

impl ListJobs {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> ListJobsBuilder {
        ListJobsBuilder::default()
    }
}

impl Endpoint for ListJobs {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        "api/json".into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        let tree = if self.with_scm == Some(true) {
            format!("jobs[_class,name,url,color,{SCM_TREE}]")
        } else {
            "jobs[_class,name,url,color]".to_owned()
        };
        params.push("tree", tree);
        params
    }
}

/// The endpoint for a single job
///
/// ```
/// use jenkins_client::api::job::GetJob;
/// # fn main() -> Result<(), jenkins_client::Error> {
/// let endpoint = GetJob::builder()
///     .name("saltation - redmine_api - Check Only Rust Package - bookworm")
///     .build()?;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct GetJob {
    /// the name of the job
    #[builder(setter(into))]
    name: JobName,
}

impl ReturnsJsonResponse for GetJob {}

impl GetJob {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> GetJobBuilder {
        GetJobBuilder::default()
    }
}

impl Endpoint for GetJob {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        format!("job/{}/api/json", encode_path_segment(&self.name.0)).into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.push(
            "tree",
            format!(
                "_class,name,url,color,buildable,disabled,inQueue,nextBuildNumber,{SCM_TREE},\
downstreamProjects[name],upstreamProjects[name]"
            ),
        );
        params
    }
}

/// The endpoint listing the recent builds of a job
///
/// Jenkins returns the builds newest first. `limit` maps onto the `tree`
/// parameter's `{0,limit}` range suffix rather than onto any pagination, so
/// asking for more builds costs one request either way.
///
/// ```
/// use jenkins_client::api::job::ListBuilds;
/// # fn main() -> Result<(), jenkins_client::Error> {
/// let endpoint = ListBuilds::builder()
///     .name("saltation___sync_ldap_subtrees___Debian_Rust_Package___bookworm___amd64")
///     .limit(30_u64)
///     .build()?;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct ListBuilds {
    /// the name of the job
    #[builder(setter(into))]
    name: JobName,
    /// how many of the most recent builds to return
    ///
    /// Defaults to [`ListBuilds::DEFAULT_LIMIT`] when not set.
    #[builder(default)]
    limit: Option<u64>,
}

impl ReturnsJsonResponse for ListBuilds {}

impl ListBuilds {
    /// the number of builds returned when no explicit limit is set
    pub const DEFAULT_LIMIT: u64 = 30;

    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> ListBuildsBuilder {
        ListBuildsBuilder::default()
    }
}

impl Endpoint for ListBuilds {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        format!("job/{}/api/json", encode_path_segment(&self.name.0)).into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        let limit = self.limit.unwrap_or(Self::DEFAULT_LIMIT);
        params.push("tree", format!("builds[{BUILD_TREE}]{{0,{limit}}}"));
        params
    }
}

/// The endpoint triggering a build of a job
///
/// Jenkins answers with `201 Created` and a `Location` header pointing at the
/// queue item, which [`crate::api::JenkinsAsync::trigger_build`] turns into a
/// [`crate::types::QueueItemId`].
///
/// Note that this crate deliberately offers no `notifyCommit` equivalent: on
/// installations where a forge already notifies Jenkins on push, a second
/// notification is at best redundant, and several notifications arriving in
/// quick succession for the same repository can make Jenkins start the same
/// job more than once.
///
/// ```
/// use jenkins_client::api::job::TriggerBuild;
/// # fn main() -> Result<(), jenkins_client::Error> {
/// let endpoint = TriggerBuild::builder()
///     .name("some-job")
///     .parameters(vec![("BRANCH".to_owned(), "master".to_owned())])
///     .build()?;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct TriggerBuild {
    /// the name of the job to build
    #[builder(setter(into))]
    name: JobName,
    /// the build parameters to pass, if the job is parameterized
    #[builder(default)]
    parameters: Vec<(String, String)>,
}

impl TriggerBuild {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> TriggerBuildBuilder {
        TriggerBuildBuilder::default()
    }
}

impl Endpoint for TriggerBuild {
    fn method(&self) -> Method {
        Method::POST
    }

    fn endpoint(&self) -> Cow<'static, str> {
        let encoded = encode_path_segment(&self.name.0);
        if self.parameters.is_empty() {
            format!("job/{encoded}/build").into()
        } else {
            format!("job/{encoded}/buildWithParameters").into()
        }
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.extend(
            self.parameters
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );
        params
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::error::Error;

    #[test]
    fn test_list_jobs_tree_without_scm() -> Result<(), Box<dyn Error>> {
        let endpoint = ListJobs::builder().build()?;
        let mut url =
            url::Url::parse("https://jenkins.example.com/")?.join(&endpoint.endpoint())?;
        endpoint.parameters().add_to_url(&mut url);
        assert_eq!(
            url.as_str(),
            "https://jenkins.example.com/api/json?tree=jobs%5B_class%2Cname%2Curl%2Ccolor%5D"
        );
        Ok(())
    }

    #[test]
    fn test_list_jobs_tree_with_scm_includes_remote_urls() -> Result<(), Box<dyn Error>> {
        let endpoint = ListJobs::builder().with_scm(true).build()?;
        let mut url =
            url::Url::parse("https://jenkins.example.com/")?.join(&endpoint.endpoint())?;
        endpoint.parameters().add_to_url(&mut url);
        let query = url.query().ok_or("no query string")?;
        let decoded = percent_encoding::percent_decode_str(query).decode_utf8()?;
        assert!(
            decoded.contains("userRemoteConfigs[url"),
            "tree should request the remote URLs, got {decoded}"
        );
        Ok(())
    }

    #[test]
    fn test_job_name_with_spaces_is_percent_encoded() -> Result<(), Box<dyn Error>> {
        let endpoint = GetJob::builder()
            .name("saltation - redmine_api - Check Only Rust Package - bookworm")
            .build()?;
        assert_eq!(
            endpoint.endpoint(),
            "job/saltation%20-%20redmine_api%20-%20Check%20Only%20Rust%20Package%20-%20bookworm/api/json"
        );
        Ok(())
    }

    #[test]
    fn test_list_builds_range_suffix() -> Result<(), Box<dyn Error>> {
        let endpoint = ListBuilds::builder()
            .name("some-job")
            .limit(5_u64)
            .build()?;
        let params = endpoint.parameters();
        let mut url = url::Url::parse("https://jenkins.example.com/")?;
        params.add_to_url(&mut url);
        let query = url.query().ok_or("no query string")?;
        let decoded = percent_encoding::percent_decode_str(query).decode_utf8()?;
        assert!(
            decoded.ends_with("]{0,5}"),
            "tree should carry the range suffix, got {decoded}"
        );
        Ok(())
    }

    #[test]
    fn test_list_builds_default_limit() -> Result<(), Box<dyn Error>> {
        let endpoint = ListBuilds::builder().name("some-job").build()?;
        let mut url = url::Url::parse("https://jenkins.example.com/")?;
        endpoint.parameters().add_to_url(&mut url);
        let query = url.query().ok_or("no query string")?;
        let decoded = percent_encoding::percent_decode_str(query).decode_utf8()?;
        assert!(
            decoded.ends_with("]{0,30}"),
            "default limit should be 30, got {decoded}"
        );
        Ok(())
    }

    #[test]
    fn test_trigger_build_picks_endpoint_by_parameters() -> Result<(), Box<dyn Error>> {
        let plain = TriggerBuild::builder().name("some-job").build()?;
        assert_eq!(plain.endpoint(), "job/some-job/build");
        assert_eq!(plain.method(), Method::POST);

        let parameterized = TriggerBuild::builder()
            .name("some-job")
            .parameters(vec![("BRANCH".to_owned(), "master".to_owned())])
            .build()?;
        assert_eq!(parameterized.endpoint(), "job/some-job/buildWithParameters");
        Ok(())
    }
}
