//! Schema 8 revision counters shared by every Hub-visible write.
//!
//! `library_change_seq` gives the Hub a monotonic total order so a watch
//! consumer can tell "nothing happened" from "a write I did not see".
//! `library_domain_revisions` lets a query re-validate only the domains it
//! actually reads, so a Job transition never invalidates a pure folder view.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) type LibraryRevisionResult<T> = Result<T, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum LibraryDomain {
    Structure,
    Tags,
    Lifecycle,
    Engagement,
    Artifacts,
    Jobs,
    SmartCollections,
    Sort,
}

pub(crate) const ALL_LIBRARY_DOMAINS: [LibraryDomain; 8] = [
    LibraryDomain::Structure,
    LibraryDomain::Tags,
    LibraryDomain::Lifecycle,
    LibraryDomain::Engagement,
    LibraryDomain::Artifacts,
    LibraryDomain::Jobs,
    LibraryDomain::SmartCollections,
    LibraryDomain::Sort,
];

impl LibraryDomain {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Structure => "structure",
            Self::Tags => "tags",
            Self::Lifecycle => "lifecycle",
            Self::Engagement => "engagement",
            Self::Artifacts => "artifacts",
            Self::Jobs => "jobs",
            Self::SmartCollections => "smart_collections",
            Self::Sort => "sort",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        ALL_LIBRARY_DOMAINS
            .into_iter()
            .find(|domain| domain.as_str() == value)
    }
}

/// A revision snapshot: the global sequence plus every domain counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LibraryRevisions {
    pub(crate) global: i64,
    pub(crate) domains: BTreeMap<LibraryDomain, i64>,
}

/// One `(domain, value)` entry of a dependency vector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainRevision {
    pub domain: String,
    pub value: i64,
}

impl LibraryRevisions {
    pub(crate) fn domain(&self, domain: LibraryDomain) -> i64 {
        self.domains.get(&domain).copied().unwrap_or(0)
    }

    fn sorted(domains: &[LibraryDomain]) -> Vec<LibraryDomain> {
        let mut wanted: Vec<LibraryDomain> = domains.to_vec();
        // Ordered by database name, never by enum declaration order: this string
        // is compared across the Rust / TypeScript boundary.
        wanted.sort_by_key(|domain| domain.as_str());
        wanted.dedup();
        wanted
    }

    /// Stable dependency vector for a query: only the listed domains, so a Job
    /// transition never invalidates a pure folder view.
    pub(crate) fn dependency_vector(&self, domains: &[LibraryDomain]) -> String {
        Self::sorted(domains)
            .into_iter()
            .map(|domain| format!("{}={}", domain.as_str(), self.domain(domain)))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// The same vector as typed entries, in the same order as
    /// [`LibraryRevisions::dependency_vector`].
    pub(crate) fn domain_revisions(&self, domains: &[LibraryDomain]) -> Vec<DomainRevision> {
        Self::sorted(domains)
            .into_iter()
            .map(|domain| DomainRevision {
                domain: domain.as_str().to_string(),
                value: self.domain(domain),
            })
            .collect()
    }
}

/// Advance the global sequence and the listed domain counters inside the
/// caller's transaction. A missing singleton row fails closed instead of
/// silently pretending the Hub stayed current.
pub(crate) fn bump_library_revisions(
    connection: &Connection,
    domains: &[LibraryDomain],
) -> LibraryRevisionResult<i64> {
    let updated = connection
        .execute(
            "UPDATE library_change_seq SET value = value + 1 WHERE singleton = 1",
            [],
        )
        .map_err(|error| error.to_string())?;
    if updated != 1 {
        return Err(
            "schema 8 revision counters are missing; library_change_seq must hold one row"
                .to_string(),
        );
    }
    let global: i64 = connection
        .query_row(
            "SELECT value FROM library_change_seq WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let mut wanted: Vec<LibraryDomain> = domains.to_vec();
    wanted.sort();
    wanted.dedup();
    for domain in wanted {
        connection
            .execute(
                "INSERT INTO library_domain_revisions(domain, value) VALUES (?1, 1)
                 ON CONFLICT(domain) DO UPDATE
                   SET value = library_domain_revisions.value + 1",
                params![domain.as_str()],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(global)
}

pub(crate) fn library_revision(connection: &Connection) -> LibraryRevisionResult<i64> {
    connection
        .query_row(
            "SELECT value FROM library_change_seq WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

pub(crate) fn library_revisions(
    connection: &Connection,
) -> LibraryRevisionResult<LibraryRevisions> {
    let global = library_revision(connection)?;
    let mut domains = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT domain, value FROM library_domain_revisions")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (name, value) = row.map_err(|error| error.to_string())?;
        let domain = LibraryDomain::parse(&name)
            .ok_or_else(|| format!("unknown library revision domain: {name}"))?;
        domains.insert(domain, value);
    }
    for domain in ALL_LIBRARY_DOMAINS {
        if !domains.contains_key(&domain) {
            return Err(format!(
                "library revision domain is missing: {}",
                domain.as_str()
            ));
        }
    }
    Ok(LibraryRevisions { global, domains })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::v2_workspace::WorkspaceModule;

    /// A fully initialized schema 8 workspace database.
    fn revision_connection() -> (tempfile::TempDir, Connection) {
        let directory = tempfile::tempdir().expect("tempdir");
        WorkspaceModule::new()
            .open(directory.path())
            .expect("initialize workspace");
        let connection = db::open(directory.path().join(".read-desktop/workspace.sqlite3"))
            .expect("open workspace database");
        (directory, connection)
    }

    #[test]
    fn bumping_moves_the_global_sequence_and_only_the_listed_domains() {
        let (_root, connection) = revision_connection();
        let before = library_revisions(&connection).expect("revisions");
        assert_eq!(before.global, 0);

        let first = bump_library_revisions(&connection, &[LibraryDomain::Structure]).expect("bump");
        assert_eq!(first, 1);
        let after = library_revisions(&connection).expect("revisions after");
        assert_eq!(after.domain(LibraryDomain::Structure), 1);
        assert_eq!(after.domain(LibraryDomain::Jobs), 0);
        assert_eq!(after.domain(LibraryDomain::Sort), 0);

        bump_library_revisions(
            &connection,
            &[
                LibraryDomain::Jobs,
                LibraryDomain::Jobs,
                LibraryDomain::Sort,
            ],
        )
        .expect("bump two domains at once");
        let merged = library_revisions(&connection).expect("revisions merged");
        assert_eq!(merged.global, 2);
        assert_eq!(merged.domain(LibraryDomain::Structure), 1);
        assert_eq!(merged.domain(LibraryDomain::Jobs), 1);
        assert_eq!(merged.domain(LibraryDomain::Sort), 1);
    }

    #[test]
    fn bump_rolls_back_with_the_writers_transaction() {
        let (_root, mut connection) = revision_connection();
        let before = library_revision(&connection).expect("revision");
        {
            let transaction = connection.transaction().expect("transaction");
            bump_library_revisions(&transaction, &[LibraryDomain::Structure]).expect("bump");
            assert_eq!(library_revision(&transaction).expect("in tx"), before + 1);
            transaction.rollback().expect("rollback");
        }
        assert_eq!(
            library_revision(&connection).expect("after rollback"),
            before
        );
    }

    #[test]
    fn bump_fails_closed_when_the_singleton_counter_row_is_missing() {
        let (_root, connection) = revision_connection();
        connection
            .execute("DELETE FROM library_change_seq", [])
            .expect("clear the singleton row");
        let error = bump_library_revisions(&connection, &[LibraryDomain::Structure])
            .expect_err("must fail");
        assert!(
            error.contains("schema 8 revision counters are missing"),
            "{error}"
        );
    }

    #[test]
    fn bump_fails_closed_on_a_pre_schema_8_database() {
        let (_root, connection) = revision_connection();
        connection
            .execute_batch("DROP TABLE library_change_seq;")
            .expect("drop the counter table");
        let error = bump_library_revisions(&connection, &[LibraryDomain::Structure])
            .expect_err("a workspace without the revision tables must never look current");
        assert!(error.contains("library_change_seq"), "{error}");
    }

    #[test]
    fn dependency_vector_lists_only_dependencies_in_a_stable_order() {
        let (_root, connection) = revision_connection();
        bump_library_revisions(&connection, &[LibraryDomain::Sort]).expect("bump sort");
        bump_library_revisions(&connection, &[LibraryDomain::Structure]).expect("bump structure");
        let revisions = library_revisions(&connection).expect("revisions");
        assert_eq!(
            revisions.dependency_vector(&[LibraryDomain::Structure, LibraryDomain::Sort]),
            // 按域名而非枚举声明顺序：`sort` 排在 `structure` 前。
            "sort=1;structure=1"
        );
        assert_eq!(
            revisions.dependency_vector(&[LibraryDomain::Jobs, LibraryDomain::Jobs]),
            "jobs=0"
        );
        assert_eq!(revisions.dependency_vector(&[]), "");
    }

    #[test]
    fn the_vector_lists_every_domain_in_database_name_order() {
        let (_root, connection) = revision_connection();
        bump_library_revisions(&connection, &[LibraryDomain::Sort]).expect("bump sort");
        let revisions = library_revisions(&connection).expect("revisions");
        assert_eq!(
            revisions.dependency_vector(&ALL_LIBRARY_DOMAINS),
            "artifacts=0;engagement=0;jobs=0;lifecycle=0;smart_collections=0;sort=1;structure=0;tags=0"
        );
        let typed = revisions.domain_revisions(&ALL_LIBRARY_DOMAINS);
        assert_eq!(typed.len(), ALL_LIBRARY_DOMAINS.len());
        assert_eq!(
            typed
                .iter()
                .map(|entry| entry.domain.as_str())
                .collect::<Vec<_>>(),
            vec![
                "artifacts",
                "engagement",
                "jobs",
                "lifecycle",
                "smart_collections",
                "sort",
                "structure",
                "tags"
            ]
        );
        assert_eq!(
            revisions.dependency_vector(&[LibraryDomain::Sort, LibraryDomain::Sort]),
            "sort=1",
            "a duplicated dependency is declared once"
        );
    }

    #[test]
    fn every_domain_round_trips_through_its_database_name() {
        for domain in ALL_LIBRARY_DOMAINS {
            let parsed = LibraryDomain::parse(domain.as_str());
            assert_eq!(parsed, Some(domain), "{}", domain.as_str());
        }
        assert_eq!(LibraryDomain::parse("papers"), None);
    }
}
