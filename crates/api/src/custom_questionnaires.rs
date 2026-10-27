use crate::{
    ApiError, ApiResult, AppState,
    admin::{self, QuestionnaireDraftSpec},
    authenticate, authorize_patient,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction, types::Json as DbJson};
use synapse_domain::{Role, User};
use uuid::Uuid;

pub(crate) const PHQ9_DRAFT_ID: &str = "92d53473-7d4c-4fc6-9576-c9f32a0b4d09";
const PHQ9_TITLE: &str = "PHQ‑9: симптомы депрессии";

fn is_phq9(id: Uuid) -> bool {
    id.to_string() == PHQ9_DRAFT_ID
}

fn matches_phq9(spec: &QuestionnaireDraftSpec, title: &str) -> bool {
    title == PHQ9_TITLE
        && serde_json::to_value(spec).ok()
            == serde_json::from_str::<serde_json::Value>(include_str!(
                "../../../clinical/phq9.json"
            ))
            .ok()
}

pub(crate) async fn ensure_builtin_phq9(db: &PgPool) -> Result<(), sqlx::Error> {
    let actor: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users WHERE role='admin' ORDER BY created_at,id LIMIT 1",
    )
    .fetch_optional(db)
    .await?;
    let actor = match actor {
        Some(id) => id,
        None => {
            let Some(id) = sqlx::query_scalar(
                "SELECT id FROM users WHERE role='doctor' ORDER BY created_at,id LIMIT 1",
            )
            .fetch_optional(db)
            .await?
            else {
                return Ok(());
            };
            id
        }
    };
    let draft_id = Uuid::parse_str(PHQ9_DRAFT_ID).expect("Корректный идентификатор PHQ-9");
    let spec: crate::admin::QuestionnaireDraftSpec =
        serde_json::from_str(include_str!("../../../clinical/phq9.json"))
            .expect("Проверенная форма PHQ-9");
    spec.validate()
        .map_err(|_| sqlx::Error::Protocol("Встроенная форма PHQ-9 не прошла проверку".into()))?;
    let mut tx = db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('synapse_builtin_phq9'))")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO catalog_drafts(id,kind,title,source,details,version,author_id) VALUES($1,'questionnaire',$2,$3,$4,1,$5) ON CONFLICT(id) DO NOTHING")
        .bind(draft_id)
        .bind(PHQ9_TITLE)
        .bind(spec.source_url.as_deref().unwrap_or("https://www.phqscreeners.com/select-screener"))
        .bind(&spec.limitations)
        .bind(actor)
        .execute(&mut *tx)
        .await?;
    let draft_kind: Option<String> =
        sqlx::query_scalar("SELECT kind FROM catalog_drafts WHERE id=$1 FOR UPDATE")
            .bind(draft_id)
            .fetch_optional(&mut *tx)
            .await?;
    if draft_kind.as_deref() != Some("questionnaire") {
        return Err(sqlx::Error::Protocol(
            "Идентификатор встроенного PHQ-9 занят другой записью".into(),
        ));
    }
    sqlx::query("UPDATE catalog_drafts SET title=$2 WHERE id=$1 AND title<>$2")
        .bind(draft_id)
        .bind(PHQ9_TITLE)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE custom_questionnaire_versions SET title=$2 WHERE draft_id=$1 AND title<>$2",
    )
    .bind(draft_id)
    .bind(PHQ9_TITLE)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO questionnaire_draft_specs(draft_id,spec) VALUES($1,$2) ON CONFLICT(draft_id) DO NOTHING")
        .bind(draft_id)
        .bind(DbJson(&spec))
        .execute(&mut *tx)
        .await?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM custom_questionnaire_versions WHERE draft_id=$1 AND active)",
    )
    .bind(draft_id)
    .fetch_one(&mut *tx)
    .await?;
    if !active {
        let version: i32 = sqlx::query_scalar("SELECT version FROM catalog_drafts WHERE id=$1")
            .bind(draft_id)
            .fetch_one(&mut *tx)
            .await?;
        let already_published: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_questionnaire_versions WHERE draft_id=$1 AND draft_version=$2)")
            .bind(draft_id)
            .bind(version)
            .fetch_one(&mut *tx)
            .await?;
        if already_published {
            sqlx::query("UPDATE custom_questionnaire_versions SET active=true,archived_at=NULL WHERE draft_id=$1 AND draft_version=$2")
                .bind(draft_id)
                .bind(version)
                .execute(&mut *tx)
                .await?;
        } else {
            let source = spec
                .source_url
                .as_deref()
                .unwrap_or("https://www.phqscreeners.com/select-screener");
            let form: CustomQuestionnaireVersion = sqlx::query_as(&format!("INSERT INTO custom_questionnaire_versions(id,draft_id,draft_version,title,source,spec,published_by) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING {FORM_SELECT}"))
                .bind(Uuid::new_v4())
                .bind(draft_id)
                .bind(version)
                .bind(PHQ9_TITLE)
                .bind(source)
                .bind(DbJson(&spec))
                .bind(actor)
                .fetch_one(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO custom_questionnaire_version_changes(id,form_id,author_id,action) VALUES($1,$2,$3,'published')")
                .bind(Uuid::new_v4())
                .bind(form.id)
                .bind(actor)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

fn conflict(message: &str) -> ApiError {
    ApiError(
        StatusCode::CONFLICT,
        "questionnaire_conflict",
        message.into(),
    )
}

async fn doctor(state: &AppState, headers: &HeaderMap) -> ApiResult<User> {
    let user = authenticate(state, headers).await?;
    if user.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    Ok(user)
}

#[derive(Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct CustomQuestionnaireVersion {
    id: Uuid,
    draft_id: Uuid,
    draft_version: i32,
    title: String,
    source: String,
    #[schema(value_type = QuestionnaireDraftSpec)]
    spec: DbJson<QuestionnaireDraftSpec>,
    active: bool,
    published_at: DateTime<Utc>,
    #[schema(required = true)]
    archived_at: Option<DateTime<Utc>>,
}
const FORM_SELECT: &str =
    "id,draft_id,draft_version,title,source,spec,active,published_at,archived_at";

#[utoipa::path(get,path="/api/v1/admin/questionnaires",operation_id="admin_questionnaire_catalog",security(("session_cookie"=[])),responses((status=200,body=Vec<CustomQuestionnaireVersion>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn admin_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<CustomQuestionnaireVersion>>> {
    admin::require_admin(&state, &headers).await?;
    Ok(Json(sqlx::query_as(&format!("SELECT {FORM_SELECT} FROM custom_questionnaire_versions WHERE active AND draft_id NOT IN ('3ab28556-f1b4-5d19-ae35-4bd7777260ac'::uuid,$1) ORDER BY title,id LIMIT 500")).bind(Uuid::parse_str(PHQ9_DRAFT_ID).map_err(|_| ApiError::internal())?).fetch_all(&state.db).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PublishInput {
    expected_version: i32,
    reviewed: bool,
}

#[utoipa::path(post,path="/api/v1/admin/catalog-drafts/{id}/publish-questionnaire",operation_id="admin_publish_questionnaire",security(("session_cookie"=[])),params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=PublishInput,responses((status=201,body=CustomQuestionnaireVersion),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<PublishInput>,
) -> ApiResult<(StatusCode, Json<CustomQuestionnaireVersion>)> {
    let user = admin::require_admin(&state, &headers).await?;
    if !input.reviewed {
        return Err(ApiError::invalid(
            "Подтвердите проверку источника и прав использования",
        ));
    }
    let mut tx = state.db.begin().await?;
    let row: Option<(i32, String, String, Option<DbJson<QuestionnaireDraftSpec>>)> = sqlx::query_as("SELECT d.version,d.title,d.source,s.spec FROM catalog_drafts d LEFT JOIN questionnaire_draft_specs s ON s.draft_id=d.id WHERE d.id=$1 AND d.kind='questionnaire' FOR UPDATE OF d")
        .bind(id).fetch_optional(&mut *tx).await?;
    let (version, title, source, spec) = row.ok_or_else(ApiError::not_found)?;
    if version != input.expected_version {
        return Err(conflict(
            "Черновик изменился. Обновите его перед публикацией",
        ));
    }
    if source.trim().is_empty() {
        return Err(ApiError::invalid("Укажите источник опросника"));
    }
    let spec = spec.ok_or_else(|| ApiError::invalid("Сначала сохраните форму опросника"))?;
    spec.0.validate()?;
    if id == uuid::uuid!("3ab28556-f1b4-5d19-ae35-4bd7777260ac")
        && !crate::questionnaires::managed_gad7_is_supported(&spec.0, &title)
    {
        return Err(ApiError::invalid(
            "Нельзя публиковать GAD‑7 с содержанием или правилами, отличающимися от проверенной версии",
        ));
    }
    if is_phq9(id) && !matches_phq9(&spec.0, &title) {
        return Err(ApiError::invalid(
            "Содержимое встроенного PHQ‑9 нельзя изменять",
        ));
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_questionnaire_versions WHERE draft_id=$1 AND draft_version=$2)").bind(id).bind(version).fetch_one(&mut *tx).await?;
    if exists {
        return Err(conflict(
            "Эта версия уже опубликована; измените черновик для новой версии",
        ));
    }
    let old: Vec<Uuid> = sqlx::query_scalar("UPDATE custom_questionnaire_versions SET active=false,archived_at=now() WHERE draft_id=$1 AND active RETURNING id")
        .bind(id).fetch_all(&mut *tx).await?;
    for form_id in old {
        sqlx::query("INSERT INTO custom_questionnaire_version_changes(id,form_id,author_id,action) VALUES($1,$2,$3,'archived')").bind(Uuid::new_v4()).bind(form_id).bind(user.id).execute(&mut *tx).await?;
    }
    let form: CustomQuestionnaireVersion = sqlx::query_as(&format!("INSERT INTO custom_questionnaire_versions(id,draft_id,draft_version,title,source,spec,published_by) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING {FORM_SELECT}"))
        .bind(Uuid::new_v4()).bind(id).bind(version).bind(title).bind(source).bind(spec).bind(user.id).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO custom_questionnaire_version_changes(id,form_id,author_id,action) VALUES($1,$2,$3,'published')").bind(Uuid::new_v4()).bind(form.id).bind(user.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(form)))
}

#[utoipa::path(post,path="/api/v1/admin/questionnaires/{id}/archive",operation_id="admin_archive_questionnaire",security(("session_cookie"=[])),params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)),responses((status=200,body=CustomQuestionnaireVersion),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CustomQuestionnaireVersion>> {
    let user = admin::require_admin(&state, &headers).await?;
    let phq9_id = Uuid::parse_str(PHQ9_DRAFT_ID).map_err(|_| ApiError::internal())?;
    let is_builtin: bool =
        sqlx::query_scalar("SELECT draft_id=$2 FROM custom_questionnaire_versions WHERE id=$1")
            .bind(id)
            .bind(phq9_id)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(false);
    if is_builtin {
        return Err(ApiError::invalid("Встроенный PHQ‑9 нельзя архивировать"));
    }
    let mut tx = state.db.begin().await?;
    let row: Option<CustomQuestionnaireVersion> = sqlx::query_as(&format!("UPDATE custom_questionnaire_versions SET active=false,archived_at=now() WHERE id=$1 AND active RETURNING {FORM_SELECT}"))
        .bind(id).fetch_optional(&mut *tx).await?;
    let row = row.ok_or_else(|| conflict("Версия не найдена или уже архивирована"))?;
    sqlx::query("INSERT INTO custom_questionnaire_version_changes(id,form_id,author_id,action) VALUES($1,$2,$3,'archived')").bind(Uuid::new_v4()).bind(id).bind(user.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(row))
}

#[utoipa::path(get,path="/api/v1/questionnaires/catalog",operation_id="custom_questionnaire_catalog",security(("session_cookie"=[])),responses((status=200,body=Vec<CustomQuestionnaireVersion>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<CustomQuestionnaireVersion>>> {
    doctor(&state, &headers).await?;
    Ok(Json(sqlx::query_as(&format!("SELECT {FORM_SELECT} FROM custom_questionnaire_versions WHERE active ORDER BY title,id LIMIT 500")).fetch_all(&state.db).await?))
}

#[derive(Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct CustomQuestionnaireAssignment {
    id: Uuid,
    patient_id: Uuid,
    author_name: String,
    form_id: Uuid,
    draft_id: Uuid,
    title: String,
    source: String,
    #[schema(value_type = QuestionnaireDraftSpec)]
    spec: DbJson<QuestionnaireDraftSpec>,
    note: String,
    status: String,
    version: i32,
    #[schema(value_type = Option<Vec<u8>>, required = true)]
    answers: Option<DbJson<Vec<u8>>>,
    #[schema(required = true)]
    score: Option<i32>,
    #[schema(required = true)]
    band: Option<String>,
    created_at: DateTime<Utc>,
    #[schema(required = true)]
    completed_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    cancelled_at: Option<DateTime<Utc>>,
}
const ASSIGN_SELECT: &str = "SELECT a.*,u.name AS author_name FROM custom_questionnaire_assignments a JOIN users u ON u.id=a.author_id";

async fn assignment(
    tx: &mut Transaction<'_, Postgres>,
    patient: Uuid,
    id: Uuid,
) -> ApiResult<CustomQuestionnaireAssignment> {
    sqlx::query_as(&format!(
        "{ASSIGN_SELECT} WHERE a.patient_id=$1 AND a.id=$2 FOR UPDATE OF a"
    ))
    .bind(patient)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(ApiError::not_found)
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    user: &User,
    value: &CustomQuestionnaireAssignment,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO custom_questionnaire_changes(id,assignment_id,author_id,version,action,snapshot) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4()).bind(value.id).bind(user.id).bind(value.version).bind(&value.status).bind(DbJson(value)).execute(&mut **tx).await?;
    Ok(())
}
async fn check_age(tx: &mut Transaction<'_, Postgres>, patient: Uuid, min: u8) -> ApiResult<()> {
    let birth: NaiveDate =
        sqlx::query_scalar("SELECT birth_date FROM patient_profiles WHERE user_id=$1")
            .bind(patient)
            .fetch_one(&mut **tx)
            .await?;
    if Utc::now().date_naive().years_since(birth).unwrap_or(0) < u32::from(min) {
        return Err(ApiError::invalid(format!(
            "Опросник доступен пациентам от {min} лет"
        )));
    }
    Ok(())
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in=Query)]
pub struct PageQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct CustomQuestionnairePage {
    items: Vec<CustomQuestionnaireAssignment>,
    #[schema(required = true)]
    next_offset: Option<i64>,
}

#[utoipa::path(get,path="/api/v1/patients/{id}/custom-questionnaires",operation_id="list_custom_questionnaires",security(("session_cookie"=[])),params(("id"=Uuid,Path),PageQuery),responses((status=200,body=CustomQuestionnairePage),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<CustomQuestionnairePage>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let limit = query.limit.unwrap_or(30);
    let offset = query.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || !(0..=100_000).contains(&offset) {
        return Err(ApiError::invalid("Проверьте параметры страницы"));
    }
    let mut items: Vec<CustomQuestionnaireAssignment> = sqlx::query_as(&format!("{ASSIGN_SELECT} WHERE a.patient_id=$1 ORDER BY a.created_at DESC,a.id DESC LIMIT $2 OFFSET $3"))
        .bind(id).bind(limit+1).bind(offset).fetch_all(&state.db).await?;
    let next_offset = if items.len() > limit as usize {
        items.truncate(limit as usize);
        Some(offset + limit)
    } else {
        None
    };
    Ok(Json(CustomQuestionnairePage { items, next_offset }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CustomAssignInput {
    id: Uuid,
    form_id: Uuid,
    note: String,
}
#[utoipa::path(post,path="/api/v1/patients/{id}/custom-questionnaires",operation_id="assign_custom_questionnaire",security(("session_cookie"=[])),params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=CustomAssignInput,responses((status=201,body=CustomQuestionnaireAssignment),(status=200,body=CustomQuestionnaireAssignment),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn assign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<CustomAssignInput>,
) -> ApiResult<(StatusCode, Json<CustomQuestionnaireAssignment>)> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let note = input.note.trim();
    if note.chars().count() > 2000
        || note
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(ApiError::invalid("Проверьте комментарий"));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let existing: Option<CustomQuestionnaireAssignment> =
        sqlx::query_as(&format!("{ASSIGN_SELECT} WHERE a.id=$1"))
            .bind(input.id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(value) = existing {
        if value.patient_id != id || value.form_id != input.form_id || value.note != note {
            return Err(conflict("Идентификатор использован для другого назначения"));
        }
        tx.commit().await?;
        return Ok((StatusCode::OK, Json(value)));
    }
    let form: Option<CustomQuestionnaireVersion> = sqlx::query_as(&format!(
        "SELECT {FORM_SELECT} FROM custom_questionnaire_versions WHERE id=$1 AND active FOR SHARE"
    ))
    .bind(input.form_id)
    .fetch_optional(&mut *tx)
    .await?;
    let form = form.ok_or_else(|| conflict("Опросник больше не доступен для назначения"))?;
    check_age(&mut tx, id, form.spec.0.minimum_age).await?;
    let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_questionnaire_assignments WHERE patient_id=$1 AND draft_id=$2 AND status='assigned')").bind(id).bind(form.draft_id).fetch_one(&mut *tx).await?;
    if pending {
        return Err(conflict(
            "У пациента уже есть незавершённое назначение этого опросника",
        ));
    }
    let inserted = sqlx::query("INSERT INTO custom_questionnaire_assignments(id,patient_id,author_id,form_id,draft_id,title,source,spec,note,status,version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'assigned',1) ON CONFLICT DO NOTHING")
        .bind(input.id).bind(id).bind(user.id).bind(form.id).bind(form.draft_id).bind(form.title).bind(form.source).bind(form.spec).bind(note).execute(&mut *tx).await?;
    if inserted.rows_affected() == 0 {
        return Err(conflict(
            "Назначение уже существует или идентификатор занят",
        ));
    }
    let value = assignment(&mut tx, id, input.id).await?;
    audit(&mut tx, &user, &value).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(value)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CustomSubmitInput {
    version: i32,
    answers: Vec<u8>,
}
#[utoipa::path(post,path="/api/v1/patients/{id}/custom-questionnaires/{assignment_id}/submit",operation_id="submit_custom_questionnaire",security(("session_cookie"=[])),params(("id"=Uuid,Path),("assignment_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=CustomSubmitInput,responses((status=200,body=CustomQuestionnaireAssignment),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, assignment_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<CustomSubmitInput>,
) -> ApiResult<Json<CustomQuestionnaireAssignment>> {
    let user = authenticate(&state, &headers).await?;
    if user.role != Role::Patient || user.id != id {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &user, id).await?;
    let mut tx = state.db.begin().await?;
    let before = assignment(&mut tx, id, assignment_id).await?;
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
        return Err(conflict("Назначение уже изменено. Обновите данные"));
    }
    let spec = &before.spec.0;
    check_age(&mut tx, id, spec.minimum_age).await?;
    if input.answers.len() != spec.questions.len() {
        return Err(ApiError::invalid("Ответьте на все вопросы"));
    }
    let mut score = 0;
    for (q, answer) in spec.questions.iter().zip(&input.answers) {
        score += q
            .options
            .get(*answer as usize)
            .ok_or_else(|| ApiError::invalid("Выбран недопустимый ответ"))?
            .weight;
    }
    let band = spec
        .bands
        .iter()
        .find(|b| score >= b.min && score <= b.max)
        .map(|b| b.label.as_str());
    sqlx::query("UPDATE custom_questionnaire_assignments SET status='completed',version=version+1,answers=$1,score=$2,band=$3,completed_at=now() WHERE id=$4")
        .bind(DbJson(&input.answers)).bind(score).bind(band).bind(assignment_id).execute(&mut *tx).await?;
    let value = assignment(&mut tx, id, assignment_id).await?;
    audit(&mut tx, &user, &value).await?;
    if is_phq9(before.draft_id) && input.answers.get(8).is_some_and(|answer| *answer > 0) {
        if let Some(completed_at) = value.completed_at {
            crate::alerts::evaluate_phq9_item(&mut tx, id, assignment_id, completed_at).await?;
        }
    }
    crate::clinical_recommendations::propose_phq9_safety_assessment(
        &mut tx,
        id,
        assignment_id,
        before.draft_id,
        &input.answers,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(value))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CustomCancelInput {
    version: i32,
}
#[utoipa::path(post,path="/api/v1/patients/{id}/custom-questionnaires/{assignment_id}/cancel",operation_id="cancel_custom_questionnaire",security(("session_cookie"=[])),params(("id"=Uuid,Path),("assignment_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=CustomCancelInput,responses((status=200,body=CustomQuestionnaireAssignment),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, assignment_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<CustomCancelInput>,
) -> ApiResult<Json<CustomQuestionnaireAssignment>> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let mut tx = state.db.begin().await?;
    let before = assignment(&mut tx, id, assignment_id).await?;
    if before.status == "cancelled" && before.version.checked_sub(1) == Some(input.version) {
        tx.commit().await?;
        return Ok(Json(before));
    }
    if before.status != "assigned" || before.version != input.version {
        return Err(conflict(
            "Можно отменить только актуальное незавершённое назначение",
        ));
    }
    sqlx::query("UPDATE custom_questionnaire_assignments SET status='cancelled',version=version+1,cancelled_at=now() WHERE id=$1").bind(assignment_id).execute(&mut *tx).await?;
    let value = assignment(&mut tx, id, assignment_id).await?;
    audit(&mut tx, &user, &value).await?;
    tx.commit().await?;
    Ok(Json(value))
}

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct CustomQuestionnaireChange {
    version: i32,
    action: String,
    author_name: String,
    created_at: DateTime<Utc>,
    #[schema(value_type = CustomQuestionnaireAssignment)]
    snapshot: serde_json::Value,
}

#[utoipa::path(get,path="/api/v1/patients/{id}/custom-questionnaires/{assignment_id}/changes",operation_id="custom_questionnaire_changes",security(("session_cookie"=[])),params(("id"=Uuid,Path),("assignment_id"=Uuid,Path)),responses((status=200,body=Vec<CustomQuestionnaireChange>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, assignment_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Vec<CustomQuestionnaireChange>>> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_questionnaire_assignments WHERE id=$1 AND patient_id=$2)")
        .bind(assignment_id).bind(id).fetch_one(&state.db).await?;
    if !exists {
        return Err(ApiError::not_found());
    }
    let rows = sqlx::query_as("SELECT c.version,c.action,c.created_at,c.snapshot,u.name AS author_name FROM custom_questionnaire_changes c JOIN users u ON u.id=c.author_id WHERE c.assignment_id=$1 ORDER BY c.version DESC")
        .bind(assignment_id).fetch_all(&state.db).await?;
    Ok(Json(rows))
}
