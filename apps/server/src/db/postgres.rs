//! PostgreSQL backend (sqlx, runtime query API — no compile-time DB needed).
//!
//! Tables are created on startup with `CREATE TABLE IF NOT EXISTS`, so a fresh
//! database needs no separate migration step. `public_id` (a v4 UUID string) is
//! the stable external id, exposed by the API as `id`; enum columns store the
//! `snake_case` serde form; `created_by` is JSONB.

use std::env;

use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};

use super::{enum_from_str, enum_to_str, parse_uuid};
use crate::{
    AllowedUser, ApiError, CreateAllowedUserRecord, CreateDeveloperRecord, CreateIssueRecord,
    DeveloperRecord, DeveloperView, Issue, IssueStatus, SeedIssueRecord, UpdateAllowedUserRecord,
    UpdateIssueStatusRecord, UpsertAllowedUserResponse, User,
};

#[derive(Clone)]
pub(crate) struct PgStore {
    pool: PgPool,
}

fn internal(_err: sqlx::Error) -> ApiError {
    ApiError::internal("database error")
}

/// Map a unique-constraint violation (SQLSTATE 23505) to a conflict; anything
/// else is an internal error. Used as a backstop behind the explicit pre-checks.
fn conflict_or_internal(err: sqlx::Error, message: &str) -> ApiError {
    if let sqlx::Error::Database(db_err) = &err {
        if db_err.code().as_deref() == Some("23505") {
            return ApiError::conflict(message);
        }
    }
    ApiError::internal("database error")
}

const ISSUE_COLUMNS: &str =
    "public_id, title, description, issue_type, status, priority, created_by, created_at, source_url";
const ALLOWED_USER_COLUMNS: &str =
    "public_id, external_user_id, email, name, role, created_at";
const DEVELOPER_COLUMNS: &str =
    "public_id, email, name, role, password, password_hash, created_at";

fn row_to_issue(row: &sqlx::postgres::PgRow) -> Result<Issue, ApiError> {
    Ok(Issue {
        id: parse_uuid(&row.try_get::<String, _>("public_id").map_err(internal)?)?,
        title: row.try_get("title").map_err(internal)?,
        description: row.try_get("description").map_err(internal)?,
        issue_type: enum_from_str(&row.try_get::<String, _>("issue_type").map_err(internal)?)?,
        status: enum_from_str(&row.try_get::<String, _>("status").map_err(internal)?)?,
        priority: enum_from_str(&row.try_get::<String, _>("priority").map_err(internal)?)?,
        created_by: row
            .try_get::<sqlx::types::Json<User>, _>("created_by")
            .map_err(internal)?
            .0,
        created_at: row.try_get("created_at").map_err(internal)?,
        source_url: row.try_get("source_url").map_err(internal)?,
    })
}

fn row_to_allowed_user(row: &sqlx::postgres::PgRow) -> Result<AllowedUser, ApiError> {
    Ok(AllowedUser {
        id: parse_uuid(&row.try_get::<String, _>("public_id").map_err(internal)?)?,
        external_user_id: row.try_get("external_user_id").map_err(internal)?,
        email: row.try_get("email").map_err(internal)?,
        name: row.try_get("name").map_err(internal)?,
        role: row.try_get("role").map_err(internal)?,
        created_at: row.try_get("created_at").map_err(internal)?,
    })
}

fn row_to_developer(row: &sqlx::postgres::PgRow) -> Result<DeveloperRecord, ApiError> {
    Ok(DeveloperRecord {
        id: parse_uuid(&row.try_get::<String, _>("public_id").map_err(internal)?)?,
        email: row.try_get("email").map_err(internal)?,
        name: row.try_get("name").map_err(internal)?,
        role: row.try_get("role").map_err(internal)?,
        password: row.try_get("password").map_err(internal)?,
        password_hash: row.try_get("password_hash").map_err(internal)?,
        created_at: row.try_get("created_at").map_err(internal)?,
    })
}

impl PgStore {
    pub(crate) async fn connect() -> anyhow::Result<Self> {
        let url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@127.0.0.1:5432/triagekit".to_string()
        });
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS developers (
                public_id     TEXT PRIMARY KEY,
                email         TEXT NOT NULL UNIQUE,
                name          TEXT NOT NULL,
                role          TEXT NOT NULL,
                password      TEXT,
                password_hash TEXT,
                created_at    TIMESTAMPTZ NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS allowed_users (
                public_id        TEXT PRIMARY KEY,
                external_user_id TEXT NOT NULL UNIQUE,
                email            TEXT NOT NULL UNIQUE,
                name             TEXT NOT NULL,
                role             TEXT NOT NULL,
                created_at       TIMESTAMPTZ NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS issues (
                public_id   TEXT PRIMARY KEY,
                title       TEXT NOT NULL,
                description TEXT NOT NULL,
                issue_type  TEXT NOT NULL,
                status      TEXT NOT NULL,
                priority    TEXT NOT NULL,
                created_by  JSONB NOT NULL,
                created_at  TIMESTAMPTZ NOT NULL,
                source_url  TEXT
            )",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub(crate) async fn bootstrap(
        &self,
        developer: CreateDeveloperRecord,
        allowed_user: CreateAllowedUserRecord,
        issue: SeedIssueRecord,
    ) -> Result<(), ApiError> {
        // Developer: insert if missing, else backfill a missing password hash.
        let existing = sqlx::query("SELECT password_hash FROM developers WHERE email = $1")
            .bind(&developer.email)
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?;
        match existing {
            None => {
                sqlx::query(
                    "INSERT INTO developers (public_id, email, name, role, password, password_hash, created_at)
                     VALUES ($1, $2, $3, $4, NULL, $5, $6)",
                )
                .bind(&developer.public_id)
                .bind(&developer.email)
                .bind(&developer.name)
                .bind(&developer.role)
                .bind(&developer.password_hash)
                .bind(developer.created_at)
                .execute(&self.pool)
                .await
                .map_err(internal)?;
            }
            Some(row) => {
                let password_hash: Option<String> =
                    row.try_get("password_hash").map_err(internal)?;
                if password_hash.is_none() {
                    sqlx::query("UPDATE developers SET password_hash = $1 WHERE email = $2")
                        .bind(&developer.password_hash)
                        .bind(&developer.email)
                        .execute(&self.pool)
                        .await
                        .map_err(internal)?;
                }
            }
        }

        // Allowed user: insert if missing.
        let allowed_exists = sqlx::query("SELECT 1 FROM allowed_users WHERE email = $1")
            .bind(&allowed_user.email)
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?;
        if allowed_exists.is_none() {
            sqlx::query(&format!(
                "INSERT INTO allowed_users ({ALLOWED_USER_COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6)"
            ))
            .bind(&allowed_user.public_id)
            .bind(&allowed_user.external_user_id)
            .bind(&allowed_user.email)
            .bind(&allowed_user.name)
            .bind(&allowed_user.role)
            .bind(allowed_user.created_at)
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        }

        // Sample issue: only when the table is empty.
        let count: i64 = sqlx::query("SELECT COUNT(*) AS n FROM issues")
            .fetch_one(&self.pool)
            .await
            .map_err(internal)?
            .try_get("n")
            .map_err(internal)?;
        if count == 0 {
            self.insert_issue(
                &issue.public_id,
                &issue.title,
                &issue.description,
                &enum_to_str(&issue.issue_type)?,
                &enum_to_str(&issue.status)?,
                &enum_to_str(&issue.priority)?,
                &issue.created_by,
                issue.created_at,
                issue.source_url.as_deref(),
            )
            .await?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_issue(
        &self,
        public_id: &str,
        title: &str,
        description: &str,
        issue_type: &str,
        status: &str,
        priority: &str,
        created_by: &User,
        created_at: DateTime<Utc>,
        source_url: Option<&str>,
    ) -> Result<(), ApiError> {
        sqlx::query(&format!(
            "INSERT INTO issues ({ISSUE_COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        ))
        .bind(public_id)
        .bind(title)
        .bind(description)
        .bind(issue_type)
        .bind(status)
        .bind(priority)
        .bind(sqlx::types::Json(created_by))
        .bind(created_at)
        .bind(source_url)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    pub(crate) async fn list_issues(&self) -> Result<Vec<Issue>, ApiError> {
        let rows = sqlx::query(&format!(
            "SELECT {ISSUE_COLUMNS} FROM issues ORDER BY created_at ASC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        rows.iter().map(row_to_issue).collect()
    }

    pub(crate) async fn create_issue(&self, record: CreateIssueRecord) -> Result<Issue, ApiError> {
        let issue_type = enum_to_str(&record.issue_type)?;
        let status = enum_to_str(&IssueStatus::New)?;
        let priority = enum_to_str(&record.priority)?;

        self.insert_issue(
            &record.public_id,
            &record.title,
            &record.description,
            &issue_type,
            &status,
            &priority,
            &record.created_by,
            record.created_at,
            record.source_url.as_deref(),
        )
        .await?;

        Ok(Issue {
            id: parse_uuid(&record.public_id)?,
            title: record.title,
            description: record.description,
            issue_type: record.issue_type,
            status: IssueStatus::New,
            priority: record.priority,
            created_by: record.created_by,
            created_at: record.created_at,
            source_url: record.source_url,
        })
    }

    pub(crate) async fn update_issue_status(
        &self,
        record: UpdateIssueStatusRecord,
    ) -> Result<Issue, ApiError> {
        let status = enum_to_str(&record.status)?;
        let result = sqlx::query("UPDATE issues SET status = $1 WHERE public_id = $2")
            .bind(&status)
            .bind(&record.public_id)
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        if result.rows_affected() == 0 {
            return Err(ApiError::not_found("issue not found"));
        }

        let row = sqlx::query(&format!(
            "SELECT {ISSUE_COLUMNS} FROM issues WHERE public_id = $1"
        ))
        .bind(&record.public_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::not_found("issue not found"))?;
        row_to_issue(&row)
    }

    pub(crate) async fn list_developer_records(&self) -> Result<Vec<DeveloperRecord>, ApiError> {
        let rows = sqlx::query(&format!(
            "SELECT {DEVELOPER_COLUMNS} FROM developers ORDER BY created_at ASC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        rows.iter().map(row_to_developer).collect()
    }

    pub(crate) async fn create_developer(
        &self,
        record: CreateDeveloperRecord,
    ) -> Result<DeveloperView, ApiError> {
        let existing = sqlx::query("SELECT 1 FROM developers WHERE email = $1")
            .bind(&record.email)
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?;
        if existing.is_some() {
            return Err(ApiError::conflict("developer already exists"));
        }

        sqlx::query(
            "INSERT INTO developers (public_id, email, name, role, password, password_hash, created_at)
             VALUES ($1, $2, $3, $4, NULL, $5, $6)",
        )
        .bind(&record.public_id)
        .bind(&record.email)
        .bind(&record.name)
        .bind(&record.role)
        .bind(&record.password_hash)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|err| conflict_or_internal(err, "developer already exists"))?;

        Ok(DeveloperView {
            id: parse_uuid(&record.public_id)?,
            email: record.email,
            name: record.name,
            role: record.role,
            created_at: record.created_at,
        })
    }

    pub(crate) async fn list_allowed_users(&self) -> Result<Vec<AllowedUser>, ApiError> {
        let rows = sqlx::query(&format!(
            "SELECT {ALLOWED_USER_COLUMNS} FROM allowed_users ORDER BY created_at ASC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        rows.iter().map(row_to_allowed_user).collect()
    }

    pub(crate) async fn create_allowed_user(
        &self,
        record: CreateAllowedUserRecord,
    ) -> Result<AllowedUser, ApiError> {
        if self.email_taken(&record.email).await? {
            return Err(ApiError::conflict("user already exists"));
        }
        if self.external_id_taken(&record.external_user_id).await? {
            return Err(ApiError::conflict("user already exists"));
        }

        self.insert_allowed_user(&record).await?;
        Ok(AllowedUser {
            id: parse_uuid(&record.public_id)?,
            external_user_id: record.external_user_id,
            email: record.email,
            name: record.name,
            role: record.role,
            created_at: record.created_at,
        })
    }

    pub(crate) async fn upsert_allowed_user(
        &self,
        record: CreateAllowedUserRecord,
    ) -> Result<UpsertAllowedUserResponse, ApiError> {
        // Reject if the email is already taken by a *different* external user.
        let email_owner =
            sqlx::query("SELECT external_user_id FROM allowed_users WHERE email = $1")
                .bind(&record.email)
                .fetch_optional(&self.pool)
                .await
                .map_err(internal)?;
        if let Some(row) = &email_owner {
            let owner_external: String = row.try_get("external_user_id").map_err(internal)?;
            if owner_external != record.external_user_id {
                return Err(ApiError::conflict("email already in use"));
            }
        }

        // Update in place when the external user already exists.
        let existing =
            sqlx::query("SELECT public_id, created_at FROM allowed_users WHERE external_user_id = $1")
                .bind(&record.external_user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(internal)?;
        if let Some(row) = existing {
            let public_id: String = row.try_get("public_id").map_err(internal)?;
            let created_at: DateTime<Utc> = row.try_get("created_at").map_err(internal)?;

            sqlx::query(
                "UPDATE allowed_users SET email = $1, name = $2, role = $3 WHERE external_user_id = $4",
            )
            .bind(&record.email)
            .bind(&record.name)
            .bind(&record.role)
            .bind(&record.external_user_id)
            .execute(&self.pool)
            .await
            .map_err(|err| conflict_or_internal(err, "email already in use"))?;

            return Ok(UpsertAllowedUserResponse {
                user: AllowedUser {
                    id: parse_uuid(&public_id)?,
                    external_user_id: record.external_user_id,
                    email: record.email,
                    name: record.name,
                    role: record.role,
                    created_at,
                },
                action: "updated".to_string(),
            });
        }

        self.insert_allowed_user(&record).await?;
        Ok(UpsertAllowedUserResponse {
            user: AllowedUser {
                id: parse_uuid(&record.public_id)?,
                external_user_id: record.external_user_id,
                email: record.email,
                name: record.name,
                role: record.role,
                created_at: record.created_at,
            },
            action: "created".to_string(),
        })
    }

    pub(crate) async fn update_allowed_user(
        &self,
        record: UpdateAllowedUserRecord,
    ) -> Result<AllowedUser, ApiError> {
        let existing = sqlx::query("SELECT public_id FROM allowed_users WHERE external_user_id = $1")
            .bind(&record.external_user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?
            .ok_or_else(|| ApiError::not_found("user not found"))?;
        let public_id: String = existing.try_get("public_id").map_err(internal)?;

        if let Some(email) = &record.email {
            let owner = sqlx::query("SELECT public_id FROM allowed_users WHERE email = $1")
                .bind(email)
                .fetch_optional(&self.pool)
                .await
                .map_err(internal)?;
            if let Some(row) = owner {
                let owner_public_id: String = row.try_get("public_id").map_err(internal)?;
                if owner_public_id != public_id {
                    return Err(ApiError::conflict("email already in use"));
                }
            }
        }

        // Patch only the provided fields atomically. COALESCE re-reads the
        // untouched columns under the row lock the UPDATE takes, so two
        // concurrent disjoint-field patches don't clobber each other (matches
        // Mongo's field-scoped $set). RETURNING gives the true persisted state.
        let row = sqlx::query(&format!(
            "UPDATE allowed_users
                SET email = COALESCE($1, email),
                    name = COALESCE($2, name),
                    role = COALESCE($3, role)
              WHERE external_user_id = $4
              RETURNING {ALLOWED_USER_COLUMNS}"
        ))
        .bind(record.email.as_deref())
        .bind(record.name.as_deref())
        .bind(record.role.as_deref())
        .bind(&record.external_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| conflict_or_internal(err, "email already in use"))?
        .ok_or_else(|| ApiError::not_found("user not found"))?;

        row_to_allowed_user(&row)
    }

    pub(crate) async fn delete_allowed_user(&self, external_user_id: &str) -> Result<(), ApiError> {
        let result = sqlx::query("DELETE FROM allowed_users WHERE external_user_id = $1")
            .bind(external_user_id)
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        if result.rows_affected() == 0 {
            return Err(ApiError::not_found("user not found"));
        }
        Ok(())
    }

    // ── small internal helpers ──────────────────────────────────────────────

    async fn email_taken(&self, email: &str) -> Result<bool, ApiError> {
        Ok(sqlx::query("SELECT 1 FROM allowed_users WHERE email = $1")
            .bind(email)
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?
            .is_some())
    }

    async fn external_id_taken(&self, external_user_id: &str) -> Result<bool, ApiError> {
        Ok(
            sqlx::query("SELECT 1 FROM allowed_users WHERE external_user_id = $1")
                .bind(external_user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(internal)?
                .is_some(),
        )
    }

    async fn insert_allowed_user(&self, record: &CreateAllowedUserRecord) -> Result<(), ApiError> {
        sqlx::query(&format!(
            "INSERT INTO allowed_users ({ALLOWED_USER_COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6)"
        ))
        .bind(&record.public_id)
        .bind(&record.external_user_id)
        .bind(&record.email)
        .bind(&record.name)
        .bind(&record.role)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|err| conflict_or_internal(err, "user already exists"))?;
        Ok(())
    }
}
