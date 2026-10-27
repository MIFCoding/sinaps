use crate::{
    ApiError, ApiResult, AppState, DiaryEntry, ObservationEvent, authenticate, authorize_patient,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use synapse_domain::Role;
use uuid::Uuid;

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistoryQuery {
    limit: Option<i64>,
    before: Option<DateTime<Utc>>,
    before_created: Option<DateTime<Utc>>,
    before_id: Option<Uuid>,
    kind: Option<String>,
}

impl HistoryQuery {
    fn validate(&self, max: i64) -> ApiResult<i64> {
        let limit = self.limit.unwrap_or(50);
        let parts = [
            self.before.is_some(),
            self.before_created.is_some(),
            self.before_id.is_some(),
        ];
        if !(1..=max).contains(&limit) || (parts.iter().any(|v| *v) && !parts.iter().all(|v| *v)) {
            return Err(ApiError::invalid(
                "Недопустимый размер страницы или неполный курсор истории",
            ));
        }
        Ok(limit)
    }
}

#[derive(Serialize, utoipa::ToSchema)]
#[schema(as = HistoryCursor)]
pub struct Cursor {
    before: DateTime<Utc>,
    before_created: DateTime<Utc>,
    before_id: Uuid,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct Page<T> {
    items: Vec<T>,
    #[schema(required = true)]
    next_cursor: Option<Cursor>,
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}/diary/page", operation_id = "list_diary_page",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), HistoryQuery),
    responses(
        (status = 200, description = "Успех", body = Page<DiaryEntry>),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn diary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<Page<DiaryEntry>>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let limit = query.validate(365)?;
    if query.kind.is_some() {
        return Err(ApiError::invalid("Фильтр типа применим только к событиям"));
    }
    let mut items = sqlx::query_as::<_, DiaryEntry>("SELECT * FROM diary_entries WHERE patient_id=$1 AND ($2::timestamptz IS NULL OR (measured_at,created_at,id)<($2,$3,$4)) ORDER BY measured_at DESC,created_at DESC,id DESC LIMIT $5")
        .bind(id).bind(query.before).bind(query.before_created).bind(query.before_id).bind(limit+1)
        .fetch_all(&state.db).await?;
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next_cursor = if more {
        items.last().map(|last| Cursor {
            before: last.measured_at,
            before_created: last.created_at,
            before_id: last.id,
        })
    } else {
        None
    };
    Ok(Json(Page { items, next_cursor }))
}

#[utoipa::path(
    get, path = "/api/v1/patients/{id}/events/page", operation_id = "list_events_page",
    security(("session_cookie" = [])),
    params(("id" = Uuid, Path), HistoryQuery),
    responses(
        (status = 200, description = "Успех", body = Page<ObservationEvent>),
        (status = 401, description = "Требуется вход", body = crate::ErrorEnvelope),
        (status = 403, description = "Нет доступа или неверный маркер", body = crate::ErrorEnvelope),
        (status = 422, description = "Некорректные значения", content((crate::ErrorEnvelope = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Ошибка сервера", body = crate::ErrorEnvelope),
        (status = 400, description = "Неверный формат параметров", body = String, content_type = "text/plain")
    )
)]
pub async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<Page<ObservationEvent>>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let limit = query.validate(50)?;
    let kind = match query.kind.as_deref() {
        None | Some("all") => None,
        Some(
            value @ ("initial_assessment" | "consultation" | "plan_change" | "note" | "other"),
        ) => Some(value),
        _ => return Err(ApiError::invalid("Неизвестный тип события")),
    };
    // Проверка видимости и типа выполняется до ограничения страницы.
    let mut items = sqlx::query_as::<_, ObservationEvent>("SELECT e.id,e.title,e.body,e.patient_visible,e.occurred_at,e.created_at,u.name AS author_name,e.kind,e.source FROM observation_events e JOIN users u ON u.id=e.author_id WHERE e.patient_id=$1 AND ($2 OR e.patient_visible) AND ($3::text IS NULL OR e.kind=$3) AND ($4::timestamptz IS NULL OR (e.occurred_at,e.created_at,e.id)<($4,$5,$6)) ORDER BY e.occurred_at DESC,e.created_at DESC,e.id DESC LIMIT $7")
        .bind(id).bind(user.role == Role::Doctor).bind(kind).bind(query.before).bind(query.before_created).bind(query.before_id).bind(limit+1)
        .fetch_all(&state.db).await?;
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next_cursor = if more {
        items.last().map(|last| Cursor {
            before: last.occurred_at,
            before_created: last.created_at,
            before_id: last.id,
        })
    } else {
        None
    };
    Ok(Json(Page { items, next_cursor }))
}
