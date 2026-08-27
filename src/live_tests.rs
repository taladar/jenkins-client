//! Tests that talk to a real Jenkins controller.
//!
//! Every test here is `#[ignore]`d so a plain `cargo test` does not need a
//! Jenkins, and each one returns `Ok` without doing anything when
//! `JENKINS_TEST_INSTANCE_CONFIG` is not set, so `cargo test -- --include-ignored`
//! also passes on a machine that has no access to one.
//!
//! To actually run them, point `JENKINS_TEST_INSTANCE_CONFIG` at a TOML file
//! of the shape [`crate::config::JenkinsInstance`] parses — either directly in
//! the environment or via a `.env` file, which is gitignored — and run
//! `cargo test -- --ignored`.

use pretty_assertions::assert_eq;
use std::error::Error;
use std::path::PathBuf;
use tracing_test::traced_test;

use crate::api::{Jenkins, JenkinsAsync};
use crate::types::JobName;

/// the environment variable naming the instance config file to test against
const CONFIG_ENV_VAR: &str = "JENKINS_TEST_INSTANCE_CONFIG";

/// load a `.env` file if there is one, ignoring the case where there is not
fn load_dotenv() -> Result<(), Box<dyn Error>> {
    if let Err(e) = dotenvy::dotenv()
        && !e.not_found()
    {
        return Err(e.into());
    }
    Ok(())
}

/// the path to the instance config file, if one is configured
fn instance_config_path() -> Result<Option<PathBuf>, Box<dyn Error>> {
    load_dotenv()?;
    match std::env::var(CONFIG_ENV_VAR) {
        Ok(path) => Ok(Some(PathBuf::from(path))),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// the user agent the test clients identify themselves with
const TEST_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// build a blocking client for the configured instance, or `None` when no
/// instance is configured and the test should skip
///
/// Building the reqwest client here rather than inside the library is the
/// whole point of the TLS-agnostic design: the crate under test has no TLS
/// backend of its own, the test build picks `rustls` via its dev-dependency.
fn blocking_client() -> Result<Option<Jenkins>, Box<dyn Error>> {
    let Some(path) = instance_config_path()? else {
        tracing::warn!("{CONFIG_ENV_VAR} not set, skipping live test");
        return Ok(None);
    };
    let client = reqwest::blocking::ClientBuilder::new()
        .user_agent(TEST_USER_AGENT)
        .cookie_store(true)
        .build()?;
    Ok(Some(Jenkins::from_config_file(client, &path)?))
}

/// build an async client for the configured instance, or `None` when no
/// instance is configured and the test should skip
fn async_client() -> Result<Option<JenkinsAsync>, Box<dyn Error>> {
    let Some(path) = instance_config_path()? else {
        tracing::warn!("{CONFIG_ENV_VAR} not set, skipping live test");
        return Ok(None);
    };
    let client = reqwest::ClientBuilder::new()
        .user_agent(TEST_USER_AGENT)
        .cookie_store(true)
        .build()?;
    Ok(Some(JenkinsAsync::from_config_file(client, &path)?))
}

#[traced_test]
#[test]
#[ignore = "live test against a Jenkins controller, run with `cargo test -- --ignored` after setting JENKINS_TEST_INSTANCE_CONFIG"]
fn test_whoami() -> Result<(), Box<dyn Error>> {
    let Some(jenkins) = blocking_client()? else {
        return Ok(());
    };
    let who = jenkins.whoami()?;
    assert_eq!(who.authenticated, Some(true));
    Ok(())
}

#[traced_test]
#[tokio::test]
#[ignore = "live test against a Jenkins controller, run with `cargo test -- --ignored` after setting JENKINS_TEST_INSTANCE_CONFIG"]
async fn test_whoami_async() -> Result<(), Box<dyn Error>> {
    let Some(jenkins) = async_client()? else {
        return Ok(());
    };
    let who = jenkins.whoami().await?;
    assert_eq!(who.authenticated, Some(true));
    Ok(())
}

#[traced_test]
#[test]
#[ignore = "live test against a Jenkins controller, run with `cargo test -- --ignored` after setting JENKINS_TEST_INSTANCE_CONFIG"]
fn test_jobs_with_scm_returns_remote_urls() -> Result<(), Box<dyn Error>> {
    let Some(jenkins) = blocking_client()? else {
        return Ok(());
    };
    let jobs = jenkins.jobs_with_scm()?;
    assert!(!jobs.is_empty(), "the controller should have some jobs");
    let with_remote = jobs
        .iter()
        .filter(|job| {
            job.scm
                .as_ref()
                .is_some_and(|scm| scm.user_remote_configs.iter().any(|rc| rc.url.is_some()))
        })
        .count();
    assert!(
        with_remote > 0,
        "at least some jobs should be bound to a git remote"
    );
    Ok(())
}

#[traced_test]
#[tokio::test]
#[ignore = "live test against a Jenkins controller, run with `cargo test -- --ignored` after setting JENKINS_TEST_INSTANCE_CONFIG"]
async fn test_jobs_with_scm_returns_remote_urls_async() -> Result<(), Box<dyn Error>> {
    let Some(jenkins) = async_client()? else {
        return Ok(());
    };
    let jobs = jenkins.jobs_with_scm().await?;
    assert!(!jobs.is_empty(), "the controller should have some jobs");
    Ok(())
}

#[traced_test]
#[test]
#[ignore = "live test against a Jenkins controller, run with `cargo test -- --ignored` after setting JENKINS_TEST_INSTANCE_CONFIG"]
fn test_queue_is_readable() -> Result<(), Box<dyn Error>> {
    let Some(jenkins) = blocking_client()? else {
        return Ok(());
    };
    // an empty queue is the normal case, so this only asserts the request and
    // the parse succeed
    let _queue = jenkins.queue()?;
    Ok(())
}

/// a job whose name contains spaces, used to check path percent-encoding
///
/// Override with `JENKINS_TEST_JOB_WITH_SPACES` to run against a different
/// controller.
fn job_with_spaces() -> JobName {
    std::env::var("JENKINS_TEST_JOB_WITH_SPACES").map_or_else(
        |_| JobName("saltation - redmine_api - Check Only Rust Package - bookworm".to_owned()),
        JobName,
    )
}

#[traced_test]
#[test]
#[ignore = "live test against a Jenkins controller, run with `cargo test -- --ignored` after setting JENKINS_TEST_INSTANCE_CONFIG"]
fn test_job_name_with_spaces_round_trips() -> Result<(), Box<dyn Error>> {
    let Some(jenkins) = blocking_client()? else {
        return Ok(());
    };
    let name = job_with_spaces();
    let job = jenkins.job(&name)?;
    assert_eq!(job.name.as_ref(), Some(&name));
    Ok(())
}

#[traced_test]
#[test]
#[ignore = "live test against a Jenkins controller, run with `cargo test -- --ignored` after setting JENKINS_TEST_INSTANCE_CONFIG"]
fn test_views_are_a_tree() -> Result<(), Box<dyn Error>> {
    let Some(jenkins) = blocking_client()? else {
        return Ok(());
    };
    let views = jenkins.views(None)?;
    assert!(!views.is_empty(), "the controller should have some views");
    // the nested-view plugin is in use, so at least one view should have
    // children; a flat result would mean the tree spec is not nesting
    let nested = views.iter().filter(|v| !v.views.is_empty()).count();
    assert!(nested > 0, "expected at least one view with nested views");
    Ok(())
}

#[traced_test]
#[tokio::test]
#[ignore = "live test against a Jenkins controller, run with `cargo test -- --ignored` after setting JENKINS_TEST_INSTANCE_CONFIG"]
async fn test_resolve_view_and_fetch_it_async() -> Result<(), Box<dyn Error>> {
    let Some(jenkins) = async_client()? else {
        return Ok(());
    };
    let roots = jenkins.views(None).await?;
    // pick a view that actually has jobs so the round trip is meaningful
    let Some(name) = roots
        .iter()
        .flat_map(|v| v.views.iter())
        .find(|v| !v.all_jobs().is_empty())
        .and_then(|v| v.name.clone())
    else {
        return Ok(());
    };
    let (path, view) = jenkins.resolve_view(&name.0).await?;
    assert_eq!(view.name.as_ref(), Some(&name));
    let fetched = jenkins.view(&path, None).await?;
    assert_eq!(fetched.name.as_ref(), Some(&name));
    Ok(())
}

#[traced_test]
#[tokio::test]
#[ignore = "live test against a Jenkins controller, run with `cargo test -- --ignored` after setting JENKINS_TEST_INSTANCE_CONFIG"]
async fn test_find_build_for_revision_async() -> Result<(), Box<dyn Error>> {
    let Some(jenkins) = async_client()? else {
        return Ok(());
    };
    let name = job_with_spaces();
    let builds = jenkins.builds(&name, Some(50)).await?;
    let Some(with_revision) = builds.iter().find(|b| b.built_revision().is_some()) else {
        // a job that has never built anything from git has nothing to correlate
        return Ok(());
    };
    let revision = with_revision
        .built_revision()
        .ok_or("build lost its revision between two calls")?
        .clone();
    let found = jenkins
        .find_build_for_revision(&name, &revision, Some(50))
        .await?
        .ok_or("the revision we just read back was not found")?;
    assert_eq!(found.number, with_revision.number);
    Ok(())
}
