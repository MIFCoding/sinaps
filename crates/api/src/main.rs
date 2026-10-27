mod account_security;
mod admin;
mod alerts;
mod appointments;
mod chat;
mod clinical;
mod clinical_catalog;
mod clinical_recommendations;
mod contract;
mod custom_questionnaires;
mod delivery;
mod drug_catalog;
mod history;
mod notifications;
mod patient_directory;
mod patient_management;
mod questionnaires;
mod registration;
mod seed;
mod therapy;

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Json, Router,
    extract::{ConnectInfo, DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use synapse_domain::{DiaryInput, EventInput, Role, User};
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use uuid::Uuid;

type AuthAttempts = HashMap<(IpAddr, &'static str), (Instant, u32)>;

#[derive(Clone)]
struct AppState {
    db: PgPool,
    secure_cookie: bool,
    origins: Arc<Vec<String>>,
    auth_attempts: Arc<Mutex<AuthAttempts>>,
    delivery: Arc<delivery::Delivery>,
}

struct ApiError(StatusCode, &'static str, String);
type ApiResult<T> = Result<T, ApiError>;

#[derive(Serialize, utoipa::ToSchema)]
struct ErrorEnvelope {
    error: ErrorDetail,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ErrorDetail {
    code: String,
    message: String,
}

impl ApiError {
    fn unauthorized() -> Self {
        Self(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Войдите в аккаунт".into(),
        )
    }
    fn forbidden() -> Self {
        Self(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Нет доступа к этим данным".into(),
        )
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_error",
            message.into(),
        )
    }
    fn conflict() -> Self {
        Self(
            StatusCode::CONFLICT,
            "conflict",
            "Этот идентификатор уже использован для другой записи".into(),
        )
    }
    fn not_found() -> Self {
        Self(
            StatusCode::NOT_FOUND,
            "not_found",
            "Запись не найдена".into(),
        )
    }
    fn internal() -> Self {
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Не удалось выполнить запрос. Попробуйте ещё раз".into(),
        )
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        // Не выводим запросы, параметры и ошибки БД, которые могут содержать данные пациента.
        let kind = match error {
            sqlx::Error::PoolTimedOut => "pool_timeout",
            sqlx::Error::Database(_) => "database",
            _ => "query",
        };
        tracing::error!(kind, "database request failed");
        Self::internal()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(ErrorEnvelope {
                error: ErrorDetail {
                    code: self.1.to_owned(),
                    message: self.2,
                },
            }),
        )
            .into_response()
    }
}

#[derive(FromRow)]
struct Account {
    id: Uuid,
    email: String,
    name: String,
    role: String,
    password_hash: String,
    activated: bool,
    email_verified: bool,
}

impl Account {
    fn public(&self) -> User {
        User {
            id: self.id,
            email: self.email.clone(),
            name: self.name.clone(),
            role: match self.role.as_str() {
                "doctor" => Role::Doctor,
                "admin" => Role::Admin,
                _ => Role::Patient,
            },
        }
    }
}

#[derive(Serialize, FromRow, utoipa::ToSchema)]
struct Patient {
    id: Uuid,
    email: String,
    name: String,
    account_activated: bool,
    version: i32,
    birth_date: NaiveDate,
    observation_started: NaiveDate,
    #[schema(value_type = synapse_domain::PatientStatus)]
    status: String,
    medical_summary: String,
    #[schema(required = true)]
    allergies: Option<String>,
    #[schema(value_type = synapse_domain::MedicalHistory)]
    medical_history: sqlx::types::Json<synapse_domain::MedicalHistory>,
    #[schema(value_type = synapse_domain::PhysicalInfo)]
    physical_info: sqlx::types::Json<synapse_domain::PhysicalInfo>,
    doctor_name: String,
    #[schema(required = true)]
    last_entry_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    latest_mood: Option<i16>,
    #[schema(required = true)]
    latest_anxiety: Option<i16>,
    #[schema(required = true)]
    latest_sleep: Option<f64>,
    entry_count: i64,
}

const PATIENT_QUERY: &str = "SELECT u.id, u.email, u.name, (u.activated AND p.access_accepted) AS account_activated, p.version, p.birth_date, p.observation_started, p.status,
    p.medical_summary, p.allergies, p.medical_history, p.physical_info, d.name AS doctor_name,
    latest.measured_at AS last_entry_at, latest.mood AS latest_mood,
    latest.anxiety AS latest_anxiety, latest.sleep_hours AS latest_sleep,
    (SELECT count(*) FROM diary_entries de WHERE de.patient_id = u.id) AS entry_count
    FROM patient_profiles p JOIN users u ON u.id = p.user_id JOIN users d ON d.id = p.doctor_id
    LEFT JOIN LATERAL (SELECT measured_at, mood, anxiety, sleep_hours FROM diary_entries
       WHERE patient_id = p.user_id ORDER BY measured_at DESC, created_at DESC LIMIT 1) latest ON true";

#[derive(Serialize, FromRow, utoipa::ToSchema)]
struct DiaryEntry {
    id: Uuid,
    patient_id: Uuid,
    measured_at: DateTime<Utc>,
    mood: i16,
    anxiety: i16,
    sleep_hours: f64,
    comment: String,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, FromRow, utoipa::ToSchema)]
struct ObservationEvent {
    id: Uuid,
    title: String,
    body: String,
    patient_visible: bool,
    occurred_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    author_name: String,
    #[schema(value_type = synapse_domain::EventKind)]
    kind: String,
    #[schema(required = true)]
    source: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct EventSaved {
    id: Uuid,
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|c| c.trim().split_once('='))
        .find(|(name, _)| *name == "synapse_session")
        .map(|(_, value)| value)
}

async fn authenticate_any(state: &AppState, headers: &HeaderMap) -> ApiResult<User> {
    let token = session_token(headers).ok_or_else(ApiError::unauthorized)?;
    let account = sqlx::query_as::<_, Account>("SELECT u.* FROM users u JOIN sessions s ON s.user_id = u.id WHERE s.token_hash = $1 AND s.expires_at > now() AND u.activated AND ($2 OR u.email_verified)")
        .bind(token_hash(token)).bind(!state.delivery.has_mail()).fetch_optional(&state.db).await?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(account.public())
}

async fn authenticate(state: &AppState, headers: &HeaderMap) -> ApiResult<User> {
    let user = authenticate_any(state, headers).await?;
    if user.role == Role::Admin {
        return Err(ApiError::forbidden());
    }
    Ok(user)
}

async fn authorize_patient(state: &AppState, user: &User, patient_id: Uuid) -> ApiResult<()> {
    let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM patient_profiles WHERE user_id = $1 AND (($2 = 'doctor' AND doctor_id = $3) OR ($2 = 'patient' AND user_id = $3 AND access_accepted)))")
        .bind(patient_id).bind(if user.role == Role::Doctor { "doctor" } else { "patient" })
        .bind(user.id).fetch_one(&state.db).await?;
    if allowed {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}

async fn request_guard(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let modifying = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if modifying {
        let headers = request.headers();
        let valid_header = headers
            .get("x-synapse-request")
            .and_then(|v| v.to_str().ok())
            == Some("1");
        let valid_origin = match headers.get(header::ORIGIN) {
            Some(origin) => origin
                .to_str()
                .is_ok_and(|o| state.origins.iter().any(|v| v == o)),
            None => true, // Нативный Rust-клиент и консольные инструменты не передают Origin.
        };
        if !valid_header || !valid_origin {
            return ApiError::forbidden().into_response();
        }
    }
    next.run(request).await
}

#[derive(Deserialize, utoipa::ToSchema)]
struct LoginInput {
    email: String,
    password: String,
}

fn check_auth_limit(state: &AppState, address: IpAddr, operation: &'static str) -> ApiResult<()> {
    let mut attempts = state
        .auth_attempts
        .lock()
        .map_err(|_| ApiError::internal())?;
    attempts.retain(|_, (since, _)| since.elapsed() < Duration::from_secs(60));
    let key = (address, operation);
    if attempts.len() >= 10_000 && !attempts.contains_key(&key) {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limit",
            "Повторите вход через минуту".into(),
        ));
    }
    let entry = attempts.entry(key).or_insert((Instant::now(), 0));
    entry.1 += 1;
    if entry.1 > 20 {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limit",
            "Слишком много попыток. Повторите через минуту".into(),
        ));
    }
    Ok(())
}

fn session_response(state: &AppState, token: &str, user: User) -> Response {
    let cookie = format!(
        "synapse_session={token}; HttpOnly; Path=/; SameSite=Lax; Max-Age=604800{}",
        if state.secure_cookie { "; Secure" } else { "" }
    );
    ([(header::SET_COOKIE, cookie)], Json(user)).into_response()
}

#[utoipa::path(
    post, path = "/api/v1/auth/login", operation_id = "login",
    params(("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    request_body = LoginInput,
    responses(
        (status = 413, description = "Тело запроса слишком велико", body = String, content_type = "text/plain"),
        (status = 415, description = "Требуется JSON", body = String, content_type = "text/plain"),
        (status = 200, description = "Успех", body = User),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 429, description = "Превышен лимит запросов", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn login(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(input): Json<LoginInput>,
) -> ApiResult<Response> {
    if input.email.len() > 254 || input.password.len() > 1024 {
        return Err(ApiError::invalid("Слишком длинные данные для входа"));
    }
    check_auth_limit(&state, address.ip(), "login")?;
    let account = sqlx::query_as::<_, Account>(
        "SELECT * FROM users WHERE email = $1 AND ($2 OR email_verified)",
    )
    .bind(synapse_domain::normalize_login(&input.email))
    .bind(!state.delivery.has_mail())
    .fetch_optional(&state.db)
    .await?;
    // Проверяем Argon2 даже для неизвестного аккаунта, чтобы не выдавать его существование временем ответа.
    let hash = match account.as_ref() {
        Some(a) => a.password_hash.clone(),
        None => seed::dummy_hash().to_owned(),
    };
    let valid = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|parsed| {
            Argon2::default()
                .verify_password(input.password.as_bytes(), &parsed)
                .is_ok()
        })
    })
    .await
    .map_err(|_| ApiError::internal())?;
    if !valid || account.as_ref().is_none_or(|a| !a.activated) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "Неверный логин или пароль".into(),
        ));
    }
    let account = account.ok_or_else(ApiError::unauthorized)?;
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query("INSERT INTO sessions(token_hash, user_id, expires_at) VALUES ($1, $2, now() + interval '7 days')")
        .bind(token_hash(&token)).bind(account.id).execute(&state.db).await?;
    Ok(session_response(&state, &token, account.public()))
}

#[utoipa::path(
    post, path = "/api/v1/auth/logout", operation_id = "logout",
    params(("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    responses(
        (status = 204, description = "Выполнено без тела ответа"),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = session_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(token_hash(token))
            .execute(&state.db)
            .await?;
    }
    Ok((
        [(
            header::SET_COOKIE,
            format!(
                "synapse_session=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0{}",
                if state.secure_cookie { "; Secure" } else { "" }
            ),
        )],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}

#[utoipa::path(
    get, path = "/api/v1/me", operation_id = "me",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Успех", body = User),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<User>> {
    Ok(Json(authenticate_any(&state, &headers).await?))
}

#[utoipa::path(
    get, path = "/api/v1/patients", operation_id = "list_patients",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Успех", body = Vec<Patient>),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn patients(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<Patient>>> {
    let user = authenticate(&state, &headers).await?;
    let query = if user.role == Role::Doctor {
        format!("{PATIENT_QUERY} WHERE p.doctor_id = $1 ORDER BY u.name LIMIT 100")
    } else {
        format!("{PATIENT_QUERY} WHERE p.user_id = $1 AND p.access_accepted")
    };
    Ok(Json(
        sqlx::query_as(&query)
            .bind(user.id)
            .fetch_all(&state.db)
            .await?,
    ))
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}", operation_id = "get_patient",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, description = "Успех", body = Patient),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn patient(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Patient>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    Ok(Json(
        sqlx::query_as(&format!("{PATIENT_QUERY} WHERE p.user_id = $1"))
            .bind(id)
            .fetch_one(&state.db)
            .await?,
    ))
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}/diary", operation_id = "list_diary",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, description = "Успех", body = Vec<DiaryEntry>),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn diary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<DiaryEntry>>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    Ok(Json(sqlx::query_as("SELECT * FROM diary_entries WHERE patient_id = $1 ORDER BY measured_at DESC, created_at DESC LIMIT 365")
        .bind(id).fetch_all(&state.db).await?))
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}/diary/{entry_id}", operation_id = "get_diary_entry",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ("entry_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Успех", body = DiaryEntry),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 404, description = "Запись не найдена", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn diary_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, entry_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<DiaryEntry>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let entry = sqlx::query_as("SELECT * FROM diary_entries WHERE patient_id=$1 AND id=$2")
        .bind(id)
        .bind(entry_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(entry))
}

#[utoipa::path(
    post, path = "/api/v1/patients/{id}/diary", operation_id = "save_diary",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    request_body = DiaryInput,
    responses(
        (status = 413, description = "Тело запроса слишком велико", body = String, content_type = "text/plain"),
        (status = 415, description = "Требуется JSON", body = String, content_type = "text/plain"),
        (status = 200, description = "Успех", body = DiaryEntry),
        (status = 201, description = "Создано", body = DiaryEntry),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 409, description = "Конфликт данных", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn save_diary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<DiaryInput>,
) -> ApiResult<Response> {
    let user = authenticate(&state, &headers).await?;
    if user.role != Role::Patient || user.id != id {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &user, id).await?;
    input.validate(Utc::now()).map_err(ApiError::invalid)?;
    let mut tx = state.db.begin().await?;
    let result = sqlx::query("INSERT INTO diary_entries(id, patient_id, measured_at, mood, anxiety, sleep_hours, comment) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (id) DO NOTHING")
        .bind(input.id).bind(id).bind(input.measured_at).bind(input.mood).bind(input.anxiety).bind(input.sleep_hours).bind(&input.comment)
        .execute(&mut *tx).await?;
    let entry = sqlx::query_as::<_, DiaryEntry>(
        "SELECT * FROM diary_entries WHERE id = $1 AND patient_id = $2",
    )
    .bind(input.id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::conflict)?;
    if entry.mood != input.mood
        || entry.anxiety != input.anxiety
        || entry.sleep_hours != input.sleep_hours
        || entry.comment != input.comment
        || entry.measured_at.timestamp_millis() != input.measured_at.timestamp_millis()
    {
        return Err(ApiError::conflict());
    }
    if result.rows_affected() == 1 {
        notifications::diary_added(&mut tx, id, input.id).await?;
    }
    tx.commit().await?;
    Ok((
        if result.rows_affected() == 1 {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(entry),
    )
        .into_response())
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}/events", operation_id = "list_events",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, description = "Успех", body = Vec<ObservationEvent>),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ObservationEvent>>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    Ok(Json(sqlx::query_as("SELECT e.id, e.title, e.body, e.patient_visible, e.occurred_at, e.created_at, u.name AS author_name, e.kind, e.source FROM observation_events e JOIN users u ON u.id = e.author_id WHERE patient_id = $1 AND ($2 OR patient_visible) ORDER BY occurred_at DESC, e.created_at DESC, e.id DESC LIMIT 100")
        .bind(id).bind(user.role == Role::Doctor).fetch_all(&state.db).await?))
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}/events/{event_id}", operation_id = "get_event",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ("event_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Успех", body = ObservationEvent),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 404, description = "Запись не найдена", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn observation_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, event_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<ObservationEvent>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let event = sqlx::query_as("SELECT e.id,e.title,e.body,e.patient_visible,e.occurred_at,e.created_at,u.name AS author_name,e.kind,e.source FROM observation_events e JOIN users u ON u.id=e.author_id WHERE e.patient_id=$1 AND e.id=$2 AND ($3 OR e.patient_visible)")
        .bind(id).bind(event_id).bind(user.role == Role::Doctor).fetch_optional(&state.db).await?.ok_or_else(ApiError::not_found)?;
    Ok(Json(event))
}

#[derive(FromRow)]
struct StoredEvent {
    title: String,
    body: String,
    patient_visible: bool,
    kind: String,
    source: Option<String>,
    occurred_at: DateTime<Utc>,
    occurred_at_provided: bool,
}

#[utoipa::path(
    post, path = "/api/v1/patients/{id}/events", operation_id = "save_event",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    request_body = EventInput,
    responses(
        (status = 413, description = "Тело запроса слишком велико", body = String, content_type = "text/plain"),
        (status = 415, description = "Требуется JSON", body = String, content_type = "text/plain"),
        (status = 200, description = "Успех", body = EventSaved),
        (status = 201, description = "Создано", body = EventSaved),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 409, description = "Конфликт данных", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
async fn save_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut input): Json<EventInput>,
) -> ApiResult<Response> {
    let user = authenticate(&state, &headers).await?;
    if user.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &user, id).await?;
    let birth_date: NaiveDate =
        sqlx::query_scalar("SELECT birth_date FROM patient_profiles WHERE user_id=$1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    input.normalize();
    input
        .validate(Utc::now(), birth_date)
        .map_err(ApiError::invalid)?;
    let occurred_at = input.occurred_at.map(|value| value.with_timezone(&Utc));
    let mut tx = state.db.begin().await?;
    let inserted = sqlx::query("INSERT INTO observation_events(id, patient_id, author_id, title, body, patient_visible, occurred_at, kind, source, occurred_at_provided) VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7,now()),$8,$9,$10) ON CONFLICT (id) DO NOTHING")
        .bind(input.id).bind(id).bind(user.id).bind(&input.title).bind(&input.body).bind(input.patient_visible)
        .bind(occurred_at).bind(input.kind.as_str()).bind(&input.source).bind(occurred_at.is_some()).execute(&mut *tx).await?;
    let existing = sqlx::query_as::<_, StoredEvent>("SELECT title, body, patient_visible, kind, source, occurred_at, occurred_at_provided FROM observation_events WHERE id=$1 AND patient_id=$2 AND author_id=$3")
        .bind(input.id).bind(id).bind(user.id).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::conflict)?;
    // Повтор не меняет дату, тип, источник или видимость уже сохранённого события.
    if existing.title != input.title
        || existing.body != input.body
        || existing.patient_visible != input.patient_visible
        || existing.kind != input.kind.as_str()
        || existing.source != input.source
        || existing.occurred_at_provided != occurred_at.is_some()
        || occurred_at.is_some_and(|value| {
            value.timestamp_millis() != existing.occurred_at.timestamp_millis()
        })
    {
        return Err(ApiError::conflict());
    }
    if inserted.rows_affected() == 1 && input.patient_visible {
        notifications::event_published(&mut tx, id, input.id).await?;
    }
    tx.commit().await?;
    Ok((
        if inserted.rows_affected() == 1 {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(EventSaved { id: input.id }),
    )
        .into_response())
}

#[utoipa::path(
    get, path = "/ready", operation_id = "ready",
    responses(
        (status = 200, description = "Успех", body = serde_json::Value),
        (status = 503, description = "База данных недоступна", body = crate::ErrorEnvelope)
    )
)]
async fn ready(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "not_ready",
                "База данных недоступна".into(),
            )
        })?;
    Ok(Json(json!({"status": "ready"})))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        println!("{}", serde_json::to_string_pretty(&contract::routes().1)?);
        return Ok(());
    }
    dotenvy::dotenv().ok();
    let delivery = delivery::Delivery::from_env()?;
    if delivery.is_strict() {
        if std::env::var("COOKIE_SECURE").as_deref() != Ok("true") {
            return Err("Строгий режим требует COOKIE_SECURE=true".into());
        }
        if std::env::var("ALLOWED_ORIGINS")
            .ok()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err("Строгий режим требует явно заданный ALLOWED_ORIGINS".into());
        }
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "synapse_api=info,tower_http=info".into()),
        )
        .init();
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set; see .env.example")?;
    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;
    sqlx::migrate!("../../migrations").run(&db).await?;
    custom_questionnaires::ensure_builtin_phq9(&db).await?;
    if std::env::args().nth(1).as_deref() == Some("create-admin") {
        admin::create_admin(&db).await?;
        return Ok(());
    }
    sqlx::query("UPDATE push_delivery_settings SET enabled=$1 WHERE id=1")
        .bind(delivery.has_push())
        .execute(&db)
        .await?;
    if std::env::args().nth(1).as_deref() == Some("seed") {
        if std::env::var("SYNAPSE_DEMO_SEED").as_deref() != Ok("true") {
            return Err("Set SYNAPSE_DEMO_SEED=true to explicitly seed synthetic accounts".into());
        }
        seed::run(&db).await?;
        return Ok(());
    }
    // Вычисляем подставной хеш один раз при запуске, а не при обработке запроса.
    seed::dummy_hash();
    let origins: Vec<String> = std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| {
            "http://localhost:5173,http://127.0.0.1:5173,http://tauri.localhost,tauri://localhost"
                .into()
        })
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let allowed: Vec<HeaderValue> = origins
        .iter()
        .map(|s| s.parse())
        .collect::<Result<_, _>>()?;
    let state = AppState {
        db,
        origins: Arc::new(origins),
        secure_cookie: std::env::var("COOKIE_SECURE").as_deref() == Ok("true"),
        auth_attempts: Arc::default(),
        delivery: Arc::new(delivery),
    };
    if state.delivery.has_push() {
        tokio::spawn(notifications::dispatch_push(state.clone()));
    }
    let cors = CorsLayer::new()
        .allow_origin(allowed)
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::PATCH])
        .allow_headers([
            header::CONTENT_TYPE,
            header::HeaderName::from_static("x-synapse-request"),
        ]);
    let request_id = header::HeaderName::from_static("x-request-id");
    let (router, document) = contract::routes();
    let app = router
        .route(
            "/api/v1/openapi.json",
            get(move || std::future::ready(Json(document.clone()))),
        )
        // Включает несколько полей анамнеза с Unicode и JSON-экранированием.
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), request_guard))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
        .with_state(state);
    let address = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!(%address, "Synapse API started");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        tokio::signal::ctrl_c().await.ok();
    })
    .await?;
    Ok(())
}
