use crate::{
    Account, ApiError, ApiResult, AppState, PATIENT_QUERY, Patient, authenticate,
    authorize_patient, check_auth_limit, seed, session_response, token_hash,
};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use std::net::SocketAddr;
use synapse_domain::{CreatePatientInput, ProfileInput, Role, validate_new_password};
use uuid::Uuid;

fn conflict(code: &'static str, message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, code, message.into())
}

fn registration_error(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .is_some_and(|e| e.code().as_deref() == Some("23505"))
    {
        conflict(
            "account_exists",
            "Этот логин уже используется. Укажите другой логин пациента",
        )
    } else {
        error.into()
    }
}

#[utoipa::path(
    post, path = "/api/v1/patients", operation_id = "create_patient",
    security(("session_cookie" = [])),
    params(("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    request_body = CreatePatientInput,
    responses(
        (status = 413, description = "Тело запроса слишком велико", body = String, content_type = "text/plain"),
        (status = 415, description = "Требуется JSON", body = String, content_type = "text/plain"),
        (status = 200, description = "Успех", body = Patient),
        (status = 201, description = "Создано", body = Patient),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 409, description = "Конфликт данных", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn create_patient(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut input): Json<CreatePatientInput>,
) -> ApiResult<Response> {
    let doctor = authenticate(&state, &headers).await?;
    if doctor.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    input.normalize();
    input
        .validate(Utc::now().date_naive())
        .map_err(ApiError::invalid)?;
    if state.delivery.is_strict() && !crate::registration::valid_email_address(&input.email) {
        return Err(ApiError::invalid(
            "В строгом режиме для приглашения укажите электронную почту пациента",
        ));
    }
    // До активации пациент не может войти, а случайный исходный пароль никому не передаётся.
    let hash = tokio::task::spawn_blocking(|| seed::hash(&Uuid::new_v4().to_string()))
        .await
        .map_err(|_| ApiError::internal())?;
    let mut tx = state.db.begin().await?;
    let registered = sqlx::query_as::<_, Account>(
        "SELECT * FROM users WHERE email=$1 AND activated AND role='patient' FOR UPDATE",
    )
    .bind(&input.email)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(account) = registered {
        // Уже зарегистрированный пациент подтверждает связь ссылкой, не создавая второй аккаунт.
        let inserted = sqlx::query("INSERT INTO patient_profiles(user_id,doctor_id,birth_date,access_accepted) VALUES($1,$2,$3,false) ON CONFLICT(user_id) DO NOTHING")
            .bind(account.id).bind(doctor.id).bind(input.birth_date).execute(&mut *tx).await?;
        let matching: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM patient_profiles WHERE user_id=$1 AND doctor_id=$2 AND birth_date=$3 AND NOT access_accepted)")
            .bind(account.id).bind(doctor.id).bind(input.birth_date).fetch_one(&mut *tx).await?;
        if !matching {
            return Err(conflict(
                "account_exists",
                "Аккаунт уже связан с наблюдением",
            ));
        }
        let profile = sqlx::query_as::<_, Patient>(&format!("{PATIENT_QUERY} WHERE p.user_id=$1"))
            .bind(account.id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok((
            if inserted.rows_affected() == 1 {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            Json(profile),
        )
            .into_response());
    }
    let inserted = sqlx::query("INSERT INTO users(id,email,name,role,password_hash,activated) VALUES ($1,$2,$3,'patient',$4,false) ON CONFLICT (id) DO NOTHING")
        .bind(input.id).bind(&input.email).bind(&input.name).bind(hash)
        .execute(&mut *tx).await.map_err(registration_error)?;
    if inserted.rows_affected() == 1 {
        sqlx::query("INSERT INTO patient_profiles(user_id,doctor_id,birth_date) VALUES ($1,$2,$3)")
            .bind(input.id)
            .bind(doctor.id)
            .bind(input.birth_date)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO observation_events(id,patient_id,author_id,title,body,patient_visible,occurred_at) VALUES ($1,$2,$3,'Начало наблюдения','Врач создал карточку пациента.',true,now())")
            .bind(Uuid::new_v4()).bind(input.id).bind(doctor.id).execute(&mut *tx).await?;
    } else {
        // Повтор создания с тем же идентификатором не меняет существующий аккаунт или его врача.
        let same: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users u JOIN patient_profiles p ON p.user_id=u.id WHERE u.id=$1 AND u.email=$2 AND u.name=$3 AND p.birth_date=$4 AND p.doctor_id=$5)")
            .bind(input.id).bind(&input.email).bind(&input.name).bind(input.birth_date).bind(doctor.id)
            .fetch_one(&mut *tx).await?;
        if !same {
            return Err(ApiError::conflict());
        }
    }
    let profile: Patient = sqlx::query_as(&format!("{PATIENT_QUERY} WHERE p.user_id=$1"))
        .bind(input.id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        if inserted.rows_affected() == 1 {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(profile),
    )
        .into_response())
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct Invitation {
    code: String,
    expires_at: DateTime<Utc>,
}

#[utoipa::path(
    post, path = "/api/v1/patients/{id}/invitation", operation_id = "create_invitation",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    responses(
        (status = 200, description = "Успех", body = Invitation),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 409, description = "Конфликт данных", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn create_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Invitation>> {
    let doctor = authenticate(&state, &headers).await?;
    if doctor.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &doctor, id).await?;
    let mut tx = state.db.begin().await?;
    let activated: bool = sqlx::query_scalar("SELECT u.activated AND p.access_accepted FROM users u JOIN patient_profiles p ON p.user_id=u.id WHERE u.id=$1 AND p.doctor_id=$2 FOR UPDATE OF u")
        .bind(id).bind(doctor.id).fetch_one(&mut *tx).await?;
    if activated {
        return Err(conflict(
            "already_connected",
            "Пациент уже подключён к платформе",
        ));
    }
    // Новый код отзывает предыдущий. В БД сохраняется только хеш, а исходный код возвращается один раз.
    sqlx::query("UPDATE patient_invitations SET revoked_at=now() WHERE patient_id=$1 AND consumed_at IS NULL AND revoked_at IS NULL")
        .bind(id).execute(&mut *tx).await?;
    let code = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let expires_at = sqlx::query_scalar("INSERT INTO patient_invitations(id,patient_id,created_by,token_hash,expires_at) VALUES ($1,$2,$3,$4,now()+interval '72 hours') RETURNING expires_at")
        .bind(Uuid::new_v4()).bind(id).bind(doctor.id).bind(token_hash(&code))
        .fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(Invitation { code, expires_at }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ActivateInput {
    code: String,
    password: String,
}

#[utoipa::path(
    post, path = "/api/v1/auth/activate", operation_id = "activate",
    params(("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    request_body = ActivateInput,
    responses(
        (status = 413, description = "Тело запроса слишком велико", body = String, content_type = "text/plain"),
        (status = 415, description = "Требуется JSON", body = String, content_type = "text/plain"),
        (status = 200, description = "Успех", body = synapse_domain::User),
        (status = 202, description = "Требуется подтверждение почты", body = crate::registration::RegistrationResult),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 429, description = "Превышен лимит запросов", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn activate(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(input): Json<ActivateInput>,
) -> ApiResult<Response> {
    check_auth_limit(&state, address.ip(), "activate")?;
    validate_new_password(&input.password).map_err(ApiError::invalid)?;
    let code = input.code.trim().to_lowercase();
    let invalid = || {
        ApiError::invalid(
            "Приглашение недействительно или срок его действия истёк. Запросите новую ссылку у врача",
        )
    };
    if code.len() != 64 || !code.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    let mut tx = state.db.begin().await?;
    // Сначала блокируем аккаунт, затем приглашение: тот же порядок используется при выдаче нового кода.
    let existing = sqlx::query_as::<_, Account>("SELECT u.* FROM users u JOIN patient_invitations i ON i.patient_id=u.id JOIN patient_profiles p ON p.user_id=u.id WHERE i.token_hash=$1 AND (NOT u.activated OR NOT p.access_accepted) FOR UPDATE OF u")
        .bind(token_hash(&code)).fetch_optional(&mut *tx).await?.ok_or_else(invalid)?;
    let patient_id = existing.id;
    let invitation_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM patient_invitations WHERE token_hash=$1 AND patient_id=$2 AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at>now() FOR UPDATE")
        .bind(token_hash(&code)).bind(patient_id).fetch_optional(&mut *tx).await?;
    let invitation_id = invitation_id.ok_or_else(invalid)?;
    if state.delivery.is_strict() && !crate::registration::valid_email_address(&existing.email) {
        return Err(ApiError::invalid(
            "Для подтверждения связи укажите действующий адрес электронной почты",
        ));
    }
    let verify_email = state.delivery.has_mail() && !existing.email_verified;
    let password_hash = if existing.activated {
        let hash = existing.password_hash.clone();
        let valid = tokio::task::spawn_blocking(move || {
            PasswordHash::new(&hash).is_ok_and(|parsed| {
                Argon2::default()
                    .verify_password(input.password.as_bytes(), &parsed)
                    .is_ok()
            })
        })
        .await
        .map_err(|_| ApiError::internal())?;
        if !valid {
            return Err(ApiError::invalid("Введите пароль существующего аккаунта"));
        }
        existing.password_hash
    } else {
        tokio::task::spawn_blocking(move || seed::hash(&input.password))
            .await
            .map_err(|_| ApiError::internal())?
    };
    let account: Account =
        sqlx::query_as("UPDATE users SET password_hash=$1,activated=true,email_verified=CASE WHEN $3 THEN email_verified ELSE true END,registration_consent_at=coalesce(registration_consent_at,now()) WHERE id=$2 RETURNING *")
            .bind(password_hash)
            .bind(patient_id)
            .bind(verify_email)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("UPDATE patient_profiles SET access_accepted=true WHERE user_id=$1")
        .bind(patient_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE patient_invitations SET consumed_at=now() WHERE id=$1")
        .bind(invitation_id)
        .execute(&mut *tx)
        .await?;
    let session = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let verification_token = if verify_email {
        let token = crate::account_security::fresh_token();
        sqlx::query("INSERT INTO auth_action_tokens(token_hash,user_id,purpose,expires_at) VALUES($1,$2,'verify_email',now()+interval '24 hours')")
            .bind(token_hash(&token)).bind(patient_id).execute(&mut *tx).await?;
        Some(token)
    } else {
        sqlx::query("INSERT INTO sessions(token_hash,user_id,expires_at) VALUES ($1,$2,now()+interval '7 days')")
            .bind(token_hash(&session)).bind(patient_id).execute(&mut *tx).await?;
        None
    };
    tx.commit().await?;
    if let Some(token) = verification_token {
        if let Some(url) = state.delivery.confirmation_url(&token)
            && state.delivery.send_mail(
                &account.email,
                "Подтверждение электронной почты СИНАПС",
                &format!("Чтобы завершить подключение к врачу и подтвердить почту, откройте ссылку: {url}\nСсылка действует 24 часа."),
            ).await.is_err() {
            tracing::warn!("Не удалось отправить письмо подтверждения после активации приглашения");
        }
        return Ok((
            StatusCode::ACCEPTED,
            Json(crate::registration::RegistrationResult {
                verification_required: true,
                email: account.email,
            }),
        )
            .into_response());
    }
    Ok(session_response(&state, &session, account.public()))
}

#[derive(FromRow)]
struct ProfileRow {
    version: i32,
    name: String,
    birth_date: chrono::NaiveDate,
    observation_started: chrono::NaiveDate,
    status: String,
    medical_summary: String,
    allergies: Option<String>,
    medical_history: sqlx::types::Json<synapse_domain::MedicalHistory>,
    physical_info: sqlx::types::Json<synapse_domain::PhysicalInfo>,
}

impl From<ProfileRow> for ProfileInput {
    fn from(row: ProfileRow) -> Self {
        Self {
            version: row.version,
            name: row.name,
            birth_date: row.birth_date,
            observation_started: row.observation_started,
            status: row.status,
            medical_summary: row.medical_summary,
            allergies: row.allergies,
            medical_history: Some(row.medical_history.0),
            physical_info: Some(row.physical_info.0),
        }
    }
}

#[utoipa::path(
    patch, path = "/api/v1/patients/{id}", operation_id = "update_patient",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    request_body = ProfileInput,
    responses(
        (status = 413, description = "Тело запроса слишком велико", body = String, content_type = "text/plain"),
        (status = 415, description = "Требуется JSON", body = String, content_type = "text/plain"),
        (status = 200, description = "Успех", body = Patient),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 409, description = "Конфликт данных", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn update_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut input): Json<ProfileInput>,
) -> ApiResult<Json<Patient>> {
    let doctor = authenticate(&state, &headers).await?;
    if doctor.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &doctor, id).await?;
    input.normalize();
    input
        .validate(Utc::now().date_naive())
        .map_err(ApiError::invalid)?;
    let mut tx = state.db.begin().await?;
    // Во всех сценариях сначала блокируем аккаунт; свежую карточку читаем следующим запросом после ожидания.
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let before: ProfileInput = sqlx::query_as::<_, ProfileRow>("SELECT p.version,u.name,p.birth_date,p.observation_started,p.status,p.medical_summary,p.allergies,p.medical_history,p.physical_info FROM patient_profiles p JOIN users u ON u.id=p.user_id WHERE p.user_id=$1 AND p.doctor_id=$2")
        .bind(id).bind(doctor.id).fetch_one(&mut *tx).await?.into();
    if before.version != input.version {
        return Err(conflict(
            "stale_version",
            "Карточка уже изменена в другом окне. Обновите данные перед сохранением",
        ));
    }
    if input.medical_history.is_none() {
        input.medical_history = before.medical_history.clone();
    }
    if input.physical_info.is_none() {
        input.physical_info = before.physical_info.clone();
    }
    if before != input {
        input.version = before.version + 1;
        sqlx::query("UPDATE users SET name=$1 WHERE id=$2")
            .bind(&input.name)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE patient_profiles SET birth_date=$1,observation_started=$2,status=$3,medical_summary=$4,allergies=$5,version=$6,medical_history=$8,physical_info=$9 WHERE user_id=$7")
            .bind(input.birth_date).bind(input.observation_started).bind(&input.status)
            .bind(&input.medical_summary).bind(&input.allergies).bind(input.version).bind(id)
            .bind(sqlx::types::Json(&input.medical_history))
            .bind(sqlx::types::Json(&input.physical_info))
            .execute(&mut *tx).await?;
        sqlx::query("INSERT INTO patient_profile_changes(id,patient_id,author_id,version_before,version_after,before_data,after_data) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(Uuid::new_v4()).bind(id).bind(doctor.id).bind(before.version).bind(input.version)
            .bind(sqlx::types::Json(&before)).bind(sqlx::types::Json(&input)).execute(&mut *tx).await?;
    }
    let profile = sqlx::query_as(&format!("{PATIENT_QUERY} WHERE p.user_id=$1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(profile))
}

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct ProfileChange {
    id: Uuid,
    version_before: i32,
    version_after: i32,
    author_name: String,
    created_at: DateTime<Utc>,
    #[schema(value_type = ProfileInput)]
    before_data: Value,
    #[schema(value_type = ProfileInput)]
    after_data: Value,
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}/changes", operation_id = "list_profile_changes",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, description = "Успех", body = Vec<ProfileChange>),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn profile_changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ProfileChange>>> {
    let doctor = authenticate(&state, &headers).await?;
    if doctor.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &doctor, id).await?;
    Ok(Json(sqlx::query_as("SELECT c.id,c.version_before,c.version_after,c.created_at,c.before_data,c.after_data,u.name AS author_name FROM patient_profile_changes c JOIN users u ON u.id=c.author_id WHERE c.patient_id=$1 ORDER BY c.version_after DESC LIMIT 100")
        .bind(id).fetch_all(&state.db).await?))
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ChangesQuery {
    limit: Option<i64>,
    before_version: Option<i32>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ChangesCursor {
    before_version: i32,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ChangesPage {
    items: Vec<ProfileChange>,
    #[schema(required = true)]
    next_cursor: Option<ChangesCursor>,
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}/changes/page", operation_id = "list_profile_changes_page",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ChangesQuery),
    responses(
        (status = 200, description = "Успех", body = ChangesPage),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn profile_changes_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<ChangesQuery>,
) -> ApiResult<Json<ChangesPage>> {
    let doctor = authenticate(&state, &headers).await?;
    if doctor.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &doctor, id).await?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) || query.before_version.is_some_and(|version| version < 2) {
        return Err(ApiError::invalid(
            "Размер страницы: 1–50; версия курсора: не меньше 2",
        ));
    }
    // Версия уникальна внутри карточки; новые изменения не сдвигают старые страницы.
    let mut items = sqlx::query_as::<_, ProfileChange>("SELECT c.id,c.version_before,c.version_after,c.created_at,c.before_data,c.after_data,u.name AS author_name FROM patient_profile_changes c JOIN users u ON u.id=c.author_id WHERE c.patient_id=$1 AND ($2::integer IS NULL OR c.version_after<$2) ORDER BY c.version_after DESC LIMIT $3")
        .bind(id).bind(query.before_version).bind(limit+1).fetch_all(&state.db).await?;
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next_cursor = if more {
        items.last().map(|last| ChangesCursor {
            before_version: last.version_after,
        })
    } else {
        None
    };
    Ok(Json(ChangesPage { items, next_cursor }))
}
