use crate::{ApiError, ApiResult, AppState, authenticate, authorize_patient};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct ChatMessage {
    id: Uuid,
    client_id: Uuid,
    patient_id: Uuid,
    sender_id: Uuid,
    sender_name: String,
    sender_role: String,
    body: String,
    created_at: DateTime<Utc>,
    #[schema(required = true)]
    read_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ChatPage {
    items: Vec<ChatMessage>,
    unread_count: i64,
    #[schema(required = true)]
    next_cursor: Option<ChatCursor>,
}

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct ChatThread {
    patient_id: Uuid,
    patient_name: String,
    unread_count: i64,
}

#[utoipa::path(
    get, path = "/api/v1/chat/threads", operation_id = "list_chat_threads",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Диалоги врача", body = [ChatThread]),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope)
    )
)]
pub async fn threads(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<ChatThread>>> {
    let doctor = authenticate(&state, &headers).await?;
    if doctor.role != synapse_domain::Role::Doctor {
        return Err(ApiError::forbidden());
    }
    let values = sqlx::query_as(
        "SELECT p.user_id AS patient_id, u.name AS patient_name,
            count(m.id) FILTER (WHERE m.sender_id<>$1 AND m.read_at IS NULL) AS unread_count
         FROM patient_profiles p
         JOIN users u ON u.id=p.user_id
         LEFT JOIN chat_messages m ON m.patient_id=p.user_id
         WHERE p.doctor_id=$1
         GROUP BY p.user_id,u.id,u.name
         ORDER BY (count(m.id) FILTER (WHERE m.sender_id<>$1 AND m.read_at IS NULL)) DESC,
                  u.name,u.id",
    )
    .bind(doctor.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(values))
}

#[derive(Serialize, Deserialize, utoipa::ToSchema, utoipa::IntoParams, Clone)]
#[into_params(parameter_in = Query)]
pub struct ChatCursor {
    before: Option<DateTime<Utc>>,
    before_id: Option<Uuid>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SendInput {
    client_id: Uuid,
    body: String,
}

#[derive(Serialize, utoipa::ToSchema)]
#[schema(as = ChatReadResult)]
pub struct ReadResult {
    read_count: i64,
}

fn clean_body(body: &str) -> ApiResult<String> {
    let value = body.trim();
    if value.is_empty()
        || value.chars().count() > 4000
        || value
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(ApiError::invalid(
            "Сообщение должно содержать от 1 до 4000 символов",
        ));
    }
    Ok(value.to_owned())
}

async fn access(state: &AppState, headers: &HeaderMap, patient_id: Uuid) -> ApiResult<crate::User> {
    let user = authenticate(state, headers).await?;
    authorize_patient(state, &user, patient_id).await?;
    Ok(user)
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}/messages", operation_id = "list_messages",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ChatCursor),
    responses(
        (status = 200, description = "История диалога", body = ChatPage),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректный курсор", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(cursor): Query<ChatCursor>,
) -> ApiResult<Json<ChatPage>> {
    let user = access(&state, &headers, id).await?;
    if cursor.before.is_some() != cursor.before_id.is_some() {
        return Err(ApiError::invalid(
            "Курсор должен содержать дату и идентификатор сообщения",
        ));
    }
    let mut items = sqlx::query_as::<_, ChatMessage>(
        "SELECT m.id,m.client_id,m.patient_id,m.sender_id,u.name AS sender_name,
            u.role AS sender_role,m.body,m.created_at,m.read_at
         FROM chat_messages m JOIN users u ON u.id=m.sender_id
         WHERE m.patient_id=$1
           AND ($2::timestamptz IS NULL OR (m.created_at,m.id)<($2,$3))
         ORDER BY m.created_at DESC,m.id DESC LIMIT 51",
    )
    .bind(id)
    .bind(cursor.before)
    .bind(cursor.before_id)
    .fetch_all(&state.db)
    .await?;
    let has_more = items.len() > 50;
    items.truncate(50);
    let next_cursor = if has_more {
        items.last().map(|message| ChatCursor {
            before: Some(message.created_at),
            before_id: Some(message.id),
        })
    } else {
        None
    };
    let unread_count = sqlx::query_scalar(
        "SELECT count(*) FROM chat_messages m
         WHERE m.patient_id=$1 AND m.sender_id<>$2 AND m.read_at IS NULL",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(ChatPage {
        items,
        unread_count,
        next_cursor,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/patients/{id}/messages", operation_id = "send_message",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ("X-Synapse-Request" = String, Header)),
    request_body = SendInput,
    responses(
        (status = 201, description = "Сообщение создано", body = ChatMessage),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа", body = crate::ErrorEnvelope),
        (status = 409, description = "Идентификатор уже использован", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректное сообщение", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope)
    )
)]
pub async fn send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<SendInput>,
) -> ApiResult<(StatusCode, Json<ChatMessage>)> {
    let user = access(&state, &headers, id).await?;
    let body = clean_body(&input.body)?;
    let mut tx = state.db.begin().await?;
    let inserted = sqlx::query_as::<_, ChatMessage>(
        "INSERT INTO chat_messages(id,client_id,patient_id,sender_id,body)
         VALUES($1,$2,$3,$4,$5)
         ON CONFLICT (client_id) DO NOTHING
         RETURNING id,client_id,patient_id,sender_id,
           (SELECT name FROM users WHERE id=chat_messages.sender_id) AS sender_name,
           (SELECT role FROM users WHERE id=chat_messages.sender_id) AS sender_role,
           body,created_at,read_at",
    )
    .bind(Uuid::new_v4())
    .bind(input.client_id)
    .bind(id)
    .bind(user.id)
    .bind(&body)
    .fetch_optional(&mut *tx)
    .await?;
    let message = if let Some(message) = inserted {
        sqlx::query(
            "INSERT INTO notifications(id,recipient_id,patient_id,kind,related_id)
             SELECT $1,
                    CASE WHEN p.doctor_id=$3 THEN p.user_id ELSE p.doctor_id END,
                    p.user_id,'chat_message',$2
             FROM patient_profiles p
             WHERE p.user_id=$4
             ON CONFLICT (recipient_id,kind,related_id) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(message.id)
        .bind(user.id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        message
    } else {
        let existing = sqlx::query_as::<_, ChatMessage>(
            "SELECT m.id,m.client_id,m.patient_id,m.sender_id,u.name AS sender_name,
                u.role AS sender_role,m.body,m.created_at,m.read_at
             FROM chat_messages m JOIN users u ON u.id=m.sender_id
             WHERE m.client_id=$1",
        )
        .bind(input.client_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::internal)?;
        if existing.patient_id != id || existing.sender_id != user.id || existing.body != body {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "message_conflict",
                "Идентификатор сообщения уже использован".into(),
            ));
        }
        existing
    };
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(message)))
}

#[utoipa::path(
    post, path = "/api/v1/patients/{id}/messages/read", operation_id = "read_messages",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ("X-Synapse-Request" = String, Header)),
    responses(
        (status = 200, description = "Сообщения отмечены прочитанными", body = ReadResult),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope)
    )
)]
pub async fn read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ReadResult>> {
    let user = access(&state, &headers, id).await?;
    let result = sqlx::query(
        "UPDATE chat_messages SET read_at=now()
         WHERE patient_id=$1 AND sender_id<>$2 AND read_at IS NULL",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?;
    sqlx::query(
        "UPDATE notifications n SET read_at=now()
         WHERE n.recipient_id=$2 AND n.kind='chat_message' AND n.read_at IS NULL
           AND EXISTS (
             SELECT 1 FROM chat_messages m
             WHERE m.id=n.related_id AND m.patient_id=$1 AND m.sender_id<>$2
           )",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?;
    Ok(Json(ReadResult {
        read_count: result.rows_affected() as i64,
    }))
}
