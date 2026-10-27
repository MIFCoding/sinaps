use crate::{ApiError, ApiResult, AppState, PATIENT_QUERY, Patient, authenticate};
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use synapse_domain::Role;
use uuid::Uuid;

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DirectoryQuery {
    search: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    before_name: Option<String>,
    before_id: Option<Uuid>,
}

#[derive(Serialize, utoipa::ToSchema)]
#[schema(as = DirectoryCursor)]
pub struct Cursor {
    name: String,
    id: Uuid,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PatientPage {
    items: Vec<Patient>,
    total_count: i64,
    matching_count: i64,
    #[schema(required = true)]
    next_cursor: Option<Cursor>,
}

#[utoipa::path(
    get, path = "/api/v1/patients/page", operation_id = "list_patients_page",
    security(("session_cookie" = [])),
    params(DirectoryQuery),
    responses(
        (status = 200, description = "Успех", body = PatientPage),
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
    Query(query): Query<DirectoryQuery>,
) -> ApiResult<Json<PatientPage>> {
    let user = authenticate(&state, &headers).await?;
    let limit = query.limit.unwrap_or(20);
    let search = query.search.unwrap_or_default().trim().to_owned();
    if !(1..=50).contains(&limit)
        || search.chars().count() > 120
        || search.chars().any(char::is_control)
        || query.before_name.is_some() != query.before_id.is_some()
        || query
            .before_name
            .as_ref()
            .is_some_and(|name| name.chars().count() > 120 || name.chars().any(char::is_control))
    {
        return Err(ApiError::invalid(
            "Размер страницы: 1–50; поиск: до 120 символов; курсор должен содержать имя и UUID",
        ));
    }
    let status = match query.status.as_deref() {
        None | Some("all") => None,
        Some(value @ ("active" | "observation" | "archived")) => Some(value),
        _ => return Err(ApiError::invalid("Неизвестный статус пациента")),
    };
    // Символы LIKE остаются обычной частью имени, а не пользовательским шаблоном SQL.
    let pattern = if search.is_empty() {
        None
    } else {
        Some(format!(
            "%{}%",
            search
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        ))
    };
    let scope = if user.role == Role::Doctor {
        "p.doctor_id=$1"
    } else {
        "p.user_id=$1 AND p.access_accepted"
    };
    let filters = format!(
        "{scope} AND ($2::text IS NULL OR p.status=$2) AND ($3::text IS NULL OR u.name ILIKE $3)"
    );
    let mut items = sqlx::query_as::<_, Patient>(&format!("{PATIENT_QUERY} WHERE {filters} AND ($4::text IS NULL OR (u.name,u.id)>($4,$5)) ORDER BY u.name,u.id LIMIT $6"))
        .bind(user.id).bind(status).bind(&pattern).bind(&query.before_name).bind(query.before_id).bind(limit+1)
        .fetch_all(&state.db).await?;
    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next_cursor = if has_more {
        items.last().map(|item| Cursor {
            name: item.name.clone(),
            id: item.id,
        })
    } else {
        None
    };
    let matching_count = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM patient_profiles p JOIN users u ON u.id=p.user_id WHERE {filters}"
    ))
    .bind(user.id)
    .bind(status)
    .bind(&pattern)
    .fetch_one(&state.db)
    .await?;
    let total_count = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM patient_profiles p WHERE {scope}"
    ))
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(PatientPage {
        items,
        total_count,
        matching_count,
        next_cursor,
    }))
}
