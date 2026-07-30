//! MongoDB backend.
//!
//! Documents are stored with plain field names (`public_id`, `external_user_id`,
//! ...) and RFC 3339 string timestamps, so no BSON date/uuid feature flags are
//! needed. `publicId` (a v4 UUID string) is the stable external id; the API
//! exposes it as `id`.

use std::env;

use futures::stream::TryStreamExt;
use mongodb::{
    Client, Collection,
    bson::doc,
    options::{IndexOptions},
    IndexModel,
};
use serde::{Deserialize, Serialize};

use super::{enum_from_str, enum_to_str, parse_time, parse_uuid};
use crate::{
    AllowedUser, ApiError, CreateAllowedUserRecord, CreateDeveloperRecord, CreateIssueRecord,
    DeveloperRecord, DeveloperView, Issue, IssueStatus, SeedIssueRecord, UpdateAllowedUserRecord,
    UpdateIssueStatusRecord, UpsertAllowedUserResponse, User,
};

#[derive(Clone)]
pub(crate) struct MongoStore {
    developers: Collection<DeveloperDoc>,
    allowed_users: Collection<AllowedUserDoc>,
    issues: Collection<IssueDoc>,
}

// ── Stored document shapes ───────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct DeveloperDoc {
    public_id: String,
    email: String,
    name: String,
    role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password_hash: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AllowedUserDoc {
    public_id: String,
    external_user_id: String,
    email: String,
    name: String,
    role: String,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct IssueDoc {
    public_id: String,
    title: String,
    description: String,
    issue_type: String,
    status: String,
    priority: String,
    created_by: User,
    created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_url: Option<String>,
}

impl DeveloperDoc {
    fn from_create(record: &CreateDeveloperRecord) -> Self {
        Self {
            public_id: record.public_id.clone(),
            email: record.email.clone(),
            name: record.name.clone(),
            role: record.role.clone(),
            password: None,
            password_hash: Some(record.password_hash.clone()),
            created_at: record.created_at.to_rfc3339(),
        }
    }

    fn into_record(self) -> Result<DeveloperRecord, ApiError> {
        Ok(DeveloperRecord {
            id: parse_uuid(&self.public_id)?,
            email: self.email,
            name: self.name,
            role: self.role,
            password: self.password,
            password_hash: self.password_hash,
            created_at: parse_time(&self.created_at)?,
        })
    }
}

impl AllowedUserDoc {
    fn from_create(record: &CreateAllowedUserRecord) -> Self {
        Self {
            public_id: record.public_id.clone(),
            external_user_id: record.external_user_id.clone(),
            email: record.email.clone(),
            name: record.name.clone(),
            role: record.role.clone(),
            created_at: record.created_at.to_rfc3339(),
        }
    }

    fn into_domain(self) -> Result<AllowedUser, ApiError> {
        Ok(AllowedUser {
            id: parse_uuid(&self.public_id)?,
            external_user_id: self.external_user_id,
            email: self.email,
            name: self.name,
            role: self.role,
            created_at: parse_time(&self.created_at)?,
        })
    }
}

impl IssueDoc {
    fn into_domain(self) -> Result<Issue, ApiError> {
        Ok(Issue {
            id: parse_uuid(&self.public_id)?,
            title: self.title,
            description: self.description,
            issue_type: enum_from_str(&self.issue_type)?,
            status: enum_from_str(&self.status)?,
            priority: enum_from_str(&self.priority)?,
            created_by: self.created_by,
            created_at: parse_time(&self.created_at)?,
            source_url: self.source_url,
        })
    }
}

fn internal(_err: mongodb::error::Error) -> ApiError {
    ApiError::internal("database error")
}

/// Map a duplicate-key rejection (Mongo error 11000, e.g. losing a
/// check-then-insert race against a unique index) to a 409 conflict; anything
/// else is an internal error. Mirrors the Postgres backend's `conflict_or_internal`
/// so both return the same status under a concurrent unique-constraint violation.
fn conflict_or_internal(err: mongodb::error::Error, message: &str) -> ApiError {
    use mongodb::error::{ErrorKind, WriteFailure};
    let duplicate_key = match err.kind.as_ref() {
        ErrorKind::Write(WriteFailure::WriteError(write_error)) => write_error.code == 11000,
        _ => false,
    };
    if duplicate_key {
        ApiError::conflict(message)
    } else {
        ApiError::internal("database error")
    }
}

impl MongoStore {
    pub(crate) async fn connect() -> anyhow::Result<Self> {
        let uri =
            env::var("MONGO_URI").unwrap_or_else(|_| "mongodb://127.0.0.1:27017".to_string());
        let db_name = env::var("MONGO_DB").unwrap_or_else(|_| "triagekit".to_string());

        let client = Client::with_uri_str(&uri).await?;
        let db = client.database(&db_name);
        let store = Self {
            developers: db.collection("developers"),
            allowed_users: db.collection("allowed_users"),
            issues: db.collection("issues"),
        };
        store.ensure_indexes().await?;
        Ok(store)
    }

    async fn ensure_indexes(&self) -> anyhow::Result<()> {
        let unique = || IndexOptions::builder().unique(true).build();
        let key = |field: &str| {
            IndexModel::builder()
                .keys(doc! { field: 1 })
                .options(unique())
                .build()
        };

        self.developers.create_index(key("public_id")).await?;
        self.developers.create_index(key("email")).await?;
        self.allowed_users.create_index(key("public_id")).await?;
        self.allowed_users.create_index(key("external_user_id")).await?;
        self.allowed_users.create_index(key("email")).await?;
        self.issues.create_index(key("public_id")).await?;
        Ok(())
    }

    pub(crate) async fn bootstrap(
        &self,
        developer: CreateDeveloperRecord,
        allowed_user: CreateAllowedUserRecord,
        issue: SeedIssueRecord,
    ) -> Result<(), ApiError> {
        // Developer: insert if missing, else backfill a missing password hash.
        match self
            .developers
            .find_one(doc! { "email": &developer.email })
            .await
            .map_err(internal)?
        {
            None => {
                self.developers
                    .insert_one(DeveloperDoc::from_create(&developer))
                    .await
                    .map_err(internal)?;
            }
            Some(existing) if existing.password_hash.is_none() => {
                self.developers
                    .update_one(
                        doc! { "email": &developer.email },
                        doc! { "$set": { "password_hash": &developer.password_hash } },
                    )
                    .await
                    .map_err(internal)?;
            }
            Some(_) => {}
        }

        // Allowed user: insert if missing.
        if self
            .allowed_users
            .find_one(doc! { "email": &allowed_user.email })
            .await
            .map_err(internal)?
            .is_none()
        {
            self.allowed_users
                .insert_one(AllowedUserDoc::from_create(&allowed_user))
                .await
                .map_err(internal)?;
        }

        // Sample issue: only when the collection is empty.
        if self.issues.count_documents(doc! {}).await.map_err(internal)? == 0 {
            let doc = IssueDoc {
                public_id: issue.public_id,
                title: issue.title,
                description: issue.description,
                issue_type: enum_to_str(&issue.issue_type)?,
                status: enum_to_str(&issue.status)?,
                priority: enum_to_str(&issue.priority)?,
                created_by: issue.created_by,
                created_at: issue.created_at.to_rfc3339(),
                source_url: issue.source_url,
            };
            self.issues.insert_one(doc).await.map_err(internal)?;
        }

        Ok(())
    }

    pub(crate) async fn list_issues(&self) -> Result<Vec<Issue>, ApiError> {
        let cursor = self
            .issues
            .find(doc! {})
            .sort(doc! { "created_at": 1 })
            .await
            .map_err(internal)?;
        let docs: Vec<IssueDoc> = cursor.try_collect().await.map_err(internal)?;
        docs.into_iter().map(IssueDoc::into_domain).collect()
    }

    pub(crate) async fn create_issue(&self, record: CreateIssueRecord) -> Result<Issue, ApiError> {
        let doc = IssueDoc {
            public_id: record.public_id,
            title: record.title,
            description: record.description,
            issue_type: enum_to_str(&record.issue_type)?,
            status: enum_to_str(&IssueStatus::New)?,
            priority: enum_to_str(&record.priority)?,
            created_by: record.created_by,
            created_at: record.created_at.to_rfc3339(),
            source_url: record.source_url,
        };
        self.issues.insert_one(&doc).await.map_err(internal)?;
        doc.into_domain()
    }

    pub(crate) async fn update_issue_status(
        &self,
        record: UpdateIssueStatusRecord,
    ) -> Result<Issue, ApiError> {
        let status = enum_to_str(&record.status)?;
        let result = self
            .issues
            .update_one(
                doc! { "public_id": &record.public_id },
                doc! { "$set": { "status": &status } },
            )
            .await
            .map_err(internal)?;
        if result.matched_count == 0 {
            return Err(ApiError::not_found("issue not found"));
        }

        self.issues
            .find_one(doc! { "public_id": &record.public_id })
            .await
            .map_err(internal)?
            .ok_or_else(|| ApiError::not_found("issue not found"))?
            .into_domain()
    }

    pub(crate) async fn list_developer_records(&self) -> Result<Vec<DeveloperRecord>, ApiError> {
        let cursor = self
            .developers
            .find(doc! {})
            .sort(doc! { "created_at": 1 })
            .await
            .map_err(internal)?;
        let docs: Vec<DeveloperDoc> = cursor.try_collect().await.map_err(internal)?;
        docs.into_iter().map(DeveloperDoc::into_record).collect()
    }

    pub(crate) async fn create_developer(
        &self,
        record: CreateDeveloperRecord,
    ) -> Result<DeveloperView, ApiError> {
        if self
            .developers
            .find_one(doc! { "email": &record.email })
            .await
            .map_err(internal)?
            .is_some()
        {
            return Err(ApiError::conflict("developer already exists"));
        }

        self.developers
            .insert_one(DeveloperDoc::from_create(&record))
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
        let cursor = self
            .allowed_users
            .find(doc! {})
            .sort(doc! { "created_at": 1 })
            .await
            .map_err(internal)?;
        let docs: Vec<AllowedUserDoc> = cursor.try_collect().await.map_err(internal)?;
        docs.into_iter().map(AllowedUserDoc::into_domain).collect()
    }

    pub(crate) async fn create_allowed_user(
        &self,
        record: CreateAllowedUserRecord,
    ) -> Result<AllowedUser, ApiError> {
        if self
            .allowed_users
            .find_one(doc! { "email": &record.email })
            .await
            .map_err(internal)?
            .is_some()
        {
            return Err(ApiError::conflict("user already exists"));
        }
        if self
            .allowed_users
            .find_one(doc! { "external_user_id": &record.external_user_id })
            .await
            .map_err(internal)?
            .is_some()
        {
            return Err(ApiError::conflict("user already exists"));
        }

        let doc = AllowedUserDoc::from_create(&record);
        self.allowed_users
            .insert_one(&doc)
            .await
            .map_err(|err| conflict_or_internal(err, "user already exists"))?;
        doc.into_domain()
    }

    pub(crate) async fn upsert_allowed_user(
        &self,
        record: CreateAllowedUserRecord,
    ) -> Result<UpsertAllowedUserResponse, ApiError> {
        // Reject if the email is already taken by a *different* external user.
        if let Some(owner) = self
            .allowed_users
            .find_one(doc! { "email": &record.email })
            .await
            .map_err(internal)?
        {
            if owner.external_user_id != record.external_user_id {
                return Err(ApiError::conflict("email already in use"));
            }
        }

        // Update in place when the external user already exists.
        if let Some(existing) = self
            .allowed_users
            .find_one(doc! { "external_user_id": &record.external_user_id })
            .await
            .map_err(internal)?
        {
            self.allowed_users
                .update_one(
                    doc! { "external_user_id": &record.external_user_id },
                    doc! { "$set": {
                        "email": &record.email,
                        "name": &record.name,
                        "role": &record.role,
                    } },
                )
                .await
                .map_err(|err| conflict_or_internal(err, "email already in use"))?;

            let user = AllowedUser {
                id: parse_uuid(&existing.public_id)?,
                external_user_id: existing.external_user_id,
                email: record.email,
                name: record.name,
                role: record.role,
                created_at: parse_time(&existing.created_at)?,
            };
            return Ok(UpsertAllowedUserResponse {
                user,
                action: "updated".to_string(),
            });
        }

        let doc = AllowedUserDoc::from_create(&record);
        self.allowed_users
            .insert_one(&doc)
            .await
            .map_err(|err| conflict_or_internal(err, "user already exists"))?;
        Ok(UpsertAllowedUserResponse {
            user: doc.into_domain()?,
            action: "created".to_string(),
        })
    }

    pub(crate) async fn update_allowed_user(
        &self,
        record: UpdateAllowedUserRecord,
    ) -> Result<AllowedUser, ApiError> {
        let existing = self
            .allowed_users
            .find_one(doc! { "external_user_id": &record.external_user_id })
            .await
            .map_err(internal)?
            .ok_or_else(|| ApiError::not_found("user not found"))?;

        if let Some(email) = &record.email {
            if let Some(owner) = self
                .allowed_users
                .find_one(doc! { "email": email })
                .await
                .map_err(internal)?
            {
                if owner.public_id != existing.public_id {
                    return Err(ApiError::conflict("email already in use"));
                }
            }
        }

        let mut set = doc! {};
        if let Some(email) = &record.email {
            set.insert("email", email);
        }
        if let Some(name) = &record.name {
            set.insert("name", name);
        }
        if let Some(role) = &record.role {
            set.insert("role", role);
        }
        if !set.is_empty() {
            self.allowed_users
                .update_one(
                    doc! { "external_user_id": &record.external_user_id },
                    doc! { "$set": set },
                )
                .await
                .map_err(|err| conflict_or_internal(err, "email already in use"))?;
        }

        Ok(AllowedUser {
            id: parse_uuid(&existing.public_id)?,
            created_at: parse_time(&existing.created_at)?,
            external_user_id: existing.external_user_id,
            email: record.email.unwrap_or(existing.email),
            name: record.name.unwrap_or(existing.name),
            role: record.role.unwrap_or(existing.role),
        })
    }

    pub(crate) async fn delete_allowed_user(&self, external_user_id: &str) -> Result<(), ApiError> {
        let result = self
            .allowed_users
            .delete_one(doc! { "external_user_id": external_user_id })
            .await
            .map_err(internal)?;
        if result.deleted_count == 0 {
            return Err(ApiError::not_found("user not found"));
        }
        Ok(())
    }
}
