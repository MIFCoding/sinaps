use crate::{ApiError, ApiResult, AppState, authenticate, authorize_patient};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, Postgres, Transaction, types::Json as DbJson};
use std::sync::OnceLock;
use synapse_domain::Role;
use uuid::Uuid;

#[derive(Clone, Deserialize)]
struct RuleSet {
    rules: Vec<RecommendationRule>,
}

#[derive(Clone, Deserialize)]
struct RecommendationRule {
    code: String,
    version: String,
    source_kind: String,
    diagnosis_code: Option<String>,
    questionnaire_draft_id: Option<Uuid>,
    item_index: Option<usize>,
    answer_greater_than: Option<u8>,
    route: String,
    title: String,
    rationale: String,
    limitations: String,
    source_url: String,
}

fn rules() -> &'static RuleSet {
    static RULES: OnceLock<RuleSet> = OnceLock::new();
    RULES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../clinical/recommendation-rules.json"))
            .expect("Проверенные правила клинических рекомендаций")
    })
}

fn rule(code: &str) -> ApiResult<&'static RecommendationRule> {
    rules()
        .rules
        .iter()
        .find(|rule| rule.code == code)
        .ok_or_else(ApiError::internal)
}

const SELECT: &str = "id,patient_id,source_kind,source_id,rule_code,rule_version,title,rationale,suggested_route,source_url,evidence,status,decision_comment,version,decided_by,decided_at,published_at,created_at";

#[derive(Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct ClinicalRecommendation {
    id: Uuid,
    patient_id: Uuid,
    source_kind: String,
    source_id: Uuid,
    rule_code: String,
    rule_version: String,
    title: String,
    rationale: String,
    suggested_route: String,
    source_url: String,
    #[schema(value_type = serde_json::Value)]
    evidence: DbJson<Value>,
    status: String,
    decision_comment: String,
    version: i32,
    #[schema(required = true)]
    decided_by: Option<Uuid>,
    #[schema(required = true)]
    decided_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    published_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    value: &ClinicalRecommendation,
    author_id: Option<Uuid>,
    action: &str,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO clinical_recommendation_changes(id,recommendation_id,author_id,version,action,snapshot) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4())
        .bind(value.id)
        .bind(author_id)
        .bind(value.version)
        .bind(action)
        .bind(DbJson(value))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn propose(
    tx: &mut Transaction<'_, Postgres>,
    patient_id: Uuid,
    source_id: Uuid,
    rule: &RecommendationRule,
    evidence: Value,
) -> ApiResult<()> {
    if !matches!(rule.route.as_str(), "mandatory" | "preferred") {
        return Err(ApiError::internal());
    }
    let inserted = sqlx::query(&format!("INSERT INTO clinical_recommendations(id,patient_id,source_kind,source_id,rule_code,rule_version,title,rationale,suggested_route,source_url,evidence) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(source_kind,source_id,rule_code,rule_version) DO NOTHING"))
        .bind(Uuid::new_v4())
        .bind(patient_id)
        .bind(&rule.source_kind)
        .bind(source_id)
        .bind(&rule.code)
        .bind(&rule.version)
        .bind(&rule.title)
        .bind(format!("{} {}", rule.rationale, rule.limitations))
        .bind(&rule.route)
        .bind(&rule.source_url)
        .bind(DbJson(evidence))
        .execute(&mut **tx)
        .await?;
    if inserted.rows_affected() == 1 {
        let value: ClinicalRecommendation = sqlx::query_as(&format!("SELECT {SELECT} FROM clinical_recommendations WHERE source_kind=$1 AND source_id=$2 AND rule_code=$3 AND rule_version=$4"))
            .bind(&rule.source_kind)
            .bind(source_id)
            .bind(&rule.code)
            .bind(&rule.version)
            .fetch_one(&mut **tx)
            .await?;
        audit(tx, &value, None, "proposed").await?;
    }
    Ok(())
}

pub async fn propose_gad_followup(
    tx: &mut Transaction<'_, Postgres>,
    patient_id: Uuid,
    author_id: Uuid,
    diagnosis_id: Uuid,
    code: &str,
    status: &str,
) -> ApiResult<()> {
    let rule = rule("gad_diagnosis_specialist_followup")?;
    if rule.diagnosis_code.as_deref() == Some(code) && status == "active" {
        propose(
            tx,
            patient_id,
            diagnosis_id,
            rule,
            serde_json::json!({"diagnosis_code": code, "diagnosis_status": status}),
        )
        .await?;
    } else {
        let mut stale = sqlx::query_as::<_, ClinicalRecommendation>(&format!(
            "SELECT {SELECT} FROM clinical_recommendations WHERE source_kind='diagnosis' AND source_id=$1 AND rule_code=$2 AND status IN ('proposed','accepted','published') FOR UPDATE"
        ))
        .bind(diagnosis_id)
        .bind(&rule.code)
        .fetch_all(&mut **tx)
        .await?;
        for recommendation in stale.drain(..) {
            sqlx::query("UPDATE clinical_recommendations SET status='withdrawn',decision_comment='Диагноз закрыт или изменён; предложение снято автоматически',decided_by=$1,decided_at=now(),published_at=NULL,version=version+1 WHERE id=$2")
                .bind(author_id)
                .bind(recommendation.id)
                .execute(&mut **tx)
                .await?;
            let updated: ClinicalRecommendation = sqlx::query_as(&format!(
                "SELECT {SELECT} FROM clinical_recommendations WHERE id=$1"
            ))
            .bind(recommendation.id)
            .fetch_one(&mut **tx)
            .await?;
            audit(tx, &updated, Some(author_id), "withdrawn").await?;
        }
    }
    Ok(())
}

pub async fn propose_phq9_safety_assessment(
    tx: &mut Transaction<'_, Postgres>,
    patient_id: Uuid,
    assignment_id: Uuid,
    draft_id: Uuid,
    answers: &[u8],
) -> ApiResult<()> {
    let rule = rule("phq9_item_9_safety_assessment")?;
    let Some(index) = rule.item_index else {
        return Err(ApiError::internal());
    };
    let Some(threshold) = rule.answer_greater_than else {
        return Err(ApiError::internal());
    };
    if rule.questionnaire_draft_id == Some(draft_id)
        && answers.get(index).is_some_and(|answer| *answer > threshold)
    {
        propose(
            tx,
            patient_id,
            assignment_id,
            rule,
            serde_json::json!({"questionnaire_draft_id": draft_id, "question_index": index, "answer": answers[index]}),
        )
        .await?;
    }
    Ok(())
}

#[utoipa::path(get,path="/api/v1/patients/{id}/recommendations",operation_id="list_clinical_recommendations",security(("session_cookie"=[])),params(("id"=Uuid,Path)),responses((status=200,body=Vec<ClinicalRecommendation>),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(patient_id): Path<Uuid>,
) -> ApiResult<Json<Vec<ClinicalRecommendation>>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, patient_id).await?;
    let query = if user.role == Role::Patient {
        format!(
            "SELECT {SELECT} FROM clinical_recommendations WHERE patient_id=$1 AND status='published' ORDER BY created_at DESC,id DESC LIMIT 100"
        )
    } else {
        format!(
            "SELECT {SELECT} FROM clinical_recommendations WHERE patient_id=$1 ORDER BY created_at DESC,id DESC LIMIT 100"
        )
    };
    Ok(Json(
        sqlx::query_as(&query)
            .bind(patient_id)
            .fetch_all(&state.db)
            .await?,
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DecisionInput {
    expected_version: i32,
    decision: String,
    comment: String,
}

#[utoipa::path(post,path="/api/v1/patients/{id}/recommendations/{recommendation_id}/decision",operation_id="decide_clinical_recommendation",security(("session_cookie"=[])),params(("id"=Uuid,Path),("recommendation_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=DecisionInput,responses((status=200,body=ClinicalRecommendation),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn decide(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((patient_id, recommendation_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<DecisionInput>,
) -> ApiResult<Json<ClinicalRecommendation>> {
    let user = authenticate(&state, &headers).await?;
    if user.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &user, patient_id).await?;
    let next_status = match input.decision.as_str() {
        "accept" => "accepted",
        "decline" => "declined",
        _ => {
            return Err(ApiError::invalid(
                "Выберите принятие или отклонение предложения",
            ));
        }
    };
    let comment = input.comment.trim();
    if comment.chars().count() > 4000 {
        return Err(ApiError::invalid(
            "Комментарий не должен превышать 4000 символов",
        ));
    }
    let mut tx = state.db.begin().await?;
    let before: ClinicalRecommendation = sqlx::query_as(&format!(
        "SELECT {SELECT} FROM clinical_recommendations WHERE id=$1 AND patient_id=$2 FOR UPDATE"
    ))
    .bind(recommendation_id)
    .bind(patient_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    if before.status == next_status
        && before.version.checked_sub(1) == Some(input.expected_version)
        && before.decision_comment == comment
    {
        tx.commit().await?;
        return Ok(Json(before));
    }
    if before.status != "proposed" || before.version != input.expected_version {
        return Err(ApiError::conflict());
    }
    sqlx::query("UPDATE clinical_recommendations SET status=$1,decision_comment=$2,version=version+1,decided_by=$3,decided_at=now() WHERE id=$4")
        .bind(next_status).bind(comment).bind(user.id).bind(recommendation_id).execute(&mut *tx).await?;
    let value: ClinicalRecommendation = sqlx::query_as(&format!(
        "SELECT {SELECT} FROM clinical_recommendations WHERE id=$1"
    ))
    .bind(recommendation_id)
    .fetch_one(&mut *tx)
    .await?;
    audit(&mut tx, &value, Some(user.id), next_status).await?;
    tx.commit().await?;
    Ok(Json(value))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PublishInput {
    expected_version: i32,
}

#[utoipa::path(post,path="/api/v1/patients/{id}/recommendations/{recommendation_id}/publish",operation_id="publish_clinical_recommendation",security(("session_cookie"=[])),params(("id"=Uuid,Path),("recommendation_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=PublishInput,responses((status=200,body=ClinicalRecommendation),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((patient_id, recommendation_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<PublishInput>,
) -> ApiResult<Json<ClinicalRecommendation>> {
    let user = authenticate(&state, &headers).await?;
    if user.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    authorize_patient(&state, &user, patient_id).await?;
    let mut tx = state.db.begin().await?;
    let before: ClinicalRecommendation = sqlx::query_as(&format!(
        "SELECT {SELECT} FROM clinical_recommendations WHERE id=$1 AND patient_id=$2 FOR UPDATE"
    ))
    .bind(recommendation_id)
    .bind(patient_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    if before.status == "published" && before.version.checked_sub(1) == Some(input.expected_version)
    {
        tx.commit().await?;
        return Ok(Json(before));
    }
    if before.status != "accepted" || before.version != input.expected_version {
        return Err(ApiError::conflict());
    }
    sqlx::query("UPDATE clinical_recommendations SET status='published',version=version+1,published_at=now() WHERE id=$1")
        .bind(recommendation_id).execute(&mut *tx).await?;
    let value: ClinicalRecommendation = sqlx::query_as(&format!(
        "SELECT {SELECT} FROM clinical_recommendations WHERE id=$1"
    ))
    .bind(recommendation_id)
    .fetch_one(&mut *tx)
    .await?;
    audit(&mut tx, &value, Some(user.id), "published").await?;
    tx.commit().await?;
    Ok(Json(value))
}
