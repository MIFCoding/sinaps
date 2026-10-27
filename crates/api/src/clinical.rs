use crate::{ApiError, ApiResult, AppState, authenticate, authorize_patient};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::Json as DbJson};
use std::{collections::HashSet, sync::OnceLock};
use synapse_domain::Role;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CatalogDiagnosis {
    pub(crate) code: String,
    pub(crate) title: String,
    pub(crate) source_page: usize,
    pub(crate) recordable: bool,
}
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CatalogFinding {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) context: String,
    pub(crate) source_code: String,
    pub(crate) source_page: usize,
}
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SourceSection {
    pub(crate) code: String,
    pub(crate) source_page: usize,
    pub(crate) text: String,
}
#[derive(Deserialize)]
struct Catalog {
    version: String,
    source_title: String,
    source_sha256: String,
    source_pages: usize,
    reference_url: String,
    diagnoses: Vec<CatalogDiagnosis>,
    findings: Vec<CatalogFinding>,
    sections: Vec<SourceSection>,
}
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../../clinical/catalog.json"))
            .expect("Проверенный каталог МКБ-10")
    })
}

pub(crate) async fn available_diagnoses(state: &AppState) -> ApiResult<Vec<CatalogDiagnosis>> {
    let (managed, _) = crate::clinical_catalog::active(state).await?;
    Ok(catalog()
        .diagnoses
        .iter()
        .filter(|item| item.recordable)
        .cloned()
        .chain(
            managed
                .iter()
                .filter_map(crate::clinical_catalog::as_diagnosis),
        )
        .collect())
}
pub(crate) fn static_catalog_version() -> &'static str {
    &catalog().version
}
pub(crate) fn static_has_code(code: &str) -> bool {
    catalog()
        .diagnoses
        .iter()
        .any(|diagnosis| diagnosis.code == code)
}
pub(crate) fn static_has_recordable_code(code: &str) -> bool {
    catalog()
        .diagnoses
        .iter()
        .any(|diagnosis| diagnosis.code == code && diagnosis.recordable)
}
async fn doctor(state: &AppState, headers: &HeaderMap) -> ApiResult<synapse_domain::User> {
    let user = authenticate(state, headers).await?;
    if user.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    Ok(user)
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CatalogQuery {
    search: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
}
impl CatalogQuery {
    fn values(&self) -> ApiResult<(String, usize, usize)> {
        let search = self.search.as_deref().unwrap_or("").trim().to_lowercase();
        let limit = self.limit.unwrap_or(20);
        if search.chars().count() > 120
            || search.chars().any(char::is_control)
            || !(1..=50).contains(&limit)
        {
            return Err(ApiError::invalid(
                "Поиск: до 120 символов; размер страницы: 1–50",
            ));
        }
        Ok((search, self.offset.unwrap_or(0), limit))
    }
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct CatalogPage<T> {
    version: String,
    source_title: String,
    source_sha256: String,
    source_pages: usize,
    reference_url: String,
    items: Vec<T>,
    total_count: usize,
    #[schema(required = true)]
    next_offset: Option<usize>,
}
fn page<T: Clone>(items: Vec<T>, offset: usize, limit: usize, version: String) -> CatalogPage<T> {
    let total_count = items.len();
    let selected = items.into_iter().skip(offset).take(limit).collect();
    CatalogPage {
        version,
        source_title: catalog().source_title.clone(),
        source_sha256: catalog().source_sha256.clone(),
        source_pages: catalog().source_pages,
        reference_url: catalog().reference_url.clone(),
        items: selected,
        total_count,
        next_offset: offset.checked_add(limit).filter(|next| *next < total_count),
    }
}

#[utoipa::path(get, path="/api/v1/clinical/findings", operation_id="clinical_findings", params(CatalogQuery), security(("session_cookie"=[])), responses((status=500,body=crate::ErrorEnvelope),(status=400,body=String,content_type="text/plain"),(status=200,body=CatalogPage<CatalogFinding>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope)))]
pub async fn findings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CatalogQuery>,
) -> ApiResult<Json<CatalogPage<CatalogFinding>>> {
    doctor(&state, &headers).await?;
    let (search, offset, limit) = query.values()?;
    let (managed, version) = crate::clinical_catalog::active(&state).await?;
    let diagnosis_items: Vec<CatalogDiagnosis> = catalog()
        .diagnoses
        .clone()
        .into_iter()
        .chain(
            managed
                .iter()
                .filter_map(crate::clinical_catalog::as_diagnosis)
                .filter(|item| {
                    format!("{} {}", item.code, item.title)
                        .to_lowercase()
                        .contains(&search)
                }),
        )
        .collect();
    let items = catalog()
        .findings
        .iter()
        .cloned()
        .chain(
            managed
                .iter()
                .filter_map(crate::clinical_catalog::as_finding),
        )
        .filter(|item| {
            let title = diagnosis_items
                .iter()
                .find(|diagnosis| diagnosis.code == item.source_code)
                .map(|d| d.title.as_str())
                .unwrap_or("");
            format!(
                "{} {} {} {}",
                item.label, item.context, item.source_code, title
            )
            .to_lowercase()
            .contains(&search)
        })
        .collect();
    Ok(Json(page(items, offset, limit, version)))
}
#[utoipa::path(get,path="/api/v1/clinical/diagnosis-catalog",operation_id="clinical_codes",params(CatalogQuery),security(("session_cookie"=[])),responses((status=500,body=crate::ErrorEnvelope),(status=400,body=String,content_type="text/plain"),(status=200,body=CatalogPage<CatalogDiagnosis>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope)))]
pub async fn codes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CatalogQuery>,
) -> ApiResult<Json<CatalogPage<CatalogDiagnosis>>> {
    doctor(&state, &headers).await?;
    let (search, offset, limit) = query.values()?;
    let (managed, version) = crate::clinical_catalog::active(&state).await?;
    let items = catalog()
        .diagnoses
        .iter()
        .filter(|item| {
            item.recordable
                && format!("{} {}", item.code, item.title)
                    .to_lowercase()
                    .contains(&search)
        })
        .cloned()
        .chain(
            managed
                .iter()
                .filter_map(crate::clinical_catalog::as_diagnosis),
        )
        .collect();
    Ok(Json(page(items, offset, limit, version)))
}
#[utoipa::path(get,path="/api/v1/clinical/source/{code}",operation_id="clinical_source",params(("code"=String,Path)),security(("session_cookie"=[])),responses((status=500,body=crate::ErrorEnvelope),(status=400,body=String,content_type="text/plain"),(status=200,body=SourceSection),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope)))]
pub async fn source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> ApiResult<Json<SourceSection>> {
    doctor(&state, &headers).await?;
    if let Some(section) = crate::clinical_catalog::source(&state, &code).await? {
        return Ok(Json(section));
    }
    Ok(Json(
        catalog()
            .sections
            .iter()
            .find(|section| section.code == code)
            .cloned()
            .ok_or_else(ApiError::not_found)?,
    ))
}

#[derive(Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct SelectedFinding {
    id: String,
    label: String,
    source_code: String,
    source_page: usize,
}
#[derive(Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct Diagnosis {
    id: Uuid,
    patient_id: Uuid,
    code: String,
    title: String,
    catalog_version: String,
    #[schema(value_type=Vec<SelectedFinding>)]
    findings: DbJson<Vec<SelectedFinding>>,
    diagnosed_on: NaiveDate,
    #[schema(value_type=DiagnosisState)]
    status: String,
    note: String,
    version: i32,
    author_name: String,
    updated_by_name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

const DIAGNOSIS_QUERY: &str = "SELECT d.id,d.patient_id,d.code,d.title,d.catalog_version,d.findings,d.diagnosed_on,d.status,d.note,d.version,a.name AS author_name,u.name AS updated_by_name,d.created_at,d.updated_at FROM clinical_diagnoses d JOIN users a ON a.id=d.author_id JOIN users u ON u.id=d.updated_by";

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in=Query)]
pub struct ListQuery {
    limit: Option<i64>,
    before: Option<DateTime<Utc>>,
    before_id: Option<Uuid>,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct DiagnosisCursor {
    before: DateTime<Utc>,
    before_id: Uuid,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct DiagnosisPage {
    items: Vec<Diagnosis>,
    #[schema(required = true)]
    next_cursor: Option<DiagnosisCursor>,
}
#[utoipa::path(get,path="/api/v1/patients/{id}/diagnoses",operation_id="patient_diagnoses",params(("id"=Uuid,Path),ListQuery),security(("session_cookie"=[])),responses((status=500,body=crate::ErrorEnvelope),(status=400,body=String,content_type="text/plain"),(status=200,body=DiagnosisPage),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope)))]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<DiagnosisPage>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) || query.before.is_some() != query.before_id.is_some() {
        return Err(ApiError::invalid(
            "Размер страницы: 1–50; курсор должен содержать дату и UUID",
        ));
    }
    // В ответе только сохранённые выборки, полный справочник сюда не включается.
    let mut items=sqlx::query_as::<_,Diagnosis>(&format!("{DIAGNOSIS_QUERY} WHERE d.patient_id=$1 AND ($2::timestamptz IS NULL OR (d.created_at,d.id)<($2,$3)) ORDER BY d.created_at DESC,d.id DESC LIMIT $4"))
        .bind(id).bind(query.before).bind(query.before_id).bind(limit+1).fetch_all(&state.db).await?;
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next_cursor = if more {
        items.last().map(|item| DiagnosisCursor {
            before: item.created_at,
            before_id: item.id,
        })
    } else {
        None
    };
    Ok(Json(DiagnosisPage { items, next_cursor }))
}
#[utoipa::path(get,path="/api/v1/patients/{id}/diagnoses/{diagnosis_id}",operation_id="patient_diagnosis",params(("id"=Uuid,Path),("diagnosis_id"=Uuid,Path)),security(("session_cookie"=[])),responses((status=500,body=crate::ErrorEnvelope),(status=400,body=String,content_type="text/plain"),(status=200,body=Diagnosis),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope)))]
pub async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, diagnosis_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Diagnosis>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let result = sqlx::query_as(&format!(
        "{DIAGNOSIS_QUERY} WHERE d.patient_id=$1 AND d.id=$2"
    ))
    .bind(id)
    .bind(diagnosis_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Json(result))
}

#[derive(Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosisState {
    Active,
    Resolved,
}
impl DiagnosisState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Resolved => "resolved",
        }
    }
}
#[derive(Deserialize, utoipa::ToSchema)]
pub struct DiagnosisInput {
    pub id: Uuid,
    version: i32,
    catalog_version: String,
    code: String,
    finding_ids: Vec<String>,
    diagnosed_on: NaiveDate,
    status: DiagnosisState,
    note: String,
}
impl DiagnosisInput {
    fn prepare(
        &mut self,
        catalog_version: &str,
        diagnoses: &[CatalogDiagnosis],
        available_findings: &[CatalogFinding],
    ) -> ApiResult<(CatalogDiagnosis, Vec<SelectedFinding>)> {
        self.note = self.note.trim().to_owned();
        if self.catalog_version != catalog_version {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "catalog_changed",
                "Справочник обновлён. Проверьте выбранные пункты".into(),
            ));
        }
        if self.version < 1
            || self.note.chars().count() > 4000
            || self.finding_ids.is_empty()
            || self.finding_ids.len() > 100
        {
            return Err(ApiError::invalid(
                "Выберите от 1 до 100 пунктов. Комментарий должен быть не длиннее 4000 символов",
            ));
        }
        let diagnosis = diagnoses
            .iter()
            .find(|d| d.code == self.code && d.recordable)
            .cloned()
            .ok_or_else(|| ApiError::invalid("Выберите код диагноза из справочника"))?;
        let unique: HashSet<_> = self.finding_ids.iter().collect();
        if unique.len() != self.finding_ids.len() {
            return Err(ApiError::invalid("Пункты не должны повторяться"));
        }
        let mut selected = Vec::new();
        self.finding_ids.sort();
        for id in &self.finding_ids {
            let source = available_findings
                .iter()
                .find(|item| &item.id == id)
                .ok_or_else(|| ApiError::invalid("Неизвестный пункт справочника"))?;
            selected.push(SelectedFinding {
                id: source.id.clone(),
                label: source.label.clone(),
                source_code: source.source_code.clone(),
                source_page: source.source_page,
            });
        }
        Ok((diagnosis, selected))
    }
}

#[utoipa::path(post,path="/api/v1/patients/{id}/diagnoses",operation_id="create_diagnosis",params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=DiagnosisInput,security(("session_cookie"=[])),responses((status=500,body=crate::ErrorEnvelope),(status=400,body=String,content_type="text/plain"),(status=413,body=String,content_type="text/plain"),(status=415,body=String,content_type="text/plain"),(status=201,body=Diagnosis),(status=200,body=Diagnosis),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,content((crate::ErrorEnvelope="application/json"),(String="text/plain")))))]
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<DiagnosisInput>,
) -> ApiResult<(StatusCode, Json<Diagnosis>)> {
    save(state, headers, id, input, None).await
}
#[utoipa::path(patch,path="/api/v1/patients/{id}/diagnoses/{diagnosis_id}",operation_id="update_diagnosis",params(("id"=Uuid,Path),("diagnosis_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=DiagnosisInput,security(("session_cookie"=[])),responses((status=500,body=crate::ErrorEnvelope),(status=400,body=String,content_type="text/plain"),(status=413,body=String,content_type="text/plain"),(status=415,body=String,content_type="text/plain"),(status=200,body=Diagnosis),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,content((crate::ErrorEnvelope="application/json"),(String="text/plain")))))]
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, diagnosis_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<DiagnosisInput>,
) -> ApiResult<(StatusCode, Json<Diagnosis>)> {
    save(state, headers, id, input, Some(diagnosis_id)).await
}
async fn save(
    state: AppState,
    headers: HeaderMap,
    patient: Uuid,
    mut input: DiagnosisInput,
    update: Option<Uuid>,
) -> ApiResult<(StatusCode, Json<Diagnosis>)> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, patient).await?;
    if update.is_some_and(|id| id != input.id) {
        return Err(ApiError::invalid("UUID диагноза не совпадает с путём"));
    }
    if update.is_none() && input.version != 1 {
        return Err(ApiError::invalid("Новый диагноз должен иметь версию 1"));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("LOCK TABLE clinical_catalog_entry_versions IN SHARE MODE")
        .execute(&mut *tx)
        .await?;
    let (managed, current_catalog_version) = crate::clinical_catalog::active(&state).await?;
    let diagnoses: Vec<CatalogDiagnosis> = catalog()
        .diagnoses
        .clone()
        .into_iter()
        .chain(
            managed
                .iter()
                .filter_map(crate::clinical_catalog::as_diagnosis),
        )
        .collect();
    let available_findings: Vec<CatalogFinding> = catalog()
        .findings
        .clone()
        .into_iter()
        .chain(
            managed
                .iter()
                .filter_map(crate::clinical_catalog::as_finding),
        )
        .collect();
    let (code, selected) =
        input.prepare(&current_catalog_version, &diagnoses, &available_findings)?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(patient)
        .execute(&mut *tx)
        .await?;
    let birth: NaiveDate =
        sqlx::query_scalar("SELECT birth_date FROM patient_profiles WHERE user_id=$1")
            .bind(patient)
            .fetch_one(&mut *tx)
            .await?;
    if input.diagnosed_on < birth || input.diagnosed_on > Utc::now().date_naive() {
        return Err(ApiError::invalid(
            "Дата диагноза должна быть между датой рождения и текущей датой",
        ));
    }
    let existing = sqlx::query_as::<_, Diagnosis>(&format!(
        "{DIAGNOSIS_QUERY} WHERE d.patient_id=$1 AND d.id=$2 FOR UPDATE OF d"
    ))
    .bind(patient)
    .bind(input.id)
    .fetch_optional(&mut *tx)
    .await?;
    let same = existing.as_ref().is_some_and(|row| {
        row.code == input.code
            && row.catalog_version == input.catalog_version
            && row.findings.0 == selected
            && row.diagnosed_on == input.diagnosed_on
            && row.status == input.status.as_str()
            && row.note == input.note
    });
    let mut changed = false;
    let status;
    if let Some(before) = existing {
        if update.is_some() && before.version != input.version {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "stale_version",
                "Диагноз изменён в другом окне. Загрузите актуальные данные".into(),
            ));
        }
        if update.is_none() && (!same || before.version != 1) {
            return Err(ApiError::conflict());
        }
        if !same {
            sqlx::query("UPDATE clinical_diagnoses SET code=$1,title=$2,catalog_version=$3,findings=$4,diagnosed_on=$5,status=$6,note=$7,version=version+1,updated_by=$8,updated_at=now() WHERE id=$9")
                .bind(&input.code).bind(&code.title).bind(&input.catalog_version).bind(DbJson(&selected)).bind(input.diagnosed_on).bind(input.status.as_str()).bind(&input.note).bind(user.id).bind(input.id).execute(&mut *tx).await?;
            changed = true;
        }
        status = StatusCode::OK;
    } else {
        if update.is_some() {
            return Err(ApiError::not_found());
        }
        let inserted=sqlx::query("INSERT INTO clinical_diagnoses(id,patient_id,code,title,catalog_version,findings,diagnosed_on,status,note,version,author_id,updated_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,1,$10,$10) ON CONFLICT(id) DO NOTHING")
            .bind(input.id).bind(patient).bind(&input.code).bind(&code.title).bind(&input.catalog_version).bind(DbJson(&selected)).bind(input.diagnosed_on).bind(input.status.as_str()).bind(&input.note).bind(user.id).execute(&mut *tx).await?;
        if inserted.rows_affected() != 1 {
            return Err(ApiError::conflict());
        }
        changed = true;
        status = StatusCode::CREATED;
    }
    let result: Diagnosis = sqlx::query_as(&format!(
        "{DIAGNOSIS_QUERY} WHERE d.id=$1 AND d.patient_id=$2"
    ))
    .bind(input.id)
    .bind(patient)
    .fetch_one(&mut *tx)
    .await?;
    if changed {
        sqlx::query("INSERT INTO clinical_diagnosis_changes(id,diagnosis_id,author_id,version,snapshot) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(input.id).bind(user.id).bind(result.version).bind(DbJson(&result)).execute(&mut *tx).await?;
        crate::clinical_recommendations::propose_gad_followup(
            &mut tx,
            patient,
            user.id,
            input.id,
            &result.code,
            &result.status,
        )
        .await?;
    }
    tx.commit().await?;
    Ok((status, Json(result)))
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in=Query)]
pub struct AuditQuery {
    before_version: Option<i32>,
}
#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct DiagnosisChange {
    version: i32,
    author_name: String,
    created_at: DateTime<Utc>,
    #[schema(value_type=Diagnosis)]
    snapshot: DbJson<Diagnosis>,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct DiagnosisAudit {
    items: Vec<DiagnosisChange>,
    #[schema(required = true)]
    next_version: Option<i32>,
}
#[utoipa::path(get,path="/api/v1/patients/{id}/diagnoses/{diagnosis_id}/changes",operation_id="diagnosis_changes",params(("id"=Uuid,Path),("diagnosis_id"=Uuid,Path),AuditQuery),security(("session_cookie"=[])),responses((status=500,body=crate::ErrorEnvelope),(status=400,body=String,content_type="text/plain"),(status=200,body=DiagnosisAudit),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope)))]
pub async fn changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, diagnosis_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<AuditQuery>,
) -> ApiResult<Json<DiagnosisAudit>> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    if query.before_version.is_some_and(|v| v < 1) {
        return Err(ApiError::invalid(
            "Версия курсора должна быть положительной",
        ));
    }
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM clinical_diagnoses WHERE id=$1 AND patient_id=$2)",
    )
    .bind(diagnosis_id)
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    if !belongs {
        return Err(ApiError::not_found());
    }
    let mut items=sqlx::query_as::<_,DiagnosisChange>("SELECT c.version,c.created_at,c.snapshot,u.name AS author_name FROM clinical_diagnosis_changes c JOIN users u ON u.id=c.author_id WHERE c.diagnosis_id=$1 AND ($2::integer IS NULL OR c.version<$2) ORDER BY c.version DESC LIMIT 21")
        .bind(diagnosis_id).bind(query.before_version).fetch_all(&state.db).await?;
    let more = items.len() > 20;
    items.truncate(20);
    let next_version = if more {
        items.last().map(|row| row.version)
    } else {
        None
    };
    Ok(Json(DiagnosisAudit {
        items,
        next_version,
    }))
}
