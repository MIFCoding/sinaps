use crate::delivery::PushMessage;
use crate::{ApiError, ApiResult, AppState, authenticate};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct Notification {
    id: Uuid,
    patient_id: Uuid,
    patient_name: String,
    kind: String,
    related_id: Uuid,
    created_at: DateTime<Utc>,
    #[schema(required = true)]
    read_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, utoipa::ToSchema)]
#[schema(as = NotificationCursor)]
pub struct Cursor {
    before: DateTime<Utc>,
    before_id: Uuid,
}

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    limit: Option<i64>,
    before: Option<DateTime<Utc>>,
    before_id: Option<Uuid>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct NotificationPage {
    items: Vec<Notification>,
    unread_count: i64,
    unread_messages_count: i64,
    #[schema(required = true)]
    next_cursor: Option<Cursor>,
}

#[utoipa::path(
    get, path = "/api/v1/notifications", operation_id = "list_notifications",
    security(("session_cookie" = [])),
    params(ListQuery),
    responses(
        (status = 200, description = "Успех", body = NotificationPage),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<NotificationPage>> {
    let user = authenticate(&state, &headers).await?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) || query.before.is_some() != query.before_id.is_some() {
        return Err(ApiError::invalid(
            "Размер страницы: от 1 до 50; курсор должен содержать дату и идентификатор",
        ));
    }
    let mut items = sqlx::query_as::<_, Notification>("SELECT n.id,n.patient_id,u.name AS patient_name,n.kind,n.related_id,n.created_at,n.read_at FROM notifications n JOIN users u ON u.id=n.patient_id WHERE n.recipient_id=$1 AND ($2::timestamptz IS NULL OR (n.created_at,n.id)<($2,$3)) ORDER BY n.created_at DESC,n.id DESC LIMIT $4")
        .bind(user.id).bind(query.before).bind(query.before_id).bind(limit + 1).fetch_all(&state.db).await?;
    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next_cursor = if has_more {
        items.last().map(|last| Cursor {
            before: last.created_at,
            before_id: last.id,
        })
    } else {
        None
    };
    let unread_count = sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE recipient_id=$1 AND read_at IS NULL",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    let unread_messages_count = sqlx::query_scalar(
        "SELECT count(*) FROM notifications
         WHERE recipient_id=$1 AND kind='chat_message' AND read_at IS NULL",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(NotificationPage {
        items,
        unread_count,
        unread_messages_count,
        next_cursor,
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReadResult {
    read_at: DateTime<Utc>,
}

#[utoipa::path(
    post, path = "/api/v1/notifications/{id}/read", operation_id = "read_notification",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), ("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    responses(
        (status = 200, description = "Успех", body = ReadResult),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 404, description = "Запись не найдена", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ReadResult>> {
    let user = authenticate(&state, &headers).await?;
    let read_at = sqlx::query_scalar("UPDATE notifications SET read_at=COALESCE(read_at,now()) WHERE id=$1 AND recipient_id=$2 RETURNING read_at")
        .bind(id).bind(user.id).fetch_optional(&state.db).await?
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "not_found", "Уведомление не найдено".into()))?;
    Ok(Json(ReadResult { read_at }))
}

#[utoipa::path(
    post, path = "/api/v1/notifications/read-all", operation_id = "read_all_notifications",
    security(("session_cookie" = [])),
    params(("X-Synapse-Request" = String, Header, description = "Маркер изменяющего запроса: 1")),
    responses(
        (status = 204, description = "Выполнено без тела ответа"),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn read_all(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<StatusCode> {
    let user = authenticate(&state, &headers).await?;
    sqlx::query("UPDATE notifications SET read_at=now() WHERE recipient_id=$1 AND read_at IS NULL")
        .bind(user.id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn diary_added(
    tx: &mut Transaction<'_, Postgres>,
    patient: Uuid,
    entry: Uuid,
) -> ApiResult<()> {
    // Событие и уведомление сохраняются одной транзакцией; текст дневника не копируется.
    sqlx::query("INSERT INTO notifications(id,recipient_id,patient_id,kind,related_id) SELECT $1,doctor_id,user_id,'diary_entry',$3 FROM patient_profiles WHERE user_id=$2 ON CONFLICT (recipient_id,kind,related_id) DO NOTHING")
        .bind(Uuid::new_v4()).bind(patient).bind(entry).execute(&mut **tx).await?;
    Ok(())
}

pub async fn event_published(
    tx: &mut Transaction<'_, Postgres>,
    patient: Uuid,
    event: Uuid,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO notifications(id,recipient_id,patient_id,kind,related_id) SELECT $1,$2,$2,'published_event',$3 FROM patient_profiles WHERE user_id=$2 AND access_accepted ON CONFLICT (recipient_id,kind,related_id) DO NOTHING")
        .bind(Uuid::new_v4()).bind(patient).bind(event).execute(&mut **tx).await?;
    Ok(())
}

#[derive(FromRow)]
struct PushQueueRow {
    notification_id: Uuid,
    recipient_id: Uuid,
    kind: String,
    created_at: DateTime<Utc>,
    attempts: i32,
}

pub async fn dispatch_push(state: AppState) {
    loop {
        let mut tx = match state.db.begin().await {
            Ok(tx) => tx,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };
        let rows = match sqlx::query_as::<_, PushQueueRow>(
            "SELECT o.notification_id,n.recipient_id,n.kind,n.created_at,o.attempts+1 AS attempts
             FROM notification_push_outbox o JOIN notifications n ON n.id=o.notification_id
             WHERE o.delivered_at IS NULL AND o.attempts<12 AND o.next_attempt_at<=now()
               AND (o.lease_until IS NULL OR o.lease_until<now())
             ORDER BY o.created_at LIMIT 25 FOR UPDATE OF o SKIP LOCKED",
        )
        .fetch_all(&mut *tx)
        .await
        {
            Ok(rows) => rows,
            Err(_) => {
                drop(tx);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };
        for row in &rows {
            if sqlx::query("UPDATE notification_push_outbox SET attempts=attempts+1,lease_until=now()+interval '30 seconds' WHERE notification_id=$1")
                .bind(row.notification_id).execute(&mut *tx).await.is_err()
            {
                tracing::error!("Не удалось зарезервировать push-уведомление");
            }
        }
        if tx.commit().await.is_err() {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }
        if rows.is_empty() {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }
        for row in rows {
            let message = PushMessage {
                notification_id: row.notification_id.to_string(),
                recipient_id: row.recipient_id.to_string(),
                kind: row.kind,
                created_at: row.created_at.to_rfc3339(),
            };
            let result = state.delivery.send_push(message).await;
            let persisted = if result.is_ok() {
                sqlx::query("UPDATE notification_push_outbox SET delivered_at=now(),lease_until=NULL WHERE notification_id=$1")
                    .bind(row.notification_id).execute(&state.db).await
            } else {
                let backoff = (5_i32 * 2_i32.pow(row.attempts.min(6) as u32)).min(300);
                sqlx::query("UPDATE notification_push_outbox SET lease_until=NULL,next_attempt_at=now()+($2 * interval '1 second') WHERE notification_id=$1")
                    .bind(row.notification_id).bind(backoff).execute(&state.db).await
            };
            if persisted.is_err() {
                tracing::error!("Не удалось сохранить результат push-доставки");
            }
            if result.is_err() {
                tracing::warn!("Push-шлюз не принял уведомление");
            }
        }
    }
}
