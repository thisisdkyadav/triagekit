//! Persistence layer for TriageKit.
//!
//! All storage goes through the [`Db`] enum, which dispatches to one of two
//! interchangeable backends selected at startup by the `DB_BACKEND` env var:
//!
//! - `mongo` (default) -> [`mongo::MongoStore`]
//! - `postgres`        -> [`postgres::PgStore`]
//!
//! Both backends implement the same method set and reproduce the same domain
//! semantics (idempotent bootstrap, conflict / not-found errors, upsert keyed
//! by `externalUserId`). When adding a method, implement it for BOTH backends
//! and add the dispatch arm here.

use std::env;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    AllowedUser, ApiError, CreateAllowedUserRecord, CreateDeveloperRecord, CreateIssueRecord,
    DeveloperRecord, DeveloperView, Issue, UpdateAllowedUserRecord, UpdateIssueStatusRecord,
    UpsertAllowedUserResponse,
};

mod mongo;
mod postgres;

use mongo::MongoStore;
use postgres::PgStore;

/// A database backend. Cheap to clone (each variant wraps an internally
/// reference-counted pool / client), so it lives directly in `AppState`.
#[derive(Clone)]
pub(crate) enum Db {
    Mongo(MongoStore),
    Postgres(PgStore),
}

impl Db {
    /// Connect to the backend named by `DB_BACKEND` (default `mongo`) and make
    /// sure its schema / indexes exist.
    pub(crate) async fn from_env() -> anyhow::Result<Self> {
        let backend = env::var("DB_BACKEND").unwrap_or_else(|_| "mongo".to_string());
        match backend.trim().to_lowercase().as_str() {
            "mongo" | "mongodb" => Ok(Db::Mongo(MongoStore::connect().await?)),
            "postgres" | "postgresql" | "pg" => Ok(Db::Postgres(PgStore::connect().await?)),
            other => anyhow::bail!(
                "unsupported DB_BACKEND '{other}' (expected 'mongo' or 'postgres')"
            ),
        }
    }

    /// Idempotently seed the first owner developer, one allowed user, and a
    /// sample issue. Safe to run on every startup.
    pub(crate) async fn bootstrap(&self) -> Result<(), ApiError> {
        let developer = crate::seed_developer_record();
        let allowed_user = crate::seed_allowed_user_record();
        let issue = crate::seed_issue_record();
        match self {
            Db::Mongo(s) => s.bootstrap(developer, allowed_user, issue).await,
            Db::Postgres(s) => s.bootstrap(developer, allowed_user, issue).await,
        }
    }

    pub(crate) async fn list_issues(&self) -> Result<Vec<Issue>, ApiError> {
        match self {
            Db::Mongo(s) => s.list_issues().await,
            Db::Postgres(s) => s.list_issues().await,
        }
    }

    pub(crate) async fn create_issue(&self, record: CreateIssueRecord) -> Result<Issue, ApiError> {
        match self {
            Db::Mongo(s) => s.create_issue(record).await,
            Db::Postgres(s) => s.create_issue(record).await,
        }
    }

    pub(crate) async fn update_issue_status(
        &self,
        record: UpdateIssueStatusRecord,
    ) -> Result<Issue, ApiError> {
        match self {
            Db::Mongo(s) => s.update_issue_status(record).await,
            Db::Postgres(s) => s.update_issue_status(record).await,
        }
    }

    pub(crate) async fn list_developer_records(&self) -> Result<Vec<DeveloperRecord>, ApiError> {
        match self {
            Db::Mongo(s) => s.list_developer_records().await,
            Db::Postgres(s) => s.list_developer_records().await,
        }
    }

    /// Developers without the credential fields, for admin listing.
    pub(crate) async fn list_developers(&self) -> Result<Vec<DeveloperView>, ApiError> {
        Ok(self
            .list_developer_records()
            .await?
            .into_iter()
            .map(DeveloperView::from)
            .collect())
    }

    pub(crate) async fn create_developer(
        &self,
        record: CreateDeveloperRecord,
    ) -> Result<DeveloperView, ApiError> {
        match self {
            Db::Mongo(s) => s.create_developer(record).await,
            Db::Postgres(s) => s.create_developer(record).await,
        }
    }

    pub(crate) async fn list_allowed_users(&self) -> Result<Vec<AllowedUser>, ApiError> {
        match self {
            Db::Mongo(s) => s.list_allowed_users().await,
            Db::Postgres(s) => s.list_allowed_users().await,
        }
    }

    pub(crate) async fn create_allowed_user(
        &self,
        record: CreateAllowedUserRecord,
    ) -> Result<AllowedUser, ApiError> {
        match self {
            Db::Mongo(s) => s.create_allowed_user(record).await,
            Db::Postgres(s) => s.create_allowed_user(record).await,
        }
    }

    pub(crate) async fn upsert_allowed_user(
        &self,
        record: CreateAllowedUserRecord,
    ) -> Result<UpsertAllowedUserResponse, ApiError> {
        match self {
            Db::Mongo(s) => s.upsert_allowed_user(record).await,
            Db::Postgres(s) => s.upsert_allowed_user(record).await,
        }
    }

    pub(crate) async fn update_allowed_user(
        &self,
        record: UpdateAllowedUserRecord,
    ) -> Result<AllowedUser, ApiError> {
        match self {
            Db::Mongo(s) => s.update_allowed_user(record).await,
            Db::Postgres(s) => s.update_allowed_user(record).await,
        }
    }

    pub(crate) async fn delete_allowed_user(&self, external_user_id: &str) -> Result<(), ApiError> {
        match self {
            Db::Mongo(s) => s.delete_allowed_user(external_user_id).await,
            Db::Postgres(s) => s.delete_allowed_user(external_user_id).await,
        }
    }
}

// ── Shared conversion helpers used by both backends ──────────────────────────

/// Encode a `#[serde(rename_all = "snake_case")]` enum to its stored string
/// form (e.g. `IssueStatus::InProgress` -> `"in_progress"`).
pub(crate) fn enum_to_str<T: serde::Serialize>(value: &T) -> Result<String, ApiError> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => Ok(s),
        _ => Err(ApiError::internal("failed to encode enum value")),
    }
}

/// Decode a stored enum string back to its Rust variant.
pub(crate) fn enum_from_str<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, ApiError> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| ApiError::internal("invalid enum value stored in database"))
}

/// Parse a stored `publicId` string into the `Uuid` the API exposes as `id`.
pub(crate) fn parse_uuid(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::internal("invalid id stored in database"))
}

/// Parse an RFC 3339 timestamp string (how Mongo stores `createdAt`).
pub(crate) fn parse_time(value: &str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| ApiError::internal("invalid timestamp stored in database"))
}
