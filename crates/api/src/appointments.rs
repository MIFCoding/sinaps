use crate::{ApiError, ApiResult, AppState, authenticate, authorize_patient};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use synapse_domain::Role;
use uuid::Uuid;

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct Appointment {
    id: Uuid,
    client_id: Uuid,
    doctor_id: Uuid,
    patient_id: Option<Uuid>,
    doctor_name: String,
    patient_name: Option<String>,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    status: String,
    note: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: i32,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AppointmentPage {
    items: Vec<Appointment>,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateInput {
    client_id: Uuid,
    patient_id: Option<Uuid>,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    note: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BookInput {
    client_id: Uuid,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ChangeInput {
    expected_version: i32,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
}

const SELECT: &str = "SELECT a.id,a.client_id,a.doctor_id,a.patient_id,
    d.name AS doctor_name,p.name AS patient_name,a.starts_at,a.ends_at,
    a.status,a.note,a.created_at,a.updated_at,a.version
    FROM appointments a
    JOIN users d ON d.id=a.doctor_id
    LEFT JOIN users p ON p.id=a.patient_id";

fn clean_note(value: &str) -> ApiResult<String> {
    let value = value.trim();
    if value.chars().count() > 1000
        || value
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(ApiError::invalid(
            "Комментарий к приёму не должен превышать 1000 символов",
        ));
    }
    Ok(value.to_owned())
}

fn validate_interval(starts_at: DateTime<Utc>, ends_at: DateTime<Utc>) -> ApiResult<()> {
    let duration = ends_at - starts_at;
    if duration <= chrono::Duration::zero() || duration > chrono::Duration::hours(4) {
        return Err(ApiError::invalid(
            "Интервал должен быть положительным и не превышать 4 часа",
        ));
    }
    if starts_at < Utc::now() - chrono::Duration::minutes(1) {
        return Err(ApiError::invalid("Нельзя создать приём в прошлом"));
    }
    Ok(())
}

async fn doctor(state: &AppState, headers: &HeaderMap) -> ApiResult<crate::User> {
    let user = authenticate(state, headers).await?;
    if user.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    Ok(user)
}

async fn lock_doctor(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: Uuid) -> ApiResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn notify(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    recipient_id: Uuid,
    patient_id: Uuid,
    appointment_id: Uuid,
    kind: &str,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO notifications(id,recipient_id,patient_id,kind,related_id)
         VALUES($1,$2,$3,$4,$5)
         ON CONFLICT (recipient_id,kind,related_id) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(recipient_id)
    .bind(patient_id)
    .bind(kind)
    .bind(appointment_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[utoipa::path(
    get, path = "/api/v1/appointments", operation_id = "list_appointments",
    security(("session_cookie" = [])), params(ListQuery),
    responses((status = 200, body = AppointmentPage), (status = 401, body = crate::ErrorEnvelope), (status = 403, body = crate::ErrorEnvelope), (status = 500, body = crate::ErrorEnvelope))
)]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<AppointmentPage>> {
    let user = authenticate(&state, &headers).await?;
    if query.from.is_some() != query.to.is_some() {
        return Err(ApiError::invalid("Период должен содержать обе даты"));
    }
    let (from, to) = query
        .from
        .zip(query.to)
        .unwrap_or((Utc::now(), Utc::now() + chrono::Duration::days(90)));
    let rows = if user.role == Role::Doctor {
        sqlx::query_as::<_, Appointment>(&format!(
            "{SELECT} WHERE a.doctor_id=$1 AND a.starts_at < $3 AND a.ends_at > $2 AND a.status <> 'cancelled' ORDER BY a.starts_at,a.id"
        ))
        .bind(user.id)
        .bind(from)
        .bind(to)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, Appointment>(&format!(
            "{SELECT} WHERE (a.patient_id=$1 OR (a.doctor_id=(SELECT doctor_id FROM patient_profiles WHERE user_id=$1) AND a.patient_id IS NULL)) AND a.starts_at < $3 AND a.ends_at > $2 AND a.status <> 'cancelled' ORDER BY a.starts_at,a.id"
        ))
        .bind(user.id)
        .bind(from)
        .bind(to)
        .fetch_all(&state.db)
        .await?
    };
    Ok(Json(AppointmentPage { items: rows }))
}

#[utoipa::path(
    post, path = "/api/v1/appointments", operation_id = "create_appointment",
    security(("session_cookie" = [])), request_body = CreateInput,
    responses((status = 201, body = Appointment), (status = 401, body = crate::ErrorEnvelope), (status = 403, body = crate::ErrorEnvelope), (status = 409, body = crate::ErrorEnvelope), (status = 422, body = crate::ErrorEnvelope), (status = 500, body = crate::ErrorEnvelope))
)]
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateInput>,
) -> ApiResult<(StatusCode, Json<Appointment>)> {
    let user = doctor(&state, &headers).await?;
    validate_interval(input.starts_at, input.ends_at)?;
    let note = clean_note(&input.note)?;
    if let Some(patient_id) = input.patient_id {
        authorize_patient(&state, &user, patient_id).await?;
    }
    let mut tx = state.db.begin().await?;
    lock_doctor(&mut tx, user.id).await?;
    if let Some(existing) = sqlx::query_as::<_, Appointment>(&format!(
        "{SELECT} WHERE a.client_id=$1 AND a.doctor_id=$2"
    ))
    .bind(input.client_id)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?
    {
        if existing.patient_id == input.patient_id
            && existing.starts_at == input.starts_at
            && existing.ends_at == input.ends_at
            && existing.note == note
        {
            tx.commit().await?;
            return Ok((StatusCode::CREATED, Json(existing)));
        }
        return Err(ApiError(
            StatusCode::CONFLICT,
            "appointment_conflict",
            "Идентификатор уже использован для другого интервала".into(),
        ));
    }
    let conflict: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM appointments WHERE doctor_id=$1 AND status <> 'cancelled' AND starts_at < $3 AND ends_at > $2)",
    )
    .bind(user.id).bind(input.starts_at).bind(input.ends_at)
    .fetch_one(&mut *tx).await?;
    if conflict {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "appointment_conflict",
            "Интервал пересекается с другим приёмом".into(),
        ));
    }
    let status = if input.patient_id.is_some() {
        "scheduled"
    } else {
        "available"
    };
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO appointments(id,client_id,doctor_id,patient_id,starts_at,ends_at,status,note,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$3)")
        .bind(id).bind(input.client_id).bind(user.id).bind(input.patient_id)
        .bind(input.starts_at).bind(input.ends_at).bind(status).bind(note)
        .execute(&mut *tx).await?;
    if let Some(patient_id) = input.patient_id {
        notify(&mut tx, patient_id, patient_id, id, "appointment_created").await?;
    }
    let row = sqlx::query_as::<_, Appointment>(&format!("{SELECT} WHERE a.id=$1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    post, path = "/api/v1/appointments/{id}/book", operation_id = "book_appointment",
    security(("session_cookie" = [])), params(("id" = Uuid, Path)), request_body = BookInput,
    responses((status = 200, body = Appointment), (status = 401, body = crate::ErrorEnvelope), (status = 403, body = crate::ErrorEnvelope), (status = 404, body = crate::ErrorEnvelope), (status = 409, body = crate::ErrorEnvelope), (status = 500, body = crate::ErrorEnvelope))
)]
pub async fn book(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<BookInput>,
) -> ApiResult<Json<Appointment>> {
    let patient = authenticate(&state, &headers).await?;
    if patient.role != Role::Patient {
        return Err(ApiError::forbidden());
    }

    authorize_patient(&state, &patient, patient.id).await?;
    let mut tx = state.db.begin().await?;
    let doctor_id: Uuid = sqlx::query_scalar("SELECT doctor_id FROM appointments WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    lock_doctor(&mut tx, doctor_id).await?;
    if let Some(existing) = sqlx::query_as::<_, Appointment>(&format!(
        "{SELECT} WHERE a.id=$1 AND a.patient_id=$2 AND a.booking_client_id=$3"
    ))
    .bind(id)
    .bind(patient.id)
    .bind(input.client_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        tx.commit().await?;
        return Ok(Json(existing));
    }
    let duplicate_client: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM appointments WHERE booking_client_id=$1 AND id<>$2)",
    )
    .bind(input.client_id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if duplicate_client {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "duplicate_client_id",
            "Идентификатор бронирования уже использован".into(),
        ));
    }
    let result = sqlx::query("UPDATE appointments SET patient_id=$2,booking_client_id=$3,status='scheduled',updated_at=now() WHERE id=$1 AND status='available' AND patient_id IS NULL AND doctor_id=(SELECT doctor_id FROM patient_profiles WHERE user_id=$2)")
        .bind(id)
        .bind(patient.id)
        .bind(input.client_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() != 1 {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "appointment_unavailable",
            "Этот интервал уже занят или недоступен".into(),
        ));
    }
    notify(&mut tx, doctor_id, patient.id, id, "appointment_booked").await?;
    let row = sqlx::query_as::<_, Appointment>(&format!("{SELECT} WHERE a.id=$1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(row))
}

#[utoipa::path(
    post, path = "/api/v1/appointments/{id}/cancel", operation_id = "cancel_appointment",
    security(("session_cookie" = [])), params(("id" = Uuid, Path)), request_body = ChangeInput,
    responses((status = 200, body = Appointment), (status = 403, body = crate::ErrorEnvelope), (status = 404, body = crate::ErrorEnvelope), (status = 409, body = crate::ErrorEnvelope), (status = 500, body = crate::ErrorEnvelope))
)]
pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<ChangeInput>,
) -> ApiResult<Json<Appointment>> {
    let user = authenticate(&state, &headers).await?;
    let mut tx = state.db.begin().await?;
    let current = sqlx::query_as::<_, Appointment>(&format!("{SELECT} WHERE a.id=$1"))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if (user.role == Role::Doctor && current.doctor_id != user.id)
        || (user.role == Role::Patient && current.patient_id != Some(user.id))
    {
        return Err(ApiError::forbidden());
    }
    lock_doctor(&mut tx, current.doctor_id).await?;
    let result = sqlx::query("UPDATE appointments SET status='cancelled',version=version+1,updated_at=now() WHERE id=$1 AND version=$2 AND status<>'cancelled'")
        .bind(id).bind(input.expected_version).execute(&mut *tx).await?;
    if result.rows_affected() != 1 {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "stale_version",
            "Приём уже изменён. Обновите расписание".into(),
        ));
    }
    let row = sqlx::query_as::<_, Appointment>(&format!("{SELECT} WHERE a.id=$1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if let Some(patient_id) = current.patient_id {
        let recipient = if user.id == current.doctor_id {
            patient_id
        } else {
            current.doctor_id
        };
        notify(&mut tx, recipient, patient_id, id, "appointment_cancelled").await?;
    }
    tx.commit().await?;
    Ok(Json(row))
}

#[utoipa::path(
    post, path = "/api/v1/appointments/{id}/reschedule", operation_id = "reschedule_appointment",
    security(("session_cookie" = [])), params(("id" = Uuid, Path)), request_body = ChangeInput,
    responses((status = 200, body = Appointment), (status = 403, body = crate::ErrorEnvelope), (status = 404, body = crate::ErrorEnvelope), (status = 409, body = crate::ErrorEnvelope), (status = 422, body = crate::ErrorEnvelope), (status = 500, body = crate::ErrorEnvelope))
)]
pub async fn reschedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<ChangeInput>,
) -> ApiResult<Json<Appointment>> {
    let user = doctor(&state, &headers).await?;
    let starts_at = input
        .starts_at
        .ok_or_else(|| ApiError::invalid("Укажите начало нового интервала"))?;
    let ends_at = input
        .ends_at
        .ok_or_else(|| ApiError::invalid("Укажите окончание нового интервала"))?;
    validate_interval(starts_at, ends_at)?;
    let mut tx = state.db.begin().await?;
    let current = sqlx::query_as::<_, Appointment>(&format!("{SELECT} WHERE a.id=$1"))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if current.doctor_id != user.id {
        return Err(ApiError::forbidden());
    }
    lock_doctor(&mut tx, user.id).await?;
    let conflict: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM appointments WHERE doctor_id=$1 AND id<>$2 AND status<>'cancelled' AND starts_at<$4 AND ends_at>$3)")
        .bind(user.id).bind(id).bind(starts_at).bind(ends_at).fetch_one(&mut *tx).await?;
    if conflict {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "appointment_conflict",
            "Новый интервал пересекается с другим приёмом".into(),
        ));
    }
    let result = sqlx::query("UPDATE appointments SET starts_at=$1,ends_at=$2,version=version+1,updated_at=now() WHERE id=$3 AND version=$4 AND status<>'cancelled'")
        .bind(starts_at).bind(ends_at).bind(id).bind(input.expected_version).execute(&mut *tx).await?;
    if result.rows_affected() != 1 {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "stale_version",
            "Приём уже изменён. Обновите расписание".into(),
        ));
    }
    let row = sqlx::query_as::<_, Appointment>(&format!("{SELECT} WHERE a.id=$1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if let Some(patient_id) = current.patient_id {
        notify(
            &mut tx,
            patient_id,
            patient_id,
            id,
            "appointment_rescheduled",
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(row))
}
