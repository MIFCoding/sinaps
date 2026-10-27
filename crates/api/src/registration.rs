use crate::*;
use axum::extract::Path;

#[derive(Serialize, utoipa::ToSchema)]
pub struct InvitationPreview {
    existing_account: bool,
    doctor_name: String,
}

#[utoipa::path(get, path="/api/v1/auth/invitation/{code}", operation_id="invitation_preview",
    params(("code"=String,Path)), responses((status=200,body=InvitationPreview),
    (status=404,body=ErrorEnvelope),(status=429,body=ErrorEnvelope),(status=500,body=ErrorEnvelope))) ]
pub async fn invitation_preview(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Path(code): Path<String>,
) -> ApiResult<Json<InvitationPreview>> {
    check_auth_limit(&state, address.ip(), "invitation_preview")?;
    if code.len() != 64 || !code.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::not_found());
    }
    let row: Option<(bool,String)> = sqlx::query_as("SELECT u.activated,d.name FROM patient_invitations i JOIN users u ON u.id=i.patient_id JOIN patient_profiles p ON p.user_id=u.id JOIN users d ON d.id=p.doctor_id WHERE i.token_hash=$1 AND i.consumed_at IS NULL AND i.revoked_at IS NULL AND i.expires_at>now() AND (NOT u.activated OR NOT p.access_accepted)")
        .bind(token_hash(&code.to_lowercase())).fetch_optional(&state.db).await?;
    let (existing_account, doctor_name) = row.ok_or_else(ApiError::not_found)?;
    Ok(Json(InvitationPreview {
        existing_account,
        doctor_name,
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RegisterInput {
    email: String,
    first_name: String,
    last_name: String,
    password: String,
    password_confirmation: String,
    consent: bool,
}

#[utoipa::path(post, path="/api/v1/auth/register", operation_id="register", request_body=RegisterInput,
    params(("X-Synapse-Request"=String,Header)),
    responses((status=201,body=synapse_domain::User),(status=202,body=RegistrationResult),(status=400,body=String,content_type="text/plain"),
    (status=403,body=ErrorEnvelope),(status=409,body=ErrorEnvelope),
    (status=413,body=String,content_type="text/plain"),(status=415,body=String,content_type="text/plain"),
    (status=422,content((ErrorEnvelope="application/json"),(String="text/plain"))),
    (status=429,body=ErrorEnvelope),(status=500,body=ErrorEnvelope))) ]
pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(input): Json<RegisterInput>,
) -> ApiResult<Response> {
    check_auth_limit(&state, address.ip(), "register")?;
    let email = synapse_domain::normalize_login(&input.email);
    let first = input.first_name.trim();
    let last = input.last_name.trim();
    let name = format!("{first} {last}");
    if !synapse_domain::valid_login(&email)
        || (state.delivery.is_strict() && !valid_email_address(&email))
        || first.is_empty()
        || last.is_empty()
        || name.chars().count() > 120
        || name.chars().any(char::is_control)
    {
        return Err(ApiError::invalid(
            "Укажите телефон или почту, имя и фамилию",
        ));
    }
    synapse_domain::validate_new_password(&input.password).map_err(ApiError::invalid)?;
    if !input.consent || input.password != input.password_confirmation {
        return Err(ApiError::invalid(
            "Подтвердите согласие и совпадение паролей",
        ));
    }
    let verify_email = state.delivery.has_mail();
    if verify_email && !valid_email_address(&email) {
        return Err(ApiError::invalid(
            "Для регистрации с подтверждением укажите адрес электронной почты",
        ));
    }
    let hash = tokio::task::spawn_blocking(move || seed::hash(&input.password))
        .await
        .map_err(|_| ApiError::internal())?;
    let mut tx = state.db.begin().await?;
    let account = sqlx::query_as::<_, Account>("INSERT INTO users(id,email,name,role,password_hash,activated,email_verified,registration_consent_at) VALUES($1,$2,$3,'patient',$4,true,$5,now()) ON CONFLICT(email) DO NOTHING RETURNING *")
        .bind(Uuid::new_v4()).bind(email).bind(name).bind(hash).bind(!verify_email).fetch_optional(&mut *tx).await?
        .ok_or_else(|| ApiError(StatusCode::CONFLICT,"account_exists","Этот логин уже используется. Войдите в существующий аккаунт или откройте приглашение врача".into()))?;
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    if verify_email {
        sqlx::query("INSERT INTO auth_action_tokens(token_hash,user_id,purpose,expires_at) VALUES($1,$2,'verify_email',now()+interval '24 hours')")
            .bind(token_hash(&token)).bind(account.id).execute(&mut *tx).await?;
    } else {
        sqlx::query("INSERT INTO sessions(token_hash,user_id,expires_at) VALUES($1,$2,now()+interval '7 days')")
            .bind(token_hash(&token)).bind(account.id).execute(&mut *tx).await?;
    }
    // Регистрация не выбирает врача и не открывает чужую карточку по совпадению почты.
    if verify_email {
        tx.commit().await?;
        let url = state
            .delivery
            .confirmation_url(&token)
            .ok_or_else(ApiError::internal)?;
        if state
            .delivery
            .send_mail(
                &account.email,
                "Подтверждение электронной почты СИНАПС",
                &format!(
                    "Здравствуйте, {}. Чтобы подтвердить почту, откройте ссылку: {url}\nСсылка действует 24 часа.",
                    account.name
                ),
            )
            .await
            .is_err()
        {
            tracing::warn!("Не удалось отправить письмо подтверждения регистрации; пользователь может запросить повторную отправку");
        }
        return Ok((
            StatusCode::ACCEPTED,
            Json(RegistrationResult {
                verification_required: true,
                email: account.email,
            }),
        )
            .into_response());
    }
    tx.commit().await?;
    let mut response = session_response(&state, &token, account.public());
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RegistrationResult {
    pub verification_required: bool,
    pub email: String,
}

pub(crate) fn valid_email_address(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !value.chars().any(char::is_whitespace)
}
