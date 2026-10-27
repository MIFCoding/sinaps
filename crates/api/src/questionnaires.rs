use crate::{ApiError, ApiResult, AppState, alerts, authenticate, authorize_patient};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, Transaction, types::Json as DbJson};
use std::sync::OnceLock;
use synapse_domain::{Role, gad7};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QuestionnaireDefinition {
    code: String,
    version: String,
    title: String,
    minimum_age: u8,
    instruction: String,
    questions: Vec<String>,
    options: Vec<String>,
    source_url: String,
    scoring_url: String,
    source_sha256: String,
    attribution: String,
    license: String,
    license_url: String,
    limitations: String,
    checked_on: String,
}
fn definition() -> &'static QuestionnaireDefinition {
    static VALUE: OnceLock<QuestionnaireDefinition> = OnceLock::new();
    VALUE.get_or_init(|| {
        serde_json::from_str(include_str!("../../../clinical/gad7.json"))
            .expect("Проверенная версия GAD-7")
    })
}

pub(crate) fn managed_gad7_is_supported(
    spec: &crate::admin::QuestionnaireDraftSpec,
    title: &str,
) -> bool {
    let expected = definition();
    if spec.questions.len() != 7
        || spec.minimum_age != 18
        || title != expected.title
        || spec.instruction != expected.instruction
        || spec.license != expected.license
        || spec.limitations != expected.limitations
        || spec.source_url.as_deref() != Some(expected.source_url.as_str())
        || spec.scoring_url.as_deref() != Some(expected.scoring_url.as_str())
        || spec.source_sha256.as_deref() != Some(expected.source_sha256.as_str())
        || spec.attribution.as_deref() != Some(expected.attribution.as_str())
        || spec.license_url.as_deref() != Some(expected.license_url.as_str())
        || spec.checked_on.as_deref() != Some(expected.checked_on.as_str())
        || spec
            .questions
            .iter()
            .map(|question| question.text.as_str())
            .collect::<Vec<_>>()
            != expected
                .questions
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        || spec
            .bands
            .iter()
            .map(|band| (band.min, band.max))
            .collect::<Vec<_>>()
            != [(0, 4), (5, 9), (10, 14), (15, 21)]
    {
        return false;
    }
    spec.questions.iter().all(|question| {
        question.options.len() == expected.options.len()
            && question.options.iter().enumerate().all(|(index, option)| {
                option.label == expected.options[index] && option.weight == index as i32
            })
    })
}

async fn managed_definition(state: &AppState) -> ApiResult<QuestionnaireDefinition> {
    let stored: Option<(DbJson<crate::admin::QuestionnaireDraftSpec>, String)> = sqlx::query_as(
        "SELECT spec, title FROM custom_questionnaire_versions WHERE draft_id='3ab28556-f1b4-5d19-ae35-4bd7777260ac'::uuid AND active",
    )
    .fetch_optional(&state.db)
    .await?;
    let Some((stored, title)) = stored else {
        let has_published_version: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM custom_questionnaire_versions WHERE draft_id='3ab28556-f1b4-5d19-ae35-4bd7777260ac'::uuid)",
        )
        .fetch_one(&state.db)
        .await?;
        if has_published_version {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "questionnaire_not_published",
                "Для новых назначений администратор должен опубликовать актуальную версию GAD‑7"
                    .into(),
            ));
        }
        return Ok(definition().clone());
    };
    let spec = stored.0;
    let expected = definition();
    if !managed_gad7_is_supported(&spec, &title) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "questionnaire_version_unsupported",
            "Опубликованная версия GAD‑7 отличается от проверенной формы или правил подсчёта"
                .into(),
        ));
    }
    let labels: Vec<String> = spec
        .questions
        .first()
        .map(|question| {
            question
                .options
                .iter()
                .map(|option| option.label.clone())
                .collect()
        })
        .unwrap_or_default();
    Ok(QuestionnaireDefinition {
        code: "gad7".into(),
        version: expected.version.clone(),
        title,
        minimum_age: 18,
        instruction: spec.instruction,
        questions: spec
            .questions
            .into_iter()
            .map(|question| question.text)
            .collect(),
        options: labels,
        source_url: spec
            .source_url
            .unwrap_or_else(|| expected.source_url.clone()),
        scoring_url: spec
            .scoring_url
            .unwrap_or_else(|| expected.scoring_url.clone()),
        source_sha256: spec
            .source_sha256
            .unwrap_or_else(|| expected.source_sha256.clone()),
        attribution: spec
            .attribution
            .unwrap_or_else(|| expected.attribution.clone()),
        license: spec.license,
        license_url: spec
            .license_url
            .unwrap_or_else(|| expected.license_url.clone()),
        limitations: spec.limitations,
        checked_on: spec
            .checked_on
            .unwrap_or_else(|| expected.checked_on.clone()),
    })
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionnaireStatus {
    Assigned,
    Completed,
    Cancelled,
}

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct QuestionnaireAssignment {
    id: Uuid,
    patient_id: Uuid,
    author_name: String,
    scale_code: String,
    scale_version: String,
    #[schema(value_type=QuestionnaireDefinition)]
    definition: DbJson<QuestionnaireDefinition>,
    note: String,
    #[schema(value_type=QuestionnaireStatus)]
    status: String,
    version: i32,
    #[schema(value_type=Option<Vec<u8>>,required=true)]
    answers: Option<DbJson<Vec<u8>>>,
    #[schema(required = true)]
    score: Option<i16>,
    #[schema(value_type=Option<gad7::Gad7Band>,required=true)]
    band: Option<String>,
    created_at: DateTime<Utc>,
    #[schema(required = true)]
    completed_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    cancelled_at: Option<DateTime<Utc>>,
}
const SELECT: &str = "SELECT q.*,u.name AS author_name FROM questionnaire_assignments q JOIN users u ON u.id=q.author_id";
fn conflict(message: &str) -> ApiError {
    ApiError(
        StatusCode::CONFLICT,
        "questionnaire_conflict",
        message.into(),
    )
}
async fn doctor(state: &AppState, headers: &HeaderMap) -> ApiResult<synapse_domain::User> {
    let user = authenticate(state, headers).await?;
    if user.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    Ok(user)
}
async fn lock_patient(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> ApiResult<()> {
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn adult(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> ApiResult<()> {
    let birth: NaiveDate =
        sqlx::query_scalar("SELECT birth_date FROM patient_profiles WHERE user_id=$1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
    if Utc::now().date_naive().years_since(birth).unwrap_or(0) < 18 {
        return Err(ApiError::invalid(
            "Эта версия опросника предназначена для пациентов от 18 лет",
        ));
    }
    Ok(())
}
async fn row(
    tx: &mut Transaction<'_, Postgres>,
    patient: Uuid,
    id: Uuid,
) -> ApiResult<QuestionnaireAssignment> {
    sqlx::query_as(&format!(
        "{SELECT} WHERE q.patient_id=$1 AND q.id=$2 FOR UPDATE OF q"
    ))
    .bind(patient)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(ApiError::not_found)
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    value: &QuestionnaireAssignment,
    author: Uuid,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO questionnaire_changes(id,assignment_id,author_id,version,action,snapshot) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4()).bind(value.id).bind(author).bind(value.version).bind(&value.status).bind(DbJson(value)).execute(&mut **tx).await?;
    Ok(())
}

#[utoipa::path(get,path="/api/v1/questionnaires/gad7",operation_id="questionnaire_definition",security(("session_cookie"=[])),responses((status=200,body=QuestionnaireDefinition),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn template(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<QuestionnaireDefinition>> {
    doctor(&state, &headers).await?;
    Ok(Json(managed_definition(&state).await?))
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in=Query)]
pub struct ListQuery {
    limit: Option<i64>,
    before: Option<DateTime<Utc>>,
    before_id: Option<Uuid>,
    status: Option<String>,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct QuestionnaireCursor {
    before: DateTime<Utc>,
    before_id: Uuid,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct QuestionnairePage {
    items: Vec<QuestionnaireAssignment>,
    #[schema(required = true)]
    next_cursor: Option<QuestionnaireCursor>,
}
#[utoipa::path(get,path="/api/v1/patients/{id}/questionnaires",operation_id="list_questionnaires",params(("id"=Uuid,Path),ListQuery),security(("session_cookie"=[])),responses((status=200,body=QuestionnairePage),(status=400,body=String,content_type="text/plain"),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<QuestionnairePage>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=50).contains(&limit)
        || query.before.is_some() != query.before_id.is_some()
        || query
            .status
            .as_deref()
            .is_some_and(|v| !["assigned", "completed", "cancelled"].contains(&v))
    {
        return Err(ApiError::invalid(
            "Проверьте статус, размер страницы и полный курсор",
        ));
    }
    let mut items=sqlx::query_as::<_,QuestionnaireAssignment>(&format!("{SELECT} WHERE q.patient_id=$1 AND ($2::timestamptz IS NULL OR (q.created_at,q.id)<($2,$3)) AND ($4::text IS NULL OR q.status=$4) ORDER BY q.created_at DESC,q.id DESC LIMIT $5"))
        .bind(id).bind(query.before).bind(query.before_id).bind(query.status).bind(limit+1).fetch_all(&state.db).await?;
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next_cursor = if more {
        items.last().map(|v| QuestionnaireCursor {
            before: v.created_at,
            before_id: v.id,
        })
    } else {
        None
    };
    Ok(Json(QuestionnairePage { items, next_cursor }))
}
#[utoipa::path(get,path="/api/v1/patients/{id}/questionnaires/{assignment_id}",operation_id="get_questionnaire",params(("id"=Uuid,Path),("assignment_id"=Uuid,Path)),security(("session_cookie"=[])),responses((status=200,body=QuestionnaireAssignment),(status=400,body=String,content_type="text/plain"),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, assignment_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<QuestionnaireAssignment>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    Ok(Json(
        sqlx::query_as(&format!("{SELECT} WHERE q.patient_id=$1 AND q.id=$2"))
            .bind(id)
            .bind(assignment_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(ApiError::not_found)?,
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AssignQuestionnaire {
    id: Uuid,
    scale_version: String,
    note: String,
}
#[utoipa::path(post,path="/api/v1/patients/{id}/questionnaires",operation_id="assign_questionnaire",params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=AssignQuestionnaire,security(("session_cookie"=[])),responses((status=201,body=QuestionnaireAssignment),(status=200,body=QuestionnaireAssignment),(status=400,body=String,content_type="text/plain"),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=413,body=String,content_type="text/plain"),(status=415,body=String,content_type="text/plain"),(status=422,content((crate::ErrorEnvelope="application/json"),(String="text/plain"))),(status=500,body=crate::ErrorEnvelope)))]
pub async fn assign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut input): Json<AssignQuestionnaire>,
) -> ApiResult<(StatusCode, Json<QuestionnaireAssignment>)> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    input.note = input.note.trim().to_owned();
    if input.note.chars().count() > 2000 {
        return Err(ApiError::invalid(
            "Комментарий должен быть не длиннее 2000 символов",
        ));
    }
    let managed_definition = managed_definition(&state).await?;
    if input.scale_version != managed_definition.version {
        return Err(conflict("Версия опросника обновилась. Загрузите её заново"));
    }
    let mut tx = state.db.begin().await?;
    lock_patient(&mut tx, id).await?;
    adult(&mut tx, id).await?;
    let existing: Option<QuestionnaireAssignment> =
        sqlx::query_as(&format!("{SELECT} WHERE q.id=$1"))
            .bind(input.id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(existing) = existing {
        if existing.patient_id != id
            || existing.scale_version != input.scale_version
            || existing.note != input.note
        {
            return Err(conflict(
                "Этот идентификатор уже использован для другого назначения",
            ));
        }
        tx.commit().await?;
        return Ok((StatusCode::OK, Json(existing)));
    }
    let pending: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM questionnaire_assignments WHERE patient_id=$1 AND scale_code='gad7' AND status='assigned')").bind(id).fetch_one(&mut *tx).await?;
    if pending {
        return Err(conflict("У пациента уже есть незавершённый GAD‑7"));
    }
    let inserted = sqlx::query("INSERT INTO questionnaire_assignments(id,patient_id,author_id,scale_code,scale_version,definition,note,status,version) VALUES($1,$2,$3,'gad7',$4,$5,$6,'assigned',1) ON CONFLICT(id) DO NOTHING")
        .bind(input.id).bind(id).bind(user.id).bind(&input.scale_version).bind(DbJson(&managed_definition)).bind(input.note).execute(&mut *tx).await?;
    if inserted.rows_affected() == 0 {
        return Err(conflict("Идентификатор уже использован другим назначением"));
    }
    let value = row(&mut tx, id, input.id).await?;
    audit(&mut tx, &value, user.id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(value)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SubmitQuestionnaire {
    version: i32,
    scale_version: String,
    #[schema(min_items = 7, max_items = 7)]
    answers: Vec<u8>,
}
#[utoipa::path(post,path="/api/v1/patients/{id}/questionnaires/{assignment_id}/submit",operation_id="submit_questionnaire",params(("id"=Uuid,Path),("assignment_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=SubmitQuestionnaire,security(("session_cookie"=[])),responses((status=200,body=QuestionnaireAssignment),(status=400,body=String,content_type="text/plain"),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=413,body=String,content_type="text/plain"),(status=415,body=String,content_type="text/plain"),(status=422,content((crate::ErrorEnvelope="application/json"),(String="text/plain"))),(status=500,body=crate::ErrorEnvelope)))]
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, assignment_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<SubmitQuestionnaire>,
) -> ApiResult<Json<QuestionnaireAssignment>> {
    let user = authenticate(&state, &headers).await?;
    if user.role != Role::Patient {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &user, id).await?;
    let (score, band) = gad7::score(&input.answers).map_err(ApiError::invalid)?;
    if input.version < 1 {
        return Err(ApiError::invalid("Некорректная версия назначения"));
    }
    let mut tx = state.db.begin().await?;
    lock_patient(&mut tx, id).await?;
    adult(&mut tx, id).await?;
    let before = row(&mut tx, id, assignment_id).await?;
    if before.scale_version != input.scale_version {
        return Err(conflict("Ответы относятся к другой версии опросника"));
    }
    if before.status == "completed"
        && before.version.checked_sub(1) == Some(input.version)
        && before
            .answers
            .as_ref()
            .is_some_and(|v| v.0 == input.answers)
    {
        tx.commit().await?;
        return Ok(Json(before));
    }
    if before.status != "assigned" || before.version != input.version {
        return Err(conflict(
            "Назначение уже заполнено или отменено. Загрузите актуальные сведения",
        ));
    }
    // Поддержка расчёта привязана к конкретной сохранённой версии, а не к данным клиента.
    if before.scale_code != "gad7" || before.scale_version != "gad7-ru-zolotareva-2023-r1" {
        return Err(conflict("Расчёт этой версии опросника недоступен"));
    }
    let band = serde_json::to_value(band).map_err(|_| ApiError::internal())?;
    sqlx::query("UPDATE questionnaire_assignments SET status='completed',version=version+1,answers=$1,score=$2,band=$3,completed_at=now() WHERE id=$4")
        .bind(DbJson(&input.answers)).bind(score).bind(band.as_str().ok_or_else(ApiError::internal)?).bind(assignment_id).execute(&mut *tx).await?;
    let value = row(&mut tx, id, assignment_id).await?;
    audit(&mut tx, &value, user.id).await?;
    if let Some(completed_at) = value.completed_at {
        alerts::evaluate_questionnaire(&mut tx, id, assignment_id, score, completed_at).await?;
    }
    tx.commit().await?;
    Ok(Json(value))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CancelQuestionnaire {
    version: i32,
}
#[utoipa::path(post,path="/api/v1/patients/{id}/questionnaires/{assignment_id}/cancel",operation_id="cancel_questionnaire",params(("id"=Uuid,Path),("assignment_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=CancelQuestionnaire,security(("session_cookie"=[])),responses((status=200,body=QuestionnaireAssignment),(status=400,body=String,content_type="text/plain"),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=413,body=String,content_type="text/plain"),(status=415,body=String,content_type="text/plain"),(status=422,content((crate::ErrorEnvelope="application/json"),(String="text/plain"))),(status=500,body=crate::ErrorEnvelope)))]
pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, assignment_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<CancelQuestionnaire>,
) -> ApiResult<Json<QuestionnaireAssignment>> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    if input.version < 1 {
        return Err(ApiError::invalid("Некорректная версия назначения"));
    }
    let mut tx = state.db.begin().await?;
    lock_patient(&mut tx, id).await?;
    let before = row(&mut tx, id, assignment_id).await?;
    if before.status == "cancelled" && before.version.checked_sub(1) == Some(input.version) {
        tx.commit().await?;
        return Ok(Json(before));
    }
    if before.status != "assigned" || before.version != input.version {
        return Err(conflict(
            "Можно отменить только актуальное незавершённое назначение",
        ));
    }
    sqlx::query("UPDATE questionnaire_assignments SET status='cancelled',version=version+1,cancelled_at=now() WHERE id=$1").bind(assignment_id).execute(&mut *tx).await?;
    let value = row(&mut tx, id, assignment_id).await?;
    audit(&mut tx, &value, user.id).await?;
    tx.commit().await?;
    Ok(Json(value))
}
#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct QuestionnaireChange {
    version: i32,
    action: String,
    author_name: String,
    created_at: DateTime<Utc>,
    #[schema(value_type=QuestionnaireAssignment)]
    snapshot: serde_json::Value,
}
#[utoipa::path(get,path="/api/v1/patients/{id}/questionnaires/{assignment_id}/changes",operation_id="questionnaire_changes",params(("id"=Uuid,Path),("assignment_id"=Uuid,Path)),security(("session_cookie"=[])),responses((status=200,body=Vec<QuestionnaireChange>),(status=400,body=String,content_type="text/plain"),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, assignment_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Vec<QuestionnaireChange>>> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM questionnaire_assignments WHERE patient_id=$1 AND id=$2)",
    )
    .bind(id)
    .bind(assignment_id)
    .fetch_one(&state.db)
    .await?;
    if !belongs {
        return Err(ApiError::not_found());
    }
    Ok(Json(sqlx::query_as("SELECT c.version,c.action,c.snapshot,c.created_at,u.name AS author_name FROM questionnaire_changes c JOIN users u ON u.id=c.author_id WHERE c.assignment_id=$1 ORDER BY c.version DESC").bind(assignment_id).fetch_all(&state.db).await?))
}
