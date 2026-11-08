use rusqlite::Connection;

use crate::error::KgError;
use crate::fields::Fields;
use crate::query::{FindQuery, Found, keep, select_nodes};
use crate::status::{Reason, Status};
use crate::store;

fn has_index(conn: &Connection) -> Result<bool, KgError> {
    Ok(store::count_rows(conn, "refs")? > 0)
}

pub(crate) fn by_law_key(
    conn: &Connection,
    key: &str,
    q: &FindQuery<'_>,
    fields: &Fields,
) -> Result<Found, KgError> {
    if !has_index(conn)? {
        return Ok(Found::empty(Status::OutOfScope, Reason::NoArticleIndex));
    }
    let hits = select_nodes(
        conn,
        "SELECT DISTINCT n.id, n.kind, n.name, n.data FROM refs r JOIN nodes n ON n.id = r.node_id \
         WHERE r.article_id = ?1 AND (?2 IS NULL OR n.kind = ?2) ORDER BY n.id LIMIT ?3",
        key,
        q,
        fields,
    )?;
    keep(hits, q, fields)
}

pub(crate) fn by_article(conn: &Connection) -> Result<Found, KgError> {
    if !has_index(conn)? {
        return Ok(Found::empty(Status::OutOfScope, Reason::NoArticleIndex));
    }
    Ok(Found::empty(Status::Unknown, Reason::ArticleKeyUnresolved))
}
