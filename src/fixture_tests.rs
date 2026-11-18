//! Offline tests that run captured Jenkins responses through the same parsing
//! code path the client uses at runtime.
//!
//! The fixtures in `tests/fixtures/` are verbatim responses from a real
//! Jenkins controller, so these tests pin down the two things that are easy to
//! get wrong and impossible to notice without real data: that the
//! heterogeneous `actions` array survives deserialization, and that the two
//! ways of correlating a build to a release both work.

use pretty_assertions::assert_eq;
use std::error::Error;
use tracing_test::traced_test;

use crate::api::parse_json_response_body;
use crate::job_config::{JobConfig, JobTrigger};
use crate::types::{
    Build, BuildNumber, BuildResult, BuildsWrapper, Crumb, JobName, QueueItem, QueueItemsWrapper,
    Revision, WhoAmI,
};

#[traced_test]
#[test]
fn test_parse_build_of_scm_bound_job() -> Result<(), Box<dyn Error>> {
    let build: Build =
        parse_json_response_body(include_bytes!("../tests/fixtures/build_scm_success.json"))?;
    assert_eq!(build.number, Some(BuildNumber(44)));
    assert_eq!(build.result, Some(BuildResult::Success));
    assert_eq!(build.building, Some(false));
    assert!(build.is_finished());
    // the real response has 20 action entries, most of them empty objects
    // because the tree filter did not match them
    assert!(build.actions.len() > 10);
    Ok(())
}

#[traced_test]
#[test]
fn test_correlate_scm_bound_build_by_revision() -> Result<(), Box<dyn Error>> {
    let build: Build =
        parse_json_response_body(include_bytes!("../tests/fixtures/build_scm_success.json"))?;
    assert_eq!(
        build.built_revision(),
        Some(&Revision(
            "602f5ccc62a3b534f69425a153c81a81bb1ab226".to_owned()
        ))
    );
    // an SCM-bound build has no upstream cause
    assert_eq!(build.upstream_cause(), None);
    Ok(())
}

#[traced_test]
#[test]
fn test_correlate_downstream_build_by_upstream_cause() -> Result<(), Box<dyn Error>> {
    let build: Build = parse_json_response_body(include_bytes!(
        "../tests/fixtures/build_downstream_upstream_cause.json"
    ))?;
    // a downstream job carries no git data at all, which is exactly why the
    // upstream cause is the only way to correlate it to a release
    assert_eq!(build.built_revision(), None);
    assert_eq!(
        build.upstream_cause(),
        Some((
            &JobName(
                "saltation___sync_ldap_subtrees___Debian_Rust_Package___bookworm___amd64"
                    .to_owned()
            ),
            BuildNumber(23)
        ))
    );
    Ok(())
}

#[traced_test]
#[test]
fn test_parse_job_builds_list() -> Result<(), Box<dyn Error>> {
    let wrapper: BuildsWrapper<Build> =
        parse_json_response_body(include_bytes!("../tests/fixtures/job_builds.json"))?;
    assert_eq!(wrapper.builds.len(), 3);
    // Jenkins returns builds newest first
    let numbers: Vec<_> = wrapper.builds.iter().filter_map(|b| b.number).collect();
    assert!(
        numbers.is_sorted_by(|a, b| a > b),
        "builds should be newest first, got {numbers:?}"
    );
    // every build in this fixture is SCM-bound and therefore has a revision
    for build in &wrapper.builds {
        assert!(
            build.built_revision().is_some(),
            "build {:?} should have a revision",
            build.number
        );
    }
    Ok(())
}

#[traced_test]
#[test]
fn test_find_build_for_revision_over_a_build_list() -> Result<(), Box<dyn Error>> {
    let wrapper: BuildsWrapper<Build> =
        parse_json_response_body(include_bytes!("../tests/fixtures/job_builds.json"))?;
    let wanted = Revision("936546cccd98545aae9dcb94b2ba91a9225f6ba7".to_owned());
    let found = wrapper
        .builds
        .iter()
        .find(|build| build.built_revision() == Some(&wanted))
        .ok_or("no build for the wanted revision")?;
    assert_eq!(found.number, Some(BuildNumber(23)));
    assert_eq!(found.result, Some(BuildResult::Success));
    Ok(())
}

#[traced_test]
#[test]
fn test_parse_empty_queue() -> Result<(), Box<dyn Error>> {
    let wrapper: QueueItemsWrapper<QueueItem> =
        parse_json_response_body(include_bytes!("../tests/fixtures/queue_empty.json"))?;
    assert_eq!(wrapper.items.len(), 0);
    Ok(())
}

#[traced_test]
#[test]
fn test_parse_crumb() -> Result<(), Box<dyn Error>> {
    let crumb: Crumb = parse_json_response_body(include_bytes!("../tests/fixtures/crumb.json"))?;
    // current Jenkins versions use this header name; the client sends the
    // crumb under whatever name the issuer reports rather than hardcoding it
    assert_eq!(crumb.crumb_request_field, "Jenkins-Crumb");
    assert!(!crumb.crumb.is_empty());
    Ok(())
}

#[traced_test]
#[test]
fn test_parse_whoami() -> Result<(), Box<dyn Error>> {
    let who: WhoAmI = parse_json_response_body(include_bytes!("../tests/fixtures/whoami.json"))?;
    assert_eq!(who.authenticated, Some(true));
    assert!(who.name.is_some());
    Ok(())
}

#[traced_test]
#[test]
fn test_job_config_of_scm_polled_job_starts_on_push() -> Result<(), Box<dyn Error>> {
    let config = JobConfig::parse(include_str!("../tests/fixtures/job_config_scm_poll.xml"))?;
    assert_eq!(config.kind, "project");
    assert!(!config.is_pipeline());
    assert!(!config.disabled);
    assert_eq!(
        config.triggers,
        vec![JobTrigger::ScmPoll {
            schedule: Some("H/15 * * * *".to_owned()),
            ignore_post_commit_hooks: false,
        }]
    );
    assert!(
        config.starts_on_push(),
        "a push notification schedules a poll of this job"
    );
    Ok(())
}

#[traced_test]
#[test]
fn test_job_config_of_upstream_triggered_job_does_not_start_on_push() -> Result<(), Box<dyn Error>>
{
    // bound to the same repository and branch as the polled job, yet a push
    // never starts it: only its upstream jobs do
    let config = JobConfig::parse(include_str!("../tests/fixtures/job_config_upstream.xml"))?;
    assert_eq!(
        config.triggers,
        vec![JobTrigger::Upstream {
            upstream_projects: vec![
                JobName::from("base_image_pull"),
                JobName::from("other_base_image"),
            ],
            threshold: Some(BuildResult::Success),
        }]
    );
    assert_eq!(
        config.upstream_projects().collect::<Vec<_>>(),
        vec![
            &JobName::from("base_image_pull"),
            &JobName::from("other_base_image")
        ]
    );
    assert!(
        !config.starts_on_push(),
        "only upstream builds start this job"
    );
    Ok(())
}

#[traced_test]
#[test]
fn test_job_config_of_pipeline_reads_property_triggers() -> Result<(), Box<dyn Error>> {
    // a pipeline keeps its triggers inside PipelineTriggersJobProperty and has
    // an empty top-level <triggers/>, which must not hide them
    let config = JobConfig::parse(include_str!(
        "../tests/fixtures/job_config_pipeline_gitlab.xml"
    ))?;
    assert!(config.is_pipeline());
    assert_eq!(
        config.triggers,
        vec![
            JobTrigger::GitLab {
                on_push: false,
                on_merge_request: true,
                on_note: true,
            },
            JobTrigger::Timer {
                schedule: Some("H 3 * * 1".to_owned()),
            },
        ]
    );
    assert_eq!(config.schedules().collect::<Vec<_>>(), vec!["H 3 * * 1"]);
    assert!(
        !config.starts_on_push(),
        "merge request, note and timer events are not pushes"
    );
    Ok(())
}
