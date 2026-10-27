use crate::{ApiError, ApiResult, AppState, authenticate_any, registration, seed};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction, types::Json as DbJson};
use synapse_domain::{Role, User};
use uuid::Uuid;

pub async fn create_admin(db: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let email = synapse_domain::normalize_login(&std::env::var("SYNAPSE_ADMIN_EMAIL")?);
    let password = std::env::var("SYNAPSE_ADMIN_PASSWORD")?;
    let name = std::env::var("SYNAPSE_ADMIN_NAME").unwrap_or_else(|_| "Администратор".into());
    let name = name.trim();
    if !registration::valid_email_address(&email)
        || name.is_empty()
        || name.chars().count() > 120
        || name.chars().any(char::is_control)
    {
        return Err("Укажите корректный email и имя администратора".into());
    }
    synapse_domain::validate_new_password(&password)
        .map_err(|e| format!("Некорректный пароль администратора: {e}"))?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE role='admin')")
        .fetch_one(db)
        .await?;
    if exists {
        return Err("Учётная запись администратора уже существует".into());
    }
    let hash = tokio::task::spawn_blocking(move || seed::hash(&password)).await?;
    sqlx::query("INSERT INTO users(id,email,name,role,password_hash,activated,email_verified) VALUES($1,$2,$3,'admin',$4,true,true)")
        .bind(Uuid::new_v4()).bind(email).bind(name).bind(hash).execute(db).await?;
    println!("Учётная запись администратора создана.");
    Ok(())
}

pub(crate) async fn require_admin(state: &AppState, headers: &HeaderMap) -> ApiResult<User> {
    let user = authenticate_any(state, headers).await?;
    if user.role != Role::Admin {
        return Err(ApiError::forbidden());
    }
    Ok(user)
}

#[derive(Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct CatalogDraft {
    pub(crate) id: Uuid,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) source: String,
    pub(crate) details: String,
    pub(crate) version: i32,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DraftInput {
    kind: String,
    title: String,
    source: String,
    details: String,
}

impl DraftInput {
    fn validated(mut self) -> ApiResult<Self> {
        self.title = self.title.trim().to_owned();
        self.source = self.source.trim().to_owned();
        self.details = self.details.trim().to_owned();
        if !matches!(
            self.kind.as_str(),
            "questionnaire" | "disease" | "finding" | "drug" | "adverse_effect"
        ) || self.title.is_empty()
            || self.title.chars().count() > 200
            || self.source.chars().count() > 2000
            || self.details.chars().count() > 20000
            || self.title.chars().any(char::is_control)
            || self
                .source
                .chars()
                .chain(self.details.chars())
                .any(|c| c.is_control() && c != '\n' && c != '\t' && c != '\r')
        {
            return Err(ApiError::invalid(
                "Проверьте тип, название и длину полей черновика",
            ));
        }
        Ok(self)
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateDraftInput {
    expected_version: i32,
    #[serde(flatten)]
    draft: DraftInput,
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    user: &User,
    draft: &CatalogDraft,
) -> ApiResult<()> {
    let spec: Option<DbJson<serde_json::Value>> =
        sqlx::query_scalar("SELECT spec FROM questionnaire_draft_specs WHERE draft_id=$1")
            .bind(draft.id)
            .fetch_optional(&mut **tx)
            .await?;
    let clinical_entry: Option<DbJson<serde_json::Value>> =
        sqlx::query_scalar("SELECT spec FROM clinical_entry_draft_specs WHERE draft_id=$1")
            .bind(draft.id)
            .fetch_optional(&mut **tx)
            .await?;
    let drug: Option<DbJson<serde_json::Value>> =
        sqlx::query_scalar("SELECT spec FROM drug_draft_specs WHERE draft_id=$1")
            .bind(draft.id)
            .fetch_optional(&mut **tx)
            .await?;
    let snapshot = serde_json::json!({"draft": draft, "questionnaire": spec.map(|v| v.0), "clinical_entry": clinical_entry.map(|v| v.0), "drug": drug.map(|v| v.0)});
    sqlx::query("INSERT INTO catalog_draft_changes(id,draft_id,author_id,version,snapshot) VALUES($1,$2,$3,$4,$5)")
        .bind(Uuid::new_v4()).bind(draft.id).bind(user.id).bind(draft.version).bind(DbJson(snapshot)).execute(&mut **tx).await?;
    Ok(())
}

#[utoipa::path(get, path="/api/v1/admin/catalog-drafts", operation_id="admin_list_drafts", security(("session_cookie"=[])), responses((status=200,body=Vec<CatalogDraft>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn list_drafts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<CatalogDraft>>> {
    require_admin(&state, &headers).await?;
    let rows = sqlx::query_as::<_, CatalogDraft>("SELECT id,kind,title,source,details,version,created_at,updated_at FROM catalog_drafts WHERE id<>$1 ORDER BY updated_at DESC,id DESC LIMIT 500")
        .bind(Uuid::parse_str(crate::custom_questionnaires::PHQ9_DRAFT_ID).map_err(|_| ApiError::internal())?)
        .fetch_all(&state.db).await?;
    Ok(Json(rows))
}

#[utoipa::path(post, path="/api/v1/admin/catalog-drafts", operation_id="admin_create_draft", security(("session_cookie"=[])), params(("X-Synapse-Request"=String,Header)), request_body=DraftInput, responses((status=201,body=CatalogDraft),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn create_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<DraftInput>,
) -> ApiResult<(StatusCode, Json<CatalogDraft>)> {
    let user = require_admin(&state, &headers).await?;
    let input = input.validated()?;
    let mut tx = state.db.begin().await?;
    let row = sqlx::query_as::<_, CatalogDraft>("INSERT INTO catalog_drafts(id,kind,title,source,details,version,author_id) VALUES($1,$2,$3,$4,$5,1,$6) RETURNING id,kind,title,source,details,version,created_at,updated_at")
        .bind(Uuid::new_v4()).bind(input.kind).bind(input.title).bind(input.source).bind(input.details).bind(user.id).fetch_one(&mut *tx).await?;
    audit(&mut tx, &user, &row).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(patch, path="/api/v1/admin/catalog-drafts/{id}", operation_id="admin_update_draft", security(("session_cookie"=[])), params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)), request_body=UpdateDraftInput, responses((status=200,body=CatalogDraft),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn update_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateDraftInput>,
) -> ApiResult<Json<CatalogDraft>> {
    let user = require_admin(&state, &headers).await?;
    let input_draft = input.draft.validated()?;
    let mut tx = state.db.begin().await?;
    let before = sqlx::query_as::<_, CatalogDraft>("SELECT id,kind,title,source,details,version,created_at,updated_at FROM catalog_drafts WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::not_found)?;
    if before.version != input.expected_version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "catalog_conflict",
            "Черновик уже изменён. Обновите список".into(),
        ));
    }
    if before.kind != input_draft.kind {
        let has_spec: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM questionnaire_draft_specs WHERE draft_id=$1) OR EXISTS(SELECT 1 FROM clinical_entry_draft_specs WHERE draft_id=$1) OR EXISTS(SELECT 1 FROM drug_draft_specs WHERE draft_id=$1)",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if has_spec {
            return Err(ApiError::invalid(
                "Нельзя менять тип черновика с сохранёнными структурированными сведениями",
            ));
        }
    }
    let row = sqlx::query_as::<_, CatalogDraft>("UPDATE catalog_drafts SET kind=$1,title=$2,source=$3,details=$4,version=version+1,updated_at=now() WHERE id=$5 RETURNING id,kind,title,source,details,version,created_at,updated_at")
        .bind(input_draft.kind).bind(input_draft.title).bind(input_draft.source).bind(input_draft.details).bind(id).fetch_one(&mut *tx).await?;
    audit(&mut tx, &user, &row).await?;
    tx.commit().await?;
    Ok(Json(row))
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QuestionnaireOption {
    pub(crate) label: String,
    pub(crate) weight: i32,
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QuestionnaireQuestion {
    pub(crate) id: Uuid,
    pub(crate) text: String,
    pub(crate) options: Vec<QuestionnaireOption>,
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QuestionnaireBand {
    pub(crate) min: i32,
    pub(crate) max: i32,
    pub(crate) label: String,
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QuestionnaireDraftSpec {
    pub(crate) instruction: String,
    pub(crate) minimum_age: u8,
    pub(crate) license: String,
    pub(crate) limitations: String,
    #[serde(default)]
    pub(crate) source_url: Option<String>,
    #[serde(default)]
    pub(crate) scoring_url: Option<String>,
    #[serde(default)]
    pub(crate) source_sha256: Option<String>,
    #[serde(default)]
    pub(crate) attribution: Option<String>,
    #[serde(default)]
    pub(crate) license_url: Option<String>,
    #[serde(default)]
    pub(crate) checked_on: Option<String>,
    pub(crate) questions: Vec<QuestionnaireQuestion>,
    pub(crate) bands: Vec<QuestionnaireBand>,
}

impl QuestionnaireDraftSpec {
    pub(crate) fn validate(&self) -> ApiResult<()> {
        let bad = || ApiError::invalid("Проверьте вопросы, ответы, баллы и диапазоны результата");
        let text_ok = |value: &str, max: usize| {
            !value.trim().is_empty()
                && value.chars().count() <= max
                && !value
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t' && c != '\r')
        };
        if !text_ok(&self.instruction, 4000)
            || !text_ok(&self.license, 1000)
            || !text_ok(&self.limitations, 4000)
            || self.minimum_age > 120
            || self.questions.is_empty()
            || self.questions.len() > 50
            || self.bands.len() > 30
        {
            return Err(bad());
        }
        for value in [
            self.source_url.as_deref(),
            self.scoring_url.as_deref(),
            self.license_url.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !value.starts_with("https://") || value.chars().count() > 2000 {
                return Err(bad());
            }
        }
        for value in [self.attribution.as_deref(), self.checked_on.as_deref()]
            .into_iter()
            .flatten()
        {
            if !text_ok(value, 1000) {
                return Err(bad());
            }
        }
        if self.source_sha256.as_deref().is_some_and(|value| {
            value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit())
        }) {
            return Err(bad());
        }
        let mut ids = std::collections::HashSet::new();
        let (mut min_score, mut max_score) = (0, 0);
        for q in &self.questions {
            if !ids.insert(q.id) || !text_ok(&q.text, 2000) || !(2..=10).contains(&q.options.len())
            {
                return Err(bad());
            }
            if q.options
                .iter()
                .any(|o| !text_ok(&o.label, 500) || !(0..=100).contains(&o.weight))
            {
                return Err(bad());
            }
            min_score += q.options.iter().map(|o| o.weight).min().ok_or_else(bad)?;
            max_score += q.options.iter().map(|o| o.weight).max().ok_or_else(bad)?;
        }
        if !self.bands.is_empty() {
            let mut bands: Vec<_> = self.bands.iter().collect();
            bands.sort_by_key(|b| b.min);
            let mut next = min_score;
            for band in bands {
                if band.min != next
                    || band.max < band.min
                    || band.max > max_score
                    || !text_ok(&band.label, 500)
                {
                    return Err(bad());
                }
                next = band.max + 1;
            }
            if next != max_score + 1 {
                return Err(bad());
            }
        }
        Ok(())
    }
}

const MANAGED_GAD7_DRAFT_ID: &str = "3ab28556-f1b4-5d19-ae35-4bd7777260ac";

#[utoipa::path(post, path="/api/v1/admin/catalog-drafts/import-gad7", operation_id="admin_import_gad7", security(("session_cookie"=[])), params(("X-Synapse-Request"=String,Header)), responses((status=200,body=CatalogDraft),(status=201,body=CatalogDraft),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn import_gad7(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<(StatusCode, Json<CatalogDraft>)> {
    let user = require_admin(&state, &headers).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('synapse_managed_gad7_import'))")
        .execute(&mut *tx)
        .await?;
    let id = Uuid::parse_str(MANAGED_GAD7_DRAFT_ID).map_err(|_| ApiError::internal())?;
    if let Some(existing) = sqlx::query_as::<_, CatalogDraft>("SELECT id,kind,title,source,details,version,created_at,updated_at FROM catalog_drafts WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await?
    {
        if existing.kind != "questionnaire" {
            return Err(ApiError(StatusCode::CONFLICT, "catalog_conflict", "Идентификатор GAD‑7 занят другим разделом каталога".into()));
        }
        tx.commit().await?;
        return Ok((StatusCode::OK, Json(existing)));
    }
    let source: serde_json::Value =
        serde_json::from_str(include_str!("../../../clinical/gad7.json"))
            .map_err(|_| ApiError::internal())?;
    let texts = source["questions"]
        .as_array()
        .ok_or_else(ApiError::internal)?;
    let options = source["options"]
        .as_array()
        .ok_or_else(ApiError::internal)?;
    if texts.len() != 7 || options.len() != 4 {
        return Err(ApiError::internal());
    }
    let spec = QuestionnaireDraftSpec {
        instruction: source["instruction"]
            .as_str()
            .ok_or_else(ApiError::internal)?
            .to_owned(),
        minimum_age: 18,
        license: source["license"]
            .as_str()
            .ok_or_else(ApiError::internal)?
            .to_owned(),
        limitations: source["limitations"]
            .as_str()
            .ok_or_else(ApiError::internal)?
            .to_owned(),
        source_url: Some(
            source["source_url"]
                .as_str()
                .ok_or_else(ApiError::internal)?
                .to_owned(),
        ),
        scoring_url: Some(
            source["scoring_url"]
                .as_str()
                .ok_or_else(ApiError::internal)?
                .to_owned(),
        ),
        source_sha256: Some(
            source["source_sha256"]
                .as_str()
                .ok_or_else(ApiError::internal)?
                .to_owned(),
        ),
        attribution: Some(
            source["attribution"]
                .as_str()
                .ok_or_else(ApiError::internal)?
                .to_owned(),
        ),
        license_url: Some(
            source["license_url"]
                .as_str()
                .ok_or_else(ApiError::internal)?
                .to_owned(),
        ),
        checked_on: Some(
            source["checked_on"]
                .as_str()
                .ok_or_else(ApiError::internal)?
                .to_owned(),
        ),
        questions: texts
            .iter()
            .map(|text| {
                Ok(QuestionnaireQuestion {
                    id: Uuid::new_v4(),
                    text: text.as_str().ok_or_else(ApiError::internal)?.to_owned(),
                    options: options
                        .iter()
                        .enumerate()
                        .map(|(weight, label)| {
                            Ok(QuestionnaireOption {
                                label: label.as_str().ok_or_else(ApiError::internal)?.to_owned(),
                                weight: weight as i32,
                            })
                        })
                        .collect::<ApiResult<Vec<_>>>()?,
                })
            })
            .collect::<ApiResult<Vec<_>>>()?,
        bands: vec![
            QuestionnaireBand {
                min: 0,
                max: 4,
                label: "Минимальные симптомы".into(),
            },
            QuestionnaireBand {
                min: 5,
                max: 9,
                label: "Лёгкая выраженность".into(),
            },
            QuestionnaireBand {
                min: 10,
                max: 14,
                label: "Умеренная выраженность".into(),
            },
            QuestionnaireBand {
                min: 15,
                max: 21,
                label: "Высокая выраженность".into(),
            },
        ],
    };
    spec.validate()?;
    let source_text = format!(
        "{}; {}",
        source["source_url"]
            .as_str()
            .ok_or_else(ApiError::internal)?,
        source["license"].as_str().ok_or_else(ApiError::internal)?
    );
    let draft = sqlx::query_as::<_, CatalogDraft>("INSERT INTO catalog_drafts(id,kind,title,source,details,version,author_id) VALUES($1,'questionnaire',$2,$3,$4,1,$5) RETURNING id,kind,title,source,details,version,created_at,updated_at")
        .bind(id).bind(source["title"].as_str().ok_or_else(ApiError::internal)?).bind(source_text).bind("Перенос проверенной формы GAD‑7 в управляемый каталог. Публикация требует ручной проверки администратором.").bind(user.id).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO questionnaire_draft_specs(draft_id,spec) VALUES($1,$2)")
        .bind(id)
        .bind(DbJson(&spec))
        .execute(&mut *tx)
        .await?;
    audit(&mut tx, &user, &draft).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(draft)))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct QuestionnaireDraftResponse {
    version: i32,
    #[schema(required = true)]
    spec: Option<QuestionnaireDraftSpec>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SaveQuestionnaireDraft {
    expected_version: i32,
    spec: QuestionnaireDraftSpec,
}

#[utoipa::path(get, path="/api/v1/admin/catalog-drafts/{id}/questionnaire", operation_id="admin_get_questionnaire_draft", security(("session_cookie"=[])), params(("id"=Uuid,Path)), responses((status=200,body=QuestionnaireDraftResponse),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn get_questionnaire_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<QuestionnaireDraftResponse>> {
    require_admin(&state, &headers).await?;
    let row: Option<(i32, Option<DbJson<QuestionnaireDraftSpec>>)> = sqlx::query_as("SELECT d.version,s.spec FROM catalog_drafts d LEFT JOIN questionnaire_draft_specs s ON s.draft_id=d.id WHERE d.id=$1 AND d.kind='questionnaire'")
        .bind(id).fetch_optional(&state.db).await?;
    let (version, spec) = row.ok_or_else(ApiError::not_found)?;
    Ok(Json(QuestionnaireDraftResponse {
        version,
        spec: spec.map(|v| v.0),
    }))
}

#[utoipa::path(patch, path="/api/v1/admin/catalog-drafts/{id}/questionnaire", operation_id="admin_save_questionnaire_draft", security(("session_cookie"=[])), params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)), request_body=SaveQuestionnaireDraft, responses((status=200,body=QuestionnaireDraftResponse),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn save_questionnaire_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<SaveQuestionnaireDraft>,
) -> ApiResult<Json<QuestionnaireDraftResponse>> {
    let user = require_admin(&state, &headers).await?;
    input.spec.validate()?;
    let mut tx = state.db.begin().await?;
    let before = sqlx::query_as::<_, CatalogDraft>("SELECT id,kind,title,source,details,version,created_at,updated_at FROM catalog_drafts WHERE id=$1 AND kind='questionnaire' FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::not_found)?;
    if before.version != input.expected_version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "catalog_conflict",
            "Черновик уже изменён. Обновите список".into(),
        ));
    }
    sqlx::query("INSERT INTO questionnaire_draft_specs(draft_id,spec) VALUES($1,$2) ON CONFLICT(draft_id) DO UPDATE SET spec=EXCLUDED.spec,updated_at=now()")
        .bind(id).bind(DbJson(&input.spec)).execute(&mut *tx).await?;
    let row = sqlx::query_as::<_, CatalogDraft>("UPDATE catalog_drafts SET version=version+1,updated_at=now() WHERE id=$1 RETURNING id,kind,title,source,details,version,created_at,updated_at")
        .bind(id).fetch_one(&mut *tx).await?;
    audit(&mut tx, &user, &row).await?;
    tx.commit().await?;
    Ok(Json(QuestionnaireDraftResponse {
        version: row.version,
        spec: Some(input.spec),
    }))
}
