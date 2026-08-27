//! Build queue Rest API Endpoint definitions
//!
//! A build trigger does not start a build directly: it puts an item into the
//! controller-wide build queue, which turns into a numbered build once an
//! executor picks it up. These endpoints let a caller follow that transition.
//!
//! [Jenkins Remote Access API documentation](https://www.jenkins.io/doc/book/using/remote-access-api/)

use std::borrow::Cow;

use derive_builder::Builder;
use reqwest::Method;

use crate::api::{Endpoint, QueryParams, ReturnsJsonResponse};
use crate::types::QueueItemId;

/// the `tree` specification selecting the queue item fields this crate models
const QUEUE_ITEM_TREE: &str =
    "_class,id,why,blocked,buildable,stuck,cancelled,executable[number,url],task[name,url]";

/// The endpoint listing everything currently in the build queue
#[derive(Debug, Clone, Default, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "derive_builder requires named-field syntax even for a parameterless endpoint"
)]
pub struct ListQueue {}

impl ReturnsJsonResponse for ListQueue {}

impl ListQueue {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> ListQueueBuilder {
        ListQueueBuilder::default()
    }
}

impl Endpoint for ListQueue {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        "queue/api/json".into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.push("tree", format!("items[{QUEUE_ITEM_TREE}]"));
        params
    }
}

/// The endpoint for one specific queue item
///
/// Jenkins keeps a queue item around for a few minutes after it turned into a
/// build, which is the window in which its `executable` field reveals the
/// build number a trigger produced.
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct GetQueueItem {
    /// the id of the queue item
    id: QueueItemId,
}

impl ReturnsJsonResponse for GetQueueItem {}

impl GetQueueItem {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> GetQueueItemBuilder {
        GetQueueItemBuilder::default()
    }
}

impl Endpoint for GetQueueItem {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        format!("queue/item/{}/api/json", self.id.0).into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.push("tree", QUEUE_ITEM_TREE);
        params
    }
}

/// The endpoint reporting which user Jenkins resolved the request credentials
/// to
///
/// Useful as a cheap connectivity and authentication check.
#[derive(Debug, Clone, Default, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "derive_builder requires named-field syntax even for a parameterless endpoint"
)]
pub struct WhoAmI {}

impl ReturnsJsonResponse for WhoAmI {}

impl WhoAmI {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> WhoAmIBuilder {
        WhoAmIBuilder::default()
    }
}

impl Endpoint for WhoAmI {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        "whoAmI/api/json".into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        params.push("tree", "authenticated,name");
        params
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::error::Error;

    #[test]
    fn test_list_queue_endpoint() -> Result<(), Box<dyn Error>> {
        let endpoint = ListQueue::builder().build()?;
        assert_eq!(endpoint.endpoint(), "queue/api/json");
        Ok(())
    }

    #[test]
    fn test_get_queue_item_endpoint() -> Result<(), Box<dyn Error>> {
        let endpoint = GetQueueItem::builder().id(QueueItemId(1234)).build()?;
        assert_eq!(endpoint.endpoint(), "queue/item/1234/api/json");
        Ok(())
    }

    #[test]
    fn test_whoami_endpoint() -> Result<(), Box<dyn Error>> {
        let endpoint = WhoAmI::builder().build()?;
        assert_eq!(endpoint.endpoint(), "whoAmI/api/json");
        Ok(())
    }
}
