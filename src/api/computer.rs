//! Computer (controller and agent) Rest API Endpoint definitions
//!
//! Jenkins has no endpoint listing running builds directly. What is running is
//! a property of the executors, so it is read from the computer list.
//!
//! [Jenkins Remote Access API documentation](https://www.jenkins.io/doc/book/using/remote-access-api/)

use std::borrow::Cow;

use derive_builder::Builder;
use reqwest::Method;

use crate::api::{Endpoint, QueryParams, ReturnsJsonResponse};

/// the `tree` specification selecting what an executor is currently running
const EXECUTABLE_TREE: &str = "currentExecutable[number,url,timestamp]";

/// The endpoint listing the controller and its agents with their executors
///
/// Both executor lists are requested because Pipeline builds run on the
/// flyweight `oneOffExecutors` rather than on regular ones.
#[derive(Debug, Clone, Default, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "derive_builder requires named-field syntax even for a parameterless endpoint"
)]
pub struct ListComputers {}

impl ReturnsJsonResponse for ListComputers {}

impl ListComputers {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> ListComputersBuilder {
        ListComputersBuilder::default()
    }
}

impl Endpoint for ListComputers {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        "computer/api/json".into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.push(
            "tree",
            format!(
                "computer[displayName,executors[{EXECUTABLE_TREE}],\
oneOffExecutors[{EXECUTABLE_TREE}]]"
            ),
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
    fn test_list_computers_endpoint() -> Result<(), Box<dyn Error>> {
        let endpoint = ListComputers::builder().build()?;
        assert_eq!(endpoint.endpoint(), "computer/api/json");
        Ok(())
    }
}
