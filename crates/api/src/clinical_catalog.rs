use crate::{ApiError, ApiResult, AppState, admin};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, types::Json as DbJson};
use uuid::Uuid;

type PublishDraftRow = (
    String,
    String,
    String,
    i32,
    Option<DbJson<ClinicalEntryDraftSpec>>,
);
type SourceRow = (String, Option<String>, Option<String>, Option<i32>, String);

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ClinicalEntryDraftSpec {
    code: String,
    source_code: String,
    context: String,
    source_page: i32,
}

impl ClinicalEntryDraftSpec {
    fn validate(&mut self, kind: &str) -> ApiResult<()> {
        self.code = self.code.trim().to_ascii_uppercase();
        self.source_code = self.source_code.trim().to_ascii_uppercase();
        self.context = self.context.trim().to_owned();
        let valid_code = |value: &str| {
            !value.is_empty()
                && value.len() <= 40
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || ".-_:+".contains(character)
                })
        };
        match kind {
            "disease" if valid_code(&self.code) => {
                self.source_code.clear();
                self.context.clear();
                self.source_page = 0;
            }
            "finding"
                if valid_code(&self.source_code)
                    && !self.context.is_empty()
                    && self.context.chars().count() <= 4000
                    && self.source_page > 0
                    && self.source_page <= 20000 =>
            {
                self.code.clear();
            }
            _ => {
                return Err(ApiError::invalid(
                    "Проверьте код заболевания либо связь и контекст проявления",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ClinicalEntryDraftResponse {
    version: i32,
    kind: String,
    #[schema(required = true)]
    spec: Option<ClinicalEntryDraftSpec>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SaveClinicalEntryDraft {
    expected_version: i32,
    spec: ClinicalEntryDraftSpec,
}

#[derive(Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct ClinicalEntryVersion {
    id: Uuid,
    draft_id: Uuid,
    draft_version: i32,
    kind: String,
    #[schema(required = true)]
    code: Option<String>,
    #[schema(required = true)]
    title: Option<String>,
    #[schema(required = true)]
    label: Option<String>,
    #[schema(required = true)]
    context: Option<String>,
    #[schema(required = true)]
    source_code: Option<String>,
    #[schema(required = true)]
    source_page: Option<i32>,
    source: String,
    active: bool,
    published_at: DateTime<Utc>,
}

const SELECT: &str = "id,draft_id,draft_version,kind,code,title,label,context,source_code,source_page,source,active,published_at";

#[utoipa::path(get, path="/api/v1/admin/clinical-catalog", operation_id="admin_clinical_catalog", security(("session_cookie"=[])), responses((status=200,body=Vec<ClinicalEntryVersion>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<ClinicalEntryVersion>>> {
    admin::require_admin(&state, &headers).await?;
    let entries = sqlx::query_as(&format!(
        "SELECT {SELECT} FROM clinical_catalog_entry_versions WHERE active ORDER BY kind,code,label,id LIMIT 500"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(entries))
}

#[utoipa::path(get, path="/api/v1/admin/catalog-drafts/{id}/clinical-entry", operation_id="admin_get_clinical_entry_draft", security(("session_cookie"=[])), params(("id"=Uuid,Path)), responses((status=200,body=ClinicalEntryDraftResponse),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn get_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ClinicalEntryDraftResponse>> {
    admin::require_admin(&state, &headers).await?;
    let row: Option<(i32, String, Option<DbJson<ClinicalEntryDraftSpec>>)> = sqlx::query_as(
        "SELECT d.version,d.kind,s.spec FROM catalog_drafts d LEFT JOIN clinical_entry_draft_specs s ON s.draft_id=d.id WHERE d.id=$1 AND d.kind IN ('disease','finding')",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (version, kind, spec) = row.ok_or_else(ApiError::not_found)?;
    Ok(Json(ClinicalEntryDraftResponse {
        version,
        kind,
        spec: spec.map(|value| value.0),
    }))
}

#[utoipa::path(patch, path="/api/v1/admin/catalog-drafts/{id}/clinical-entry", operation_id="admin_save_clinical_entry_draft", security(("session_cookie"=[])), params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)), request_body=SaveClinicalEntryDraft, responses((status=200,body=ClinicalEntryDraftResponse),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn save_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<SaveClinicalEntryDraft>,
) -> ApiResult<Json<ClinicalEntryDraftResponse>> {
    let user = admin::require_admin(&state, &headers).await?;
    let mut spec = input.spec;
    let mut tx = state.db.begin().await?;
    let before: Option<(String, String, i32)> = sqlx::query_as(
        "SELECT kind,title,version FROM catalog_drafts WHERE id=$1 AND kind IN ('disease','finding') FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (kind, title, version) = before.ok_or_else(ApiError::not_found)?;
    if input.expected_version != version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "catalog_conflict",
            "Черновик изменился. Обновите его перед сохранением".into(),
        ));
    }
    spec.validate(&kind)?;
    if title.trim().is_empty() {
        return Err(ApiError::invalid("Укажите название записи"));
    }
    sqlx::query("INSERT INTO clinical_entry_draft_specs(draft_id,spec) VALUES($1,$2) ON CONFLICT(draft_id) DO UPDATE SET spec=EXCLUDED.spec,updated_at=now()")
        .bind(id).bind(DbJson(&spec)).execute(&mut *tx).await?;
    let next = version + 1;
    sqlx::query("UPDATE catalog_drafts SET version=$1,updated_at=now() WHERE id=$2")
        .bind(next)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO catalog_draft_changes(id,draft_id,author_id,version,snapshot) VALUES($1,$2,$3,$4,$5)")
        .bind(Uuid::new_v4()).bind(id).bind(user.id).bind(next)
        .bind(DbJson(serde_json::json!({"kind":kind,"title":title,"clinical_entry":spec})))
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(ClinicalEntryDraftResponse {
        version: next,
        kind,
        spec: Some(spec),
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PublishClinicalEntry {
    expected_version: i32,
    reviewed: bool,
}

#[utoipa::path(post, path="/api/v1/admin/catalog-drafts/{id}/publish-clinical-entry", operation_id="admin_publish_clinical_entry", security(("session_cookie"=[])), params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)), request_body=PublishClinicalEntry, responses((status=201,body=ClinicalEntryVersion),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<PublishClinicalEntry>,
) -> ApiResult<(StatusCode, Json<ClinicalEntryVersion>)> {
    let user = admin::require_admin(&state, &headers).await?;
    if !input.reviewed {
        return Err(ApiError::invalid(
            "Подтвердите проверку содержания, источника и прав использования",
        ));
    }
    let mut tx = state.db.begin().await?;
    let row: Option<PublishDraftRow> = sqlx::query_as(
        "SELECT d.kind,d.title,d.source,d.version,s.spec FROM catalog_drafts d LEFT JOIN clinical_entry_draft_specs s ON s.draft_id=d.id WHERE d.id=$1 AND d.kind IN ('disease','finding') FOR UPDATE OF d",
    ).bind(id).fetch_optional(&mut *tx).await?;
    let (kind, title, source, version, spec) = row.ok_or_else(ApiError::not_found)?;
    if version != input.expected_version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "catalog_conflict",
            "Черновик изменился. Обновите его перед публикацией".into(),
        ));
    }
    if source.trim().is_empty() {
        return Err(ApiError::invalid("Укажите источник записи"));
    }
    let mut spec = spec
        .ok_or_else(|| ApiError::invalid("Сначала заполните сведения записи"))?
        .0;
    spec.validate(&kind)?;
    if kind == "disease" {
        if crate::clinical::static_has_code(&spec.code) {
            return Err(ApiError::invalid(
                "Этот код уже содержится в проверенном исходном справочнике",
            ));
        }
        let previous_code: Option<String> = sqlx::query_scalar(
            "SELECT code FROM clinical_catalog_entry_versions WHERE draft_id=$1 AND kind='disease' AND active",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if previous_code
            .as_ref()
            .is_some_and(|previous| previous != &spec.code)
        {
            return Err(ApiError::invalid(
                "Чтобы сохранить связи и историю, код опубликованного заболевания нельзя менять",
            ));
        }
        let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clinical_catalog_entry_versions WHERE active AND kind='disease' AND code=$1 AND draft_id<>$2)")
            .bind(&spec.code).bind(id).fetch_one(&mut *tx).await?;
        if duplicate {
            return Err(ApiError::invalid(
                "Код уже опубликован в управляемом каталоге",
            ));
        }
    } else {
        let source_exists = crate::clinical::static_has_recordable_code(&spec.source_code)
            || sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM clinical_catalog_entry_versions WHERE active AND kind='disease' AND code=$1)")
                .bind(&spec.source_code).fetch_one(&mut *tx).await?;
        if !source_exists {
            return Err(ApiError::invalid(
                "Свяжите проявление с опубликованным заболеванием или кодом исходного справочника",
            ));
        }
    }
    if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM clinical_catalog_entry_versions WHERE draft_id=$1 AND draft_version=$2)")
        .bind(id).bind(version).fetch_one(&mut *tx).await? {
        return Err(ApiError(StatusCode::CONFLICT,"catalog_conflict","Эта редакция уже опубликована; сохраните новую версию черновика".into()));
    }
    let has_current: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM clinical_catalog_entry_versions WHERE draft_id=$1 AND active)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let active_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM clinical_catalog_entry_versions WHERE active")
            .fetch_one(&mut *tx)
            .await?;
    if active_count >= 1000 && !has_current {
        return Err(ApiError::invalid(
            "Управляемый каталог достиг лимита в 1000 опубликованных записей",
        ));
    }
    let replaced: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE clinical_catalog_entry_versions SET active=false WHERE draft_id=$1 AND active RETURNING id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for previous in replaced {
        sqlx::query("INSERT INTO clinical_catalog_entry_changes(id,entry_version_id,author_id,action) VALUES($1,$2,$3,'replaced')")
            .bind(Uuid::new_v4()).bind(previous).bind(user.id).execute(&mut *tx).await?;
    }
    let (code, entry_title, label, context, source_code, source_page) = if kind == "disease" {
        (Some(spec.code), Some(title), None, None, None, None)
    } else {
        (
            None,
            None,
            Some(title),
            Some(spec.context),
            Some(spec.source_code),
            Some(spec.source_page),
        )
    };
    let entry: ClinicalEntryVersion = sqlx::query_as(&format!("INSERT INTO clinical_catalog_entry_versions(id,draft_id,draft_version,kind,code,title,label,context,source_code,source_page,source,published_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING {SELECT}"))
        .bind(Uuid::new_v4()).bind(id).bind(version).bind(&kind).bind(code).bind(entry_title).bind(label).bind(context).bind(source_code).bind(source_page).bind(source).bind(user.id).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO clinical_catalog_entry_changes(id,entry_version_id,author_id,action) VALUES($1,$2,$3,'published')")
        .bind(Uuid::new_v4()).bind(entry.id).bind(user.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

pub async fn active(state: &AppState) -> ApiResult<(Vec<ClinicalEntryVersion>, String)> {
    let entries: Vec<ClinicalEntryVersion> = sqlx::query_as(&format!(
        "SELECT {SELECT} FROM clinical_catalog_entry_versions WHERE active ORDER BY id LIMIT 1000"
    ))
    .fetch_all(&state.db)
    .await?;
    let mut material = String::new();
    for entry in &entries {
        material.push_str(&format!("{}:{};", entry.id, entry.draft_version));
    }
    let version = if entries.is_empty() {
        crate::clinical::static_catalog_version().to_owned()
    } else {
        let digest = Sha256::digest(material.as_bytes());
        format!(
            "{}+{}",
            crate::clinical::static_catalog_version(),
            hex::encode(&digest[..8])
        )
    };
    Ok((entries, version))
}

pub async fn source(
    state: &AppState,
    code: &str,
) -> ApiResult<Option<crate::clinical::SourceSection>> {
    let entry: Option<SourceRow> = if let Some(id) = code.strip_prefix("managed:") {
        let id = Uuid::parse_str(id).map_err(|_| ApiError::not_found())?;
        sqlx::query_as("SELECT kind,label,context,source_page,source FROM clinical_catalog_entry_versions WHERE active AND draft_id=$1")
            .bind(id).fetch_optional(&state.db).await?
    } else {
        sqlx::query_as("SELECT kind,label,context,source_page,source FROM clinical_catalog_entry_versions WHERE active AND (code=$1 OR source_code=$1) ORDER BY kind='finding' DESC LIMIT 1")
            .bind(code).fetch_optional(&state.db).await?
    };
    Ok(entry.map(
        |(kind, label, context, page, source)| crate::clinical::SourceSection {
            code: code.to_owned(),
            source_page: page.unwrap_or(1) as usize,
            text: match kind.as_str() {
                "finding" => format!(
                    "{}\n{}\nИсточник: {}",
                    label.unwrap_or_default(),
                    context.unwrap_or_default(),
                    source
                ),
                _ => format!("{}\nИсточник: {}", label.unwrap_or_default(), source),
            },
        },
    ))
}

pub fn as_diagnosis(entry: &ClinicalEntryVersion) -> Option<crate::clinical::CatalogDiagnosis> {
    (entry.kind == "disease").then(|| crate::clinical::CatalogDiagnosis {
        code: entry.code.clone().unwrap_or_default(),
        title: entry.title.clone().unwrap_or_default(),
        source_page: 0,
        recordable: true,
    })
}

pub fn as_finding(entry: &ClinicalEntryVersion) -> Option<crate::clinical::CatalogFinding> {
    (entry.kind == "finding").then(|| crate::clinical::CatalogFinding {
        id: format!("managed:{}", entry.draft_id),
        label: entry.label.clone().unwrap_or_default(),
        context: entry.context.clone().unwrap_or_default(),
        source_code: entry.source_code.clone().unwrap_or_default(),
        source_page: entry.source_page.unwrap_or(1) as usize,
    })
}
