# jenkins-api

Rust client library for the Jenkins REST API.

The crate concerns itself with **triggering jobs, reading job results and
waiting for jobs**. Configuring Jenkins is deliberately out of scope — that is
owned by Puppet and the Job DSL — so there is no endpoint here that creates or
edits a job.

Both a blocking (`Jenkins`) and an async (`JenkinsAsync`) client are provided
and always compiled; there are no feature flags to choose between them.

## What it can do

| Area | Operations |
|---|---|
| Discovery | `jobs`, `jobs_with_scm`, `job`, `downstream_projects` |
| Build results | `builds`, `build`, `last_build`, `console_text` |
| Correlation | `find_build_for_revision`, `find_build_for_upstream` |
| Triggering | `trigger_build`, `queue`, `queue_item` |
| Waiting | `wait_for_build`, `wait_for_queue_item` |
| Diagnostics | `whoami` |

## Correlating a build to a commit

There are two mechanisms and you need both, because they apply to different
kinds of job:

- Jobs bound to an SCM record the commit they checked out, so
  `find_build_for_revision` matches on the git plugin's `lastBuiltRevision`.
- Jobs triggered by an upstream job — a package-publish job, say — record **no
  git data at all**. For those, `find_build_for_upstream` matches on the
  upstream cause naming the job and build number that triggered them.

## Finding the jobs that build a repository

`jobs_with_scm` returns every job on the controller together with its git
remote URLs and branches in a single request. Matching a repository against
those remote URLs is far more reliable than trying to parse the job naming
convention, which tends not to be one.

## What it deliberately does not do

There is no `notifyCommit` equivalent. Where a forge already notifies Jenkins
on push, a second notification is at best redundant, and several notifications
arriving in quick succession for the same repository can make Jenkins start the
same job more than once.

## Usage

```rust,no_run
use jenkins_api::{Error, JenkinsAsync, JobName, Revision};

/// which jobs build this repository, and on which branches?
async fn jobs_building(remote: &str) -> Result<(), Error> {
    let path = std::path::Path::new("jenkins_instance.toml");
    let jenkins = JenkinsAsync::from_config_file(path)?;
    for job in jenkins.jobs_with_scm().await? {
        let Some(scm) = &job.scm else { continue };
        let builds_it = scm
            .user_remote_configs
            .iter()
            .any(|config| config.url.as_deref() == Some(remote));
        if builds_it {
            println!("{:?} builds {:?}", job.name, scm.branches);
        }
    }
    Ok(())
}

/// did a specific commit build successfully?
async fn build_for(name: &JobName, revision: &Revision) -> Result<(), Error> {
    let path = std::path::Path::new("jenkins_instance.toml");
    let jenkins = JenkinsAsync::from_config_file(path)?;
    let found = jenkins
        .find_build_for_revision(name, revision, None)
        .await?;
    if let Some(build) = found {
        println!("{:?} -> {:?}", build.number, build.result);
    }
    Ok(())
}
```

## Configuration

The client is built from a TOML file:

```toml
url = "https://jenkins.example.com/"
# optional, for a controller behind a private CA
ca_certificate = "/etc/ssl/certs/example-ca.pem"
username = "someuser"
api_token = "the API token from the user's Jenkins account settings"
```

Anonymous access is also supported, for controllers that allow unauthenticated
reads:

```toml
url = "https://jenkins.example.com/"
anonymous = true
```

Read-only `GET` requests are sent straight through. State-changing requests
carry a CSRF crumb, which is fetched lazily, cached, shared between clones of
the client and refreshed once if Jenkins rejects it — which is why the client
keeps a cookie jar, since `StrictCrumbIssuer` ties a crumb to its session.

## Tests

`cargo test` runs the unit tests and the offline fixture tests, which replay
captured responses from a real controller through the production parsing code.

The live tests are all `#[ignore]`d and additionally skip themselves when
`JENKINS_TEST_INSTANCE_CONFIG` is unset, so `cargo test -- --include-ignored`
passes on a machine with no Jenkins access. To run them for real, point that
variable at an instance config file (directly or via a gitignored `.env`) and
run `cargo test -- --ignored`.
