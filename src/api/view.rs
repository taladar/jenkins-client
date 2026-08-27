//! View Rest API Endpoint definitions
//!
//! With the nested-view plugin a view can contain other views, so the view
//! list is a tree. Jenkins' `tree` parameter has no recursion, so the nesting
//! has to be spelled out to a fixed depth — which is what the `depth`
//! parameter of [`ListViews`] controls. One request at a sensible depth is
//! far cheaper than walking the tree with one request per view.
//!
//! [Jenkins Remote Access API documentation](https://www.jenkins.io/doc/book/using/remote-access-api/)

use std::borrow::Cow;

use derive_builder::Builder;
use reqwest::Method;

use crate::api::{Endpoint, QueryParams, ReturnsJsonResponse, encode_path_segment};
use crate::types::ViewName;

/// the fields of a single view this crate models, without the nesting
const VIEW_LEAF_TREE: &str = "_class,name,url,jobs[_class,name,url,color]";

/// Build the `tree` specification for a view tree `depth` levels deep.
///
/// Depth 1 is a flat list of views with their jobs and no children.
fn view_tree(depth: u8) -> String {
    let mut spec = VIEW_LEAF_TREE.to_owned();
    for _ in 1..depth {
        spec = format!("{VIEW_LEAF_TREE},views[{spec}]");
    }
    spec
}

/// The endpoint listing the controller's views as a tree
///
/// ```
/// use jenkins_client::api::view::ListViews;
/// # fn main() -> Result<(), jenkins_client::Error> {
/// let endpoint = ListViews::builder().depth(6_u8).build()?;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone, Default, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct ListViews {
    /// how many levels of nested views to ask for
    ///
    /// Defaults to [`ListViews::DEFAULT_DEPTH`], which is deeper than the
    /// nesting found in practice; asking for more levels than exist costs
    /// nothing but a longer query string.
    #[builder(default)]
    depth: Option<u8>,
}

impl ReturnsJsonResponse for ListViews {}

impl ListViews {
    /// the nesting depth requested when none is given
    pub const DEFAULT_DEPTH: u8 = 6;

    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> ListViewsBuilder {
        ListViewsBuilder::default()
    }
}

impl Endpoint for ListViews {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        "api/json".into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        let depth = self.depth.unwrap_or(Self::DEFAULT_DEPTH).max(1);
        params.push("tree", format!("views[{}]", view_tree(depth)));
        params
    }
}

/// The endpoint for one view, addressed by its path from the root
///
/// A nested view lives under its parents in the URL, so the path
/// `["Overview", "Beam"]` becomes `/view/Overview/view/Beam/`.
#[derive(Debug, Clone, Builder)]
#[builder(setter(strip_option), build_fn(error = "crate::Error"))]
pub struct GetView {
    /// the path of view names from the root down to the wanted view
    path: Vec<ViewName>,
    /// how many levels of nested views to ask for below this one
    #[builder(default)]
    depth: Option<u8>,
}

impl ReturnsJsonResponse for GetView {}

impl GetView {
    /// Create a builder for the endpoint.
    #[must_use]
    pub fn builder() -> GetViewBuilder {
        GetViewBuilder::default()
    }
}

impl Endpoint for GetView {
    fn method(&self) -> Method {
        Method::GET
    }

    fn endpoint(&self) -> Cow<'static, str> {
        let mut path = String::new();
        for name in &self.path {
            path.push_str("view/");
            path.push_str(&encode_path_segment(&name.0));
            path.push('/');
        }
        format!("{path}api/json").into()
    }

    fn parameters(&self) -> QueryParams<'_> {
        let mut params = QueryParams::default();
        let depth = self.depth.unwrap_or(ListViews::DEFAULT_DEPTH).max(1);
        params.push("tree", view_tree(depth));
        params
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::error::Error;

    #[test]
    fn test_view_tree_nests_one_level_per_depth() {
        assert_eq!(view_tree(1), VIEW_LEAF_TREE);
        assert_eq!(
            view_tree(2),
            format!("{VIEW_LEAF_TREE},views[{VIEW_LEAF_TREE}]")
        );
        // each extra level adds exactly one more `views[` bracket
        assert_eq!(view_tree(5).matches("views[").count(), 4);
    }

    #[test]
    fn test_list_views_default_depth_is_used() -> Result<(), Box<dyn Error>> {
        let endpoint = ListViews::builder().build()?;
        let mut url = url::Url::parse("https://jenkins.example.com/")?;
        endpoint.parameters().add_to_url(&mut url);
        let query = url.query().ok_or("no query string")?;
        let decoded = percent_encoding::percent_decode_str(query).decode_utf8()?;
        assert_eq!(
            decoded.matches("views[").count(),
            usize::from(ListViews::DEFAULT_DEPTH),
            "one outer views[ plus one per extra level"
        );
        Ok(())
    }

    #[test]
    fn test_zero_depth_is_clamped_to_one() -> Result<(), Box<dyn Error>> {
        // asking for depth 0 would otherwise produce an empty tree spec, which
        // Jenkins answers with every field of every view
        let endpoint = ListViews::builder().depth(0_u8).build()?;
        let mut url = url::Url::parse("https://jenkins.example.com/")?;
        endpoint.parameters().add_to_url(&mut url);
        let query = url.query().ok_or("no query string")?;
        let decoded = percent_encoding::percent_decode_str(query).decode_utf8()?;
        assert_eq!(decoded.matches("views[").count(), 1);
        Ok(())
    }

    #[test]
    fn test_get_view_builds_a_nested_path() -> Result<(), Box<dyn Error>> {
        let endpoint = GetView::builder()
            .path(vec![
                ViewName("Overview".to_owned()),
                ViewName("Beam".to_owned()),
            ])
            .build()?;
        assert_eq!(endpoint.endpoint(), "view/Overview/view/Beam/api/json");
        Ok(())
    }

    #[test]
    fn test_get_view_percent_encodes_view_names() -> Result<(), Box<dyn Error>> {
        let endpoint = GetView::builder()
            .path(vec![ViewName("BUO Bielefeld".to_owned())])
            .build()?;
        assert_eq!(endpoint.endpoint(), "view/BUO%20Bielefeld/api/json");
        Ok(())
    }

    #[test]
    fn test_get_view_with_an_empty_path_is_the_root() -> Result<(), Box<dyn Error>> {
        let endpoint = GetView::builder().path(Vec::new()).build()?;
        assert_eq!(endpoint.endpoint(), "api/json");
        Ok(())
    }
}
