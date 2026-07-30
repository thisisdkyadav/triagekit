use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    routing::{get, patch, post},
};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use redis::Commands;
use serde::{Deserialize, Serialize};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

mod db;

#[derive(Clone)]
struct AppState {
    app_secret: String,
    jwt_secret: String,
    public_web_url: String,
    base_path: String,
    code_store: CodeStore,
    db: db::Db,
}

#[derive(Clone)]
enum CodeStore {
    Memory(Arc<Mutex<HashMap<String, LoginCode>>>),
    Redis {
        client: redis::Client,
        prefix: String,
        ttl_seconds: u64,
    },
}

#[derive(Clone, Serialize, Deserialize)]
struct LoginCode {
    user: User,
    return_to: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    external_user_id: String,
    email: String,
    name: String,
    role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeveloperRecord {
    id: Uuid,
    email: String,
    name: String,
    role: String,
    password: Option<String>,
    password_hash: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeveloperView {
    id: Uuid,
    email: String,
    name: String,
    role: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllowedUser {
    id: Uuid,
    external_user_id: String,
    email: String,
    name: String,
    role: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Issue {
    id: Uuid,
    title: String,
    description: String,
    issue_type: IssueType,
    status: IssueStatus,
    priority: Priority,
    created_by: User,
    created_at: DateTime<Utc>,
    source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum IssueType {
    Bug,
    Feature,
    Question,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum IssueStatus {
    New,
    Triaged,
    InProgress,
    Done,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Priority {
    Low,
    Medium,
    High,
    Urgent,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginCodeRequest {
    external_user_id: String,
    email: String,
    name: String,
    role: String,
    return_to: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginCodeResponse {
    redirect_url: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct ExchangeCodeRequest {
    code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeCodeResponse {
    token: String,
    user: User,
    return_to: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeveloperLoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    token: String,
    user: User,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateDeveloperRequest {
    email: String,
    name: String,
    password: String,
    role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAllowedUserRequest {
    external_user_id: Option<String>,
    email: String,
    name: String,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProvisionUserRequest {
    external_user_id: Option<String>,
    email: String,
    name: String,
    role: Option<String>,
    #[serde(default)]
    upsert: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkProvisionRequest {
    users: Vec<ProvisionUserRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserRequest {
    email: Option<String>,
    name: Option<String>,
    role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateIssueRequest {
    title: String,
    description: String,
    issue_type: IssueType,
    priority: Priority,
    source_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateIssueStatusRequest {
    status: IssueStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateIssueRecord {
    public_id: String,
    title: String,
    description: String,
    issue_type: IssueType,
    priority: Priority,
    created_by: User,
    created_at: DateTime<Utc>,
    source_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateDeveloperRecord {
    public_id: String,
    email: String,
    name: String,
    role: String,
    password_hash: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateAllowedUserRecord {
    public_id: String,
    external_user_id: String,
    email: String,
    name: String,
    role: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAllowedUserRecord {
    external_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertAllowedUserResponse {
    #[serde(flatten)]
    user: AllowedUser,
    action: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BulkProvisionResponse {
    created: usize,
    updated: usize,
    users: Vec<AllowedUser>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedIssueRecord {
    public_id: String,
    title: String,
    description: String,
    issue_type: IssueType,
    status: IssueStatus,
    priority: Priority,
    created_by: User,
    created_at: DateTime<Utc>,
    source_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateIssueStatusRecord {
    public_id: String,
    status: IssueStatus,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    email: String,
    name: String,
    role: String,
    exp: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::from_filename("../../.env").ok();
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter("triagekit_server=debug,tower_http=debug")
        .init();

    let host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    let store = db::Db::from_env().await?;
    store
        .bootstrap()
        .await
        .map_err(|err| anyhow::anyhow!(err.message))?;

    let state = AppState {
        app_secret: env::var("TRIAGEKIT_APP_SECRET")
            .unwrap_or_else(|_| "dev-app-secret".to_string()),
        jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret".to_string()),
        public_web_url: env::var("PUBLIC_WEB_URL")
            .unwrap_or_else(|_| "http://localhost:5173".to_string()),
        base_path: env::var("APP_BASE_PATH").unwrap_or_else(|_| "/issue-management".to_string()),
        code_store: build_code_store()?,
        db: store,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/developer/login", post(developer_login))
        .route(
            "/api/developers",
            get(list_developers).post(create_developer),
        )
        .route(
            "/api/allowed-users",
            get(list_allowed_users).post(create_allowed_user),
        )
        .route("/api/sso/users", post(provision_user))
        .route("/api/sso/users/bulk", post(provision_users_bulk))
        .route(
            "/api/sso/users/{external_user_id}",
            patch(update_user).delete(delete_user),
        )
        .route("/api/sso/login-code", post(create_login_code))
        .route("/api/sso/exchange", post(exchange_login_code))
        .route("/api/issues", get(list_issues).post(create_issue))
        .route("/api/issues/{id}/status", post(update_issue_status))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    tracing::info!("TriageKit API listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn list_allowed_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AllowedUser>>, ApiError> {
    require_developer(&state, &headers)?;
    Ok(Json(state.db.list_allowed_users().await?))
}

async fn create_allowed_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateAllowedUserRequest>,
) -> Result<Json<AllowedUser>, ApiError> {
    require_developer(&state, &headers)?;
    let record =
        build_allowed_user_record(payload.external_user_id, &payload.email, &payload.name, payload.role)?;
    Ok(Json(state.db.create_allowed_user(record).await?))
}

// Host-app provisioning: create one allowed user (server-to-server, app secret).
async fn provision_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ProvisionUserRequest>,
) -> Result<Json<AllowedUser>, ApiError> {
    require_app_secret(&state, &headers)?;
    let upsert = payload.upsert;
    let record =
        build_allowed_user_record(payload.external_user_id, &payload.email, &payload.name, payload.role)?;

    if upsert {
        Ok(Json(state.db.upsert_allowed_user(record).await?.user))
    } else {
        Ok(Json(state.db.create_allowed_user(record).await?))
    }
}

// Host-app provisioning: create-or-update many users in one call (roster sync).
async fn provision_users_bulk(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BulkProvisionRequest>,
) -> Result<Json<BulkProvisionResponse>, ApiError> {
    require_app_secret(&state, &headers)?;

    if payload.users.is_empty() {
        return Err(ApiError::bad_request("users array is required"));
    }
    if payload.users.len() > 1000 {
        return Err(ApiError::bad_request(
            "cannot provision more than 1000 users at once",
        ));
    }

    // Validate and normalize everything before touching the store.
    let mut records = Vec::with_capacity(payload.users.len());
    for user in payload.users {
        records.push(build_allowed_user_record(
            user.external_user_id,
            &user.email,
            &user.name,
            user.role,
        )?);
    }

    let mut created = 0;
    let mut updated = 0;
    let mut users = Vec::with_capacity(records.len());
    for record in records {
        let result = state.db.upsert_allowed_user(record).await?;
        if result.action == "created" {
            created += 1;
        } else {
            updated += 1;
        }
        users.push(result.user);
    }

    Ok(Json(BulkProvisionResponse {
        created,
        updated,
        users,
    }))
}

// Host-app provisioning: edit a user by externalUserId (partial update).
async fn update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(external_user_id): Path<String>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<Json<AllowedUser>, ApiError> {
    require_app_secret(&state, &headers)?;

    if payload.email.is_none() && payload.name.is_none() && payload.role.is_none() {
        return Err(ApiError::bad_request("no fields to update"));
    }

    let email = match payload.email {
        Some(email) if email.trim().is_empty() => {
            return Err(ApiError::bad_request("email cannot be empty"));
        }
        Some(email) => Some(normalize_email(&email)),
        None => None,
    };
    let name = match payload.name {
        Some(name) if name.trim().is_empty() => {
            return Err(ApiError::bad_request("name cannot be empty"));
        }
        Some(name) => Some(name.trim().to_string()),
        None => None,
    };
    let role = match payload.role {
        Some(role) if role.trim().is_empty() => {
            return Err(ApiError::bad_request("role cannot be empty"));
        }
        Some(role) => Some(role.trim().to_string()),
        None => None,
    };

    let record = UpdateAllowedUserRecord {
        external_user_id,
        email,
        name,
        role,
    };

    Ok(Json(state.db.update_allowed_user(record).await?))
}

// Host-app provisioning: remove a user by externalUserId.
async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(external_user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_app_secret(&state, &headers)?;
    state.db.delete_allowed_user(&external_user_id).await?;
    Ok(Json(
        serde_json::json!({ "ok": true, "externalUserId": external_user_id }),
    ))
}

fn build_allowed_user_record(
    external_user_id: Option<String>,
    email: &str,
    name: &str,
    role: Option<String>,
) -> Result<CreateAllowedUserRecord, ApiError> {
    if email.trim().is_empty() || name.trim().is_empty() {
        return Err(ApiError::bad_request("email and name are required"));
    }

    let email = normalize_email(email);
    let external_user_id = external_user_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| email.clone());
    let role = role
        .map(|role| role.trim().to_string())
        .filter(|role| !role.is_empty())
        .unwrap_or_else(|| "user".to_string());

    Ok(CreateAllowedUserRecord {
        public_id: Uuid::new_v4().to_string(),
        external_user_id,
        email,
        name: name.trim().to_string(),
        role,
        created_at: Utc::now(),
    })
}

async fn developer_login(
    State(state): State<AppState>,
    Json(payload): Json<DeveloperLoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let email = normalize_email(&payload.email);
    let developer = state
        .db
        .list_developer_records()
        .await?
        .into_iter()
        .find(|developer| normalize_email(&developer.email) == email)
        .ok_or_else(|| ApiError::unauthorized("invalid email or password"))?;

    if !verify_password(&developer, &payload.password)? {
        return Err(ApiError::unauthorized("invalid email or password"));
    }

    let user = User {
        external_user_id: developer.id.to_string(),
        email: developer.email.clone(),
        name: developer.name.clone(),
        role: developer.role.clone(),
    };

    Ok(Json(LoginResponse {
        token: create_session_token(&state, &user)?,
        user,
    }))
}

async fn list_developers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DeveloperView>>, ApiError> {
    require_developer(&state, &headers)?;
    Ok(Json(state.db.list_developers().await?))
}

async fn create_developer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateDeveloperRequest>,
) -> Result<Json<DeveloperView>, ApiError> {
    require_developer(&state, &headers)?;

    if payload.email.trim().is_empty()
        || payload.name.trim().is_empty()
        || payload.password.trim().is_empty()
    {
        return Err(ApiError::bad_request(
            "email, name, and password are required",
        ));
    }
    if payload.password.len() < 8 {
        return Err(ApiError::bad_request(
            "password must be at least 8 characters",
        ));
    }

    let record = CreateDeveloperRecord {
        public_id: Uuid::new_v4().to_string(),
        email: normalize_email(&payload.email),
        name: payload.name.trim().to_string(),
        role: payload.role.unwrap_or_else(|| "developer".to_string()),
        password_hash: hash_password(&payload.password)?,
        created_at: Utc::now(),
    };

    Ok(Json(state.db.create_developer(record).await?))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "service": "triagekit-server" }))
}

async fn create_login_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LoginCodeRequest>,
) -> Result<Json<LoginCodeResponse>, ApiError> {
    require_app_secret(&state, &headers)?;

    let code = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::minutes(2);
    let return_to = payload.return_to.unwrap_or_else(|| "/issues".to_string());
    let allowed_user = find_allowed_user(&state, &payload.email).await?;
    let user = User {
        external_user_id: if payload.external_user_id.trim().is_empty() {
            allowed_user.external_user_id
        } else {
            payload.external_user_id
        },
        email: allowed_user.email,
        name: if payload.name.trim().is_empty() {
            allowed_user.name
        } else {
            payload.name
        },
        role: if payload.role.trim().is_empty() {
            allowed_user.role
        } else {
            payload.role
        },
    };

    state.code_store.save(
        code.clone(),
        LoginCode {
            user,
            return_to,
            expires_at,
        },
    )?;

    let redirect_url = format!(
        "{}{}/sso/callback?code={}",
        state.public_web_url,
        normalize_base_path(&state.base_path),
        code
    );

    Ok(Json(LoginCodeResponse {
        redirect_url,
        expires_at,
    }))
}

async fn exchange_login_code(
    State(state): State<AppState>,
    Json(payload): Json<ExchangeCodeRequest>,
) -> Result<Json<ExchangeCodeResponse>, ApiError> {
    let login_code = state.code_store.take(&payload.code)?;

    if login_code.expires_at < Utc::now() {
        return Err(ApiError::bad_request("expired login code"));
    }

    Ok(Json(ExchangeCodeResponse {
        token: create_session_token(&state, &login_code.user)?,
        user: login_code.user,
        return_to: login_code.return_to,
    }))
}

async fn list_issues(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Issue>>, ApiError> {
    session_user(&state, &headers)?;
    Ok(Json(state.db.list_issues().await?))
}

async fn create_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateIssueRequest>,
) -> Result<Json<Issue>, ApiError> {
    let user = session_user(&state, &headers)?;
    if payload.title.trim().is_empty() {
        return Err(ApiError::bad_request("title is required"));
    }

    let issue = CreateIssueRecord {
        public_id: Uuid::new_v4().to_string(),
        title: payload.title,
        description: payload.description,
        issue_type: payload.issue_type,
        priority: payload.priority,
        created_by: user,
        created_at: Utc::now(),
        source_url: payload.source_url,
    };

    Ok(Json(state.db.create_issue(issue).await?))
}

async fn update_issue_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateIssueStatusRequest>,
) -> Result<Json<Issue>, ApiError> {
    require_developer(&state, &headers)?;
    Ok(Json(
        state
            .db
            .update_issue_status(UpdateIssueStatusRecord {
                public_id: id.to_string(),
                status: payload.status,
            })
            .await?,
    ))
}

fn require_app_secret(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let expected = format!("Bearer {}", state.app_secret);
    let actual = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if actual != expected {
        return Err(ApiError::unauthorized("invalid app secret"));
    }

    Ok(())
}

fn require_developer(state: &AppState, headers: &HeaderMap) -> Result<Claims, ApiError> {
    let token = bearer_token(headers)?;
    let claims = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| ApiError::unauthorized("invalid session"))?
    .claims;

    if claims.role != "developer" && claims.role != "owner" {
        return Err(ApiError::unauthorized("developer access required"));
    }

    Ok(claims)
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, ApiError> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing session"))?;

    header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::unauthorized("missing session"))
}

fn session_user(state: &AppState, headers: &HeaderMap) -> Result<User, ApiError> {
    let token = bearer_token(headers)?;
    let claims = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| ApiError::unauthorized("invalid session"))?
    .claims;

    Ok(User {
        external_user_id: claims.sub,
        email: claims.email,
        name: claims.name,
        role: claims.role,
    })
}

fn demo_user() -> User {
    User {
        external_user_id: "demo-user".to_string(),
        email: "demo@example.com".to_string(),
        name: "Demo User".to_string(),
        role: "owner".to_string(),
    }
}

fn seed_issue_record() -> SeedIssueRecord {
    SeedIssueRecord {
        public_id: Uuid::new_v4().to_string(),
        title: "Feedback inbox prototype".to_string(),
        description: "Wire the first issue list, creation form, and status updates.".to_string(),
        issue_type: IssueType::Feature,
        status: IssueStatus::Triaged,
        priority: Priority::Medium,
        created_by: demo_user(),
        created_at: Utc::now(),
        source_url: Some("/admin/dashboard".to_string()),
    }
}

fn seed_developer_record() -> CreateDeveloperRecord {
    let password = env::var("BOOTSTRAP_ADMIN_PASSWORD").unwrap_or_else(|_| "change-me".to_string());
    CreateDeveloperRecord {
        public_id: Uuid::new_v4().to_string(),
        email: env::var("BOOTSTRAP_ADMIN_EMAIL").unwrap_or_else(|_| "dev@example.com".to_string()),
        name: "Owner".to_string(),
        role: "owner".to_string(),
        password_hash: hash_password(&password).unwrap_or_else(|_| password),
        created_at: Utc::now(),
    }
}

fn seed_allowed_user_record() -> CreateAllowedUserRecord {
    CreateAllowedUserRecord {
        public_id: Uuid::new_v4().to_string(),
        external_user_id: "hms-admin-1".to_string(),
        email: "admin@example.com".to_string(),
        name: "HMS Admin".to_string(),
        role: "user".to_string(),
        created_at: Utc::now(),
    }
}

async fn find_allowed_user(state: &AppState, email: &str) -> Result<AllowedUser, ApiError> {
    let email = normalize_email(email);
    state
        .db
        .list_allowed_users()
        .await?
        .into_iter()
        .find(|user| normalize_email(&user.email) == email)
        .ok_or_else(|| ApiError::unauthorized("user is not allowed"))
}

fn create_session_token(state: &AppState, user: &User) -> Result<String, ApiError> {
    let exp = (Utc::now() + Duration::hours(12)).timestamp() as usize;
    let claims = Claims {
        sub: user.external_user_id.clone(),
        email: user.email.clone(),
        name: user.name.clone(),
        role: user.role.clone(),
        exp,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|_| ApiError::internal("failed to create session token"))
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| ApiError::internal("failed to hash password"))
}

fn verify_password(developer: &DeveloperRecord, password: &str) -> Result<bool, ApiError> {
    if let Some(hash) = &developer.password_hash {
        let parsed_hash =
            PasswordHash::new(hash).map_err(|_| ApiError::internal("invalid password hash"))?;
        return Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok());
    }

    Ok(developer.password.as_deref() == Some(password))
}

impl From<DeveloperRecord> for DeveloperView {
    fn from(developer: DeveloperRecord) -> Self {
        Self {
            id: developer.id,
            email: developer.email,
            name: developer.name,
            role: developer.role,
            created_at: developer.created_at,
        }
    }
}

fn normalize_base_path(path: &str) -> String {
    if path == "/" {
        String::new()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn build_code_store() -> anyhow::Result<CodeStore> {
    let token_store = env::var("TOKEN_STORE").unwrap_or_else(|_| "memory".to_string());
    if token_store != "redis" {
        return Ok(CodeStore::Memory(Arc::new(Mutex::new(HashMap::new()))));
    }

    let mut redis_url =
        env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_db = env::var("REDIS_DB").unwrap_or_else(|_| "12".to_string());
    if !redis_url
        .rsplit('/')
        .next()
        .is_some_and(|tail| tail.parse::<u8>().is_ok())
    {
        redis_url = format!("{}/{}", redis_url.trim_end_matches('/'), redis_db);
    }

    let client = redis::Client::open(redis_url)?;
    let prefix = env::var("REDIS_KEY_PREFIX").unwrap_or_else(|_| "triagekit:local".to_string());
    Ok(CodeStore::Redis {
        client,
        prefix,
        ttl_seconds: 120,
    })
}

impl CodeStore {
    fn save(&self, code: String, login_code: LoginCode) -> Result<(), ApiError> {
        match self {
            CodeStore::Memory(codes) => {
                codes.lock().unwrap().insert(code, login_code);
                Ok(())
            }
            CodeStore::Redis {
                client,
                prefix,
                ttl_seconds,
            } => {
                let mut connection = client
                    .get_connection()
                    .map_err(|_| ApiError::internal("failed to connect to redis"))?;
                let key = format!("{prefix}:sso:{code}");
                let value = serde_json::to_string(&login_code)
                    .map_err(|_| ApiError::internal("failed to serialize login code"))?;
                let _: () = connection
                    .set_ex(key, value, *ttl_seconds)
                    .map_err(|_| ApiError::internal("failed to store login code"))?;
                Ok(())
            }
        }
    }

    fn take(&self, code: &str) -> Result<LoginCode, ApiError> {
        match self {
            CodeStore::Memory(codes) => codes
                .lock()
                .unwrap()
                .remove(code)
                .ok_or_else(|| ApiError::bad_request("invalid login code")),
            CodeStore::Redis { client, prefix, .. } => {
                let mut connection = client
                    .get_connection()
                    .map_err(|_| ApiError::internal("failed to connect to redis"))?;
                let key = format!("{prefix}:sso:{code}");
                let value: Option<String> = connection
                    .get_del(key)
                    .map_err(|_| ApiError::internal("failed to read login code"))?;
                let value = value.ok_or_else(|| ApiError::bad_request("invalid login code"))?;
                serde_json::from_str(&value)
                    .map_err(|_| ApiError::bad_request("invalid login code payload"))
            }
        }
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.to_string(),
        }
    }

    fn unauthorized(message: &str) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.to_string(),
        }
    }

    fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.to_string(),
        }
    }

    fn conflict(message: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.to_string(),
        }
    }

    fn internal(message: &str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.to_string(),
        }
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(serde_json::json!({
                "error": {
                    "message": self.message
                }
            })),
        )
            .into_response()
    }
}
