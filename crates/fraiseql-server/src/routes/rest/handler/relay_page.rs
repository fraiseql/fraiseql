//! The body a REST relay route answers: the page's rows, its page info and cursors, and the
//! links that resume it.

use serde_json::{Value, json};

use crate::routes::rest::params::PaginationParams;

/// The cursor window parameters, which a link replaces; every other parameter it keeps.
const WINDOW: [&str; 4] = ["first", "after", "last", "before"];

/// `{"data": [<node>…], "meta": {…}, "links": {"next": …, "prev": …}}` for the connection a
/// relay read answered.
///
/// `meta` carries `hasNextPage`, `hasPreviousPage`, `startCursor` and `endCursor`, the window
/// the request asked for, and `total` when counted. `links.next` resumes after the page's last
/// row, `links.prev` before its first, each with the request's own parameters (its `?sort=`
/// above all, which the cursor was issued under), or `null` when there is no such page.
pub(super) fn body(
    connection: &Value,
    total: Option<u64>,
    pagination: &PaginationParams,
    route: &str,
    query_pairs: &[(&str, &str)],
) -> Value {
    let data: Vec<Value> = connection["edges"]
        .as_array()
        .map_or_default(|edges| edges.iter().map(|edge| edge["node"].clone()).collect());
    let page_info = &connection["pageInfo"];

    let mut meta = serde_json::Map::new();
    for key in ["hasNextPage", "hasPreviousPage", "startCursor", "endCursor"] {
        meta.insert(key.to_string(), page_info[key].clone());
    }
    let size = if let PaginationParams::Cursor {
        first,
        after,
        last,
        before,
    } = pagination
    {
        for (key, value) in [
            ("first", first.map(|f| json!(f))),
            ("after", after.as_ref().map(|a| json!(a))),
            ("last", last.map(|l| json!(l))),
            ("before", before.as_ref().map(|b| json!(b))),
        ] {
            if let Some(value) = value {
                meta.insert(key.to_string(), value);
            }
        }
        first.or(*last)
    } else {
        None
    };
    if let Some(total) = total {
        meta.insert("total".to_string(), json!(total));
    }

    let link = |more: &str, cursor: &str, count: &str, position: &str| {
        let cursor = page_info[cursor].as_str().filter(|_| page_info[more] == json!(true))?;
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in query_pairs.iter().filter(|(name, _)| !WINDOW.contains(name)) {
            query.append_pair(name, value);
        }
        if let Some(size) = size {
            query.append_pair(count, &size.to_string());
        }
        query.append_pair(position, cursor);
        Some(format!("{route}?{}", query.finish()))
    };
    let links = json!({
        "next": link("hasNextPage", "endCursor", "first", "after"),
        "prev": link("hasPreviousPage", "startCursor", "last", "before"),
    });

    json!({ "data": data, "meta": meta, "links": links })
}
