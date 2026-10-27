use crate::{ApiError, ApiResult, AppState, admin, therapy::DrugSnapshot};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::Json as DbJson};
use uuid::Uuid;

const AVIANDR_DRAFT_ID: &str = "8ea5348e-ef47-5be1-bc69-26a35a35a228";
const SELECT: &str = "id,draft_id,draft_version,drug_code,spec,active,published_at";

#[derive(Serialize, utoipa::ToSchema)]
pub struct DrugDraftResponse {
    version: i32,
    #[schema(required = true)]
    spec: Option<DrugSnapshot>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SaveDrugDraft {
    expected_version: i32,
    spec: DrugSnapshot,
}

#[derive(Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct DrugCatalogVersion {
    id: Uuid,
    draft_id: Uuid,
    draft_version: i32,
    drug_code: String,
    #[schema(value_type = DrugSnapshot)]
    spec: DbJson<DrugSnapshot>,
    active: bool,
    published_at: DateTime<Utc>,
}

fn validate(spec: &DrugSnapshot) -> ApiResult<()> {
    let text_ok = |value: &str, max: usize| {
        !value.trim().is_empty()
            && value.chars().count() <= max
            && !value.chars().any(|character| character.is_control())
    };
    if spec.code.len() < 2
        || spec.code.len() > 64
        || !spec.code.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        })
        || !text_ok(&spec.brand, 200)
        || !text_ok(&spec.ingredient, 200)
        || (!spec.dosage_form.is_empty() && !text_ok(&spec.dosage_form, 300))
        || (!spec.strength.is_empty() && !text_ok(&spec.strength, 200))
        || !text_ok(&spec.group, 300)
        || !text_ok(&spec.source_title, 1000)
        || !spec.source_url.starts_with("https://")
        || spec.source_url.chars().count() > 2000
        || spec.source_sha256.len() != 64
        || !spec
            .source_sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        || spec.source_date > spec.checked_on
        || spec.indications.is_empty()
        || spec.indications.len() > 50
        || spec.diagnosis_codes.len() > 100
        || spec.effects.is_empty()
        || spec.effects.len() > 20
        || !multiline_text_ok(&spec.study_note, 4000)
        || !multiline_text_ok(&spec.transient_note, 4000)
    {
        return Err(ApiError::invalid(
            "Проверьте сведения о препарате, источник, даты и нежелательные реакции",
        ));
    }
    if spec.indications.iter().any(|value| !text_ok(value, 1000))
        || spec.diagnosis_codes.iter().any(|value| {
            value.is_empty()
                || value.len() > 40
                || !value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || ".-_:+".contains(character)
                })
        })
        || spec.effects.iter().any(|group| {
            !text_ok(&group.frequency, 300)
                || group.items.is_empty()
                || group.items.len() > 100
                || group.items.iter().any(|value| !text_ok(value, 1000))
        })
    {
        return Err(ApiError::invalid(
            "Показания и нежелательные реакции должны содержать короткие непустые пункты",
        ));
    }
    Ok(())
}

fn multiline_text_ok(value: &str, max: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

#[utoipa::path(post, path="/api/v1/admin/catalog-drafts/import-aviandr", operation_id="admin_import_aviandr", security(("session_cookie"=[])), params(("X-Synapse-Request"=String,Header)), responses((status=200,body=crate::admin::CatalogDraft),(status=201,body=crate::admin::CatalogDraft),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn import_aviandr(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<(StatusCode, Json<admin::CatalogDraft>)> {
    let user = admin::require_admin(&state, &headers).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('synapse_managed_aviandr_import'))")
        .execute(&mut *tx)
        .await?;
    let id = Uuid::parse_str(AVIANDR_DRAFT_ID).map_err(|_| ApiError::internal())?;
    if let Some(existing) = sqlx::query_as::<_, admin::CatalogDraft>("SELECT id,kind,title,source,details,version,created_at,updated_at FROM catalog_drafts WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut *tx).await? {
        if existing.kind != "drug" {
            return Err(ApiError(StatusCode::CONFLICT,"catalog_conflict","Идентификатор Авиандра занят другой записью каталога".into()));
        }
        tx.commit().await?;
        return Ok((StatusCode::OK,Json(existing)));
    }
    let spec: DrugSnapshot = serde_json::from_str(include_str!("../../../clinical/aviandr.json"))
        .map_err(|_| ApiError::internal())?;
    validate(&spec)?;
    let draft = sqlx::query_as::<_, admin::CatalogDraft>("INSERT INTO catalog_drafts(id,kind,title,source,details,version,author_id) VALUES($1,'drug',$2,$3,$4,1,$5) RETURNING id,kind,title,source,details,version,created_at,updated_at")
        .bind(id).bind(&spec.brand).bind(format!("{}; SHA-256 {}",spec.source_url,spec.source_sha256)).bind("Проверенная справка препарата и перечень нежелательных реакций. Перед публикацией проверьте источник и применимость данных.").bind(user.id).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO drug_draft_specs(draft_id,spec) VALUES($1,$2)")
        .bind(id)
        .bind(DbJson(&spec))
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO catalog_draft_changes(id,draft_id,author_id,version,snapshot) VALUES($1,$2,$3,1,$4)")
        .bind(Uuid::new_v4()).bind(id).bind(user.id)
        .bind(DbJson(serde_json::json!({"draft":draft,"drug":spec})))
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(draft)))
}

#[utoipa::path(get, path="/api/v1/admin/catalog-drafts/{id}/drug", operation_id="admin_get_drug_draft", security(("session_cookie"=[])), params(("id"=Uuid,Path)), responses((status=200,body=DrugDraftResponse),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn get_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DrugDraftResponse>> {
    admin::require_admin(&state, &headers).await?;
    let row: Option<(i32,Option<DbJson<DrugSnapshot>>)> = sqlx::query_as("SELECT d.version,s.spec FROM catalog_drafts d LEFT JOIN drug_draft_specs s ON s.draft_id=d.id WHERE d.id=$1 AND d.kind='drug'")
        .bind(id).fetch_optional(&state.db).await?;
    let (version, spec) = row.ok_or_else(ApiError::not_found)?;
    Ok(Json(DrugDraftResponse {
        version,
        spec: spec.map(|value| value.0),
    }))
}

#[utoipa::path(patch, path="/api/v1/admin/catalog-drafts/{id}/drug", operation_id="admin_save_drug_draft", security(("session_cookie"=[])), params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)), request_body=SaveDrugDraft, responses((status=200,body=DrugDraftResponse),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn save_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<SaveDrugDraft>,
) -> ApiResult<Json<DrugDraftResponse>> {
    let user = admin::require_admin(&state, &headers).await?;
    validate(&input.spec)?;
    let mut tx = state.db.begin().await?;
    let before: Option<i32> = sqlx::query_scalar(
        "SELECT version FROM catalog_drafts WHERE id=$1 AND kind='drug' FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let version = before.ok_or_else(ApiError::not_found)?;
    if version != input.expected_version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "catalog_conflict",
            "Черновик препарата изменился. Обновите его перед сохранением".into(),
        ));
    }
    sqlx::query("INSERT INTO drug_draft_specs(draft_id,spec) VALUES($1,$2) ON CONFLICT(draft_id) DO UPDATE SET spec=EXCLUDED.spec,updated_at=now()")
        .bind(id).bind(DbJson(&input.spec)).execute(&mut *tx).await?;
    let next = version + 1;
    sqlx::query("UPDATE catalog_drafts SET version=$1,updated_at=now(),source=$2 WHERE id=$3")
        .bind(next)
        .bind(format!(
            "{}; SHA-256 {}",
            input.spec.source_url, input.spec.source_sha256
        ))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO catalog_draft_changes(id,draft_id,author_id,version,snapshot) VALUES($1,$2,$3,$4,$5)")
        .bind(Uuid::new_v4()).bind(id).bind(user.id).bind(next)
        .bind(DbJson(serde_json::json!({"version":next,"drug":input.spec})))
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(DrugDraftResponse {
        version: next,
        spec: Some(input.spec),
    }))
}

#[utoipa::path(get, path="/api/v1/admin/drugs", operation_id="admin_drug_catalog", security(("session_cookie"=[])), responses((status=200,body=Vec<DrugCatalogVersion>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<DrugCatalogVersion>>> {
    admin::require_admin(&state, &headers).await?;
    let entries=sqlx::query_as(&format!("SELECT {SELECT} FROM drug_catalog_versions WHERE active ORDER BY published_at DESC LIMIT 100"))
        .fetch_all(&state.db).await?;
    Ok(Json(entries))
}

#[utoipa::path(get, path="/api/v1/admin/drug-diagnoses", operation_id="admin_drug_diagnoses", security(("session_cookie"=[])), responses((status=200,body=Vec<crate::clinical::CatalogDiagnosis>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn diagnosis_choices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<crate::clinical::CatalogDiagnosis>>> {
    admin::require_admin(&state, &headers).await?;
    Ok(Json(crate::clinical::available_diagnoses(&state).await?))
}

#[utoipa::path(get, path="/api/v1/clinical/drugs", operation_id="clinical_drugs", security(("session_cookie"=[])), responses((status=200,body=Vec<DrugCatalogVersion>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn clinical_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<DrugCatalogVersion>>> {
    let user = crate::authenticate(&state, &headers).await?;
    if user.role != synapse_domain::Role::Doctor {
        return Err(ApiError::forbidden());
    }
    let mut entries: Vec<DrugCatalogVersion> = sqlx::query_as(&format!(
        "SELECT {SELECT} FROM drug_catalog_versions WHERE active ORDER BY spec->>'brand',drug_code LIMIT 500"
    ))
    .fetch_all(&state.db)
    .await?;
    if !entries.iter().any(|item| item.drug_code == "aviandr") {
        entries.push(DrugCatalogVersion {
            id: Uuid::nil(),
            draft_id: Uuid::nil(),
            draft_version: 0,
            drug_code: "aviandr".into(),
            spec: DbJson(crate::therapy::static_drug().clone()),
            active: true,
            published_at: Utc::now(),
        });
    }
    Ok(Json(entries))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PublishDrugInput {
    expected_version: i32,
    reviewed: bool,
}

#[utoipa::path(post, path="/api/v1/admin/catalog-drafts/{id}/publish-drug", operation_id="admin_publish_drug", security(("session_cookie"=[])), params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)), request_body=PublishDrugInput, responses((status=201,body=DrugCatalogVersion),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<PublishDrugInput>,
) -> ApiResult<(StatusCode, Json<DrugCatalogVersion>)> {
    let user = admin::require_admin(&state, &headers).await?;
    if !input.reviewed {
        return Err(ApiError::invalid(
            "Подтвердите проверку содержания, источника и прав использования",
        ));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("LOCK TABLE drug_catalog_versions IN EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let row: Option<(i32,Option<DbJson<DrugSnapshot>>)>=sqlx::query_as("SELECT d.version,s.spec FROM catalog_drafts d LEFT JOIN drug_draft_specs s ON s.draft_id=d.id WHERE d.id=$1 AND d.kind='drug' FOR UPDATE OF d")
        .bind(id).fetch_optional(&mut *tx).await?;
    let (version, spec) = row.ok_or_else(ApiError::not_found)?;
    if version != input.expected_version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "catalog_conflict",
            "Черновик изменился. Обновите его перед публикацией".into(),
        ));
    }
    let spec = spec
        .ok_or_else(|| ApiError::invalid("Сначала заполните структурированные сведения препарата"))?
        .0;
    validate(&spec)?;
    if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM drug_catalog_versions WHERE draft_id=$1 AND draft_version=$2)",
    )
    .bind(id)
    .bind(version)
    .fetch_one(&mut *tx)
    .await?
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "catalog_conflict",
            "Эта версия препарата уже опубликована; сохраните новую редакцию".into(),
        ));
    }
    let duplicate_code: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM drug_catalog_versions WHERE drug_code=$1 AND draft_id<>$2)",
    )
    .bind(&spec.code)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if duplicate_code {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "catalog_conflict",
            "Код препарата уже используется другой записью каталога".into(),
        ));
    }
    if spec.diagnosis_codes.is_empty() {
        return Err(ApiError::invalid(
            "Свяжите препарат хотя бы с одним опубликованным диагнозом",
        ));
    }
    let diagnoses = crate::clinical::available_diagnoses(&state).await?;
    if spec
        .diagnosis_codes
        .iter()
        .any(|code| !diagnoses.iter().any(|diagnosis| &diagnosis.code == code))
    {
        return Err(ApiError::invalid(
            "Связь препарата должна указывать на опубликованный диагноз",
        ));
    }
    let replaced: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE drug_catalog_versions SET active=false WHERE active AND (drug_code=$1 OR draft_id=$2) RETURNING id",
    )
    .bind(&spec.code)
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for previous in replaced {
        sqlx::query("INSERT INTO drug_catalog_changes(id,drug_version_id,author_id,action) VALUES($1,$2,$3,'replaced')")
            .bind(Uuid::new_v4()).bind(previous).bind(user.id).execute(&mut *tx).await?;
    }
    let drug_code = spec.code.clone();
    let entry: DrugCatalogVersion=sqlx::query_as(&format!("INSERT INTO drug_catalog_versions(id,draft_id,draft_version,drug_code,spec,published_by) VALUES($1,$2,$3,$4,$5,$6) RETURNING {SELECT}"))
        .bind(Uuid::new_v4()).bind(id).bind(version).bind(drug_code).bind(DbJson(spec)).bind(user.id).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO drug_catalog_changes(id,drug_version_id,author_id,action) VALUES($1,$2,$3,'published')")
        .bind(Uuid::new_v4()).bind(entry.id).bind(user.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

pub async fn by_code(state: &AppState, code: &str) -> ApiResult<DrugSnapshot> {
    let active: Option<DbJson<DrugSnapshot>> =
        sqlx::query_scalar("SELECT spec FROM drug_catalog_versions WHERE drug_code=$1 AND active")
            .bind(code)
            .fetch_optional(&state.db)
            .await?;
    match active {
        Some(value) => Ok(value.0),
        None if code == "aviandr" => Ok(crate::therapy::static_drug().clone()),
        None => Err(ApiError::not_found()),
    }
}
