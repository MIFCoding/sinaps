use crate::*;

#[derive(Serialize, utoipa::ToSchema)]
pub struct AuthConfig {
    email_actions_available: bool,
    email_verification_required: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/config",
    operation_id = "auth_config",
    responses((status = 200, body = AuthConfig))
)]
pub async fn config(State(state): State<AppState>) -> Json<AuthConfig> {
    Json(AuthConfig {
        email_actions_available: state.delivery.has_mail(),
        email_verification_required: state.delivery.has_mail(),
    })
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct EmailInput {
    email: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PasswordResetInput {
    token: String,
    password: String,
    password_confirmation: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct EmailTokenInput {
    token: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/verify-email",
    operation_id = "verify_email",
    request_body = EmailTokenInput,
    params(("X-Synapse-Request" = String, Header)),
    responses(
        (status = 200, body = synapse_domain::User),
        (status = 400, body = crate::ErrorEnvelope),
        (status = 404, body = crate::ErrorEnvelope),
        (status = 500, body = crate::ErrorEnvelope)
    )
)]
pub async fn verify_email(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(input): Json<EmailTokenInput>,
) -> ApiResult<Response> {
    check_auth_limit(&state, address.ip(), "verify_email")?;
    let token = input.token;
    if !valid_token(&token) {
        return Err(ApiError::not_found());
    }
    let mut tx = state.db.begin().await?;
    let account = sqlx::query_as::<_, Account>(
        "SELECT u.* FROM auth_action_tokens t JOIN users u ON u.id=t.user_id
         WHERE t.token_hash=$1 AND t.purpose='verify_email' AND t.consumed_at IS NULL
           AND t.expires_at>now() AND u.activated FOR UPDATE OF t",
    )
    .bind(token_hash(&token.to_lowercase()))
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    sqlx::query("UPDATE users SET email_verified=true WHERE id=$1")
        .bind(account.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE auth_action_tokens SET consumed_at=now() WHERE token_hash=$1")
        .bind(token_hash(&token.to_lowercase()))
        .execute(&mut *tx)
        .await?;
    let session = fresh_token();
    sqlx::query(
        "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES($1,$2,now()+interval '7 days')",
    )
    .bind(token_hash(&session))
    .bind(account.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(session_response(&state, &session, account.public()))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/verification-email",
    operation_id = "request_email_verification",
    request_body = EmailInput,
    params(("X-Synapse-Request" = String, Header)),
    responses((status = 204), (status = 429, body = crate::ErrorEnvelope))
)]
pub async fn request_verification(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(input): Json<EmailInput>,
) -> ApiResult<StatusCode> {
    check_auth_limit(&state, address.ip(), "request_verification")?;
    if !state.delivery.has_mail() {
        return Ok(StatusCode::NO_CONTENT);
    }
    let email = synapse_domain::normalize_login(&input.email);
    let account: Option<Account> =
        sqlx::query_as("SELECT * FROM users WHERE email=$1 AND activated AND NOT email_verified")
            .bind(email)
            .fetch_optional(&state.db)
            .await?;
    if let Some(account) = account {
        let token = fresh_token();
        let mut tx = state.db.begin().await?;
        sqlx::query("DELETE FROM auth_action_tokens WHERE user_id=$1 AND purpose='verify_email' AND consumed_at IS NULL")
            .bind(account.id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO auth_action_tokens(token_hash,user_id,purpose,expires_at) VALUES($1,$2,'verify_email',now()+interval '24 hours')")
            .bind(token_hash(&token)).bind(account.id).execute(&mut *tx).await?;
        tx.commit().await?;
        if let Some(url) = state.delivery.confirmation_url(&token)
            && state
                .delivery
                .send_mail(
                    &account.email,
                    "Подтверждение электронной почты СИНАПС",
                    &format!("Чтобы подтвердить электронную почту, откройте ссылку: {url}\nСсылка действует 24 часа."),
                )
                .await
                .is_err()
        {
            tracing::warn!("Не удалось повторно отправить письмо подтверждения");
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/password-reset",
    operation_id = "request_password_reset",
    request_body = EmailInput,
    params(("X-Synapse-Request" = String, Header)),
    responses((status = 204), (status = 429, body = crate::ErrorEnvelope))
)]
pub async fn request_password_reset(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(input): Json<EmailInput>,
) -> ApiResult<StatusCode> {
    check_auth_limit(&state, address.ip(), "request_password_reset")?;
    if !state.delivery.has_mail() {
        return Ok(StatusCode::NO_CONTENT);
    }
    let email = synapse_domain::normalize_login(&input.email);
    let account: Option<Account> =
        sqlx::query_as("SELECT * FROM users WHERE email=$1 AND activated AND email_verified")
            .bind(email)
            .fetch_optional(&state.db)
            .await?;
    if let Some(account) = account {
        let token = fresh_token();
        let mut tx = state.db.begin().await?;
        sqlx::query("DELETE FROM auth_action_tokens WHERE user_id=$1 AND purpose='reset_password' AND consumed_at IS NULL")
            .bind(account.id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO auth_action_tokens(token_hash,user_id,purpose,expires_at) VALUES($1,$2,'reset_password',now()+interval '30 minutes')")
            .bind(token_hash(&token)).bind(account.id).execute(&mut *tx).await?;
        tx.commit().await?;
        if let Some(url) = state.delivery.reset_url(&token)
            && state
                .delivery
                .send_mail(
                    &account.email,
                    "Сброс пароля СИНАПС",
                    &format!("Чтобы задать новый пароль, откройте ссылку: {url}\nСсылка действует 30 минут. Если вы не запрашивали сброс, проигнорируйте это письмо."),
                )
                .await
                .is_err()
        {
            tracing::warn!("Не удалось отправить письмо сброса пароля");
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/password-reset/complete",
    operation_id = "complete_password_reset",
    request_body = PasswordResetInput,
    params(("X-Synapse-Request" = String, Header)),
    responses(
        (status = 204),
        (status = 422, body = crate::ErrorEnvelope),
        (status = 429, body = crate::ErrorEnvelope)
    )
)]
pub async fn complete_password_reset(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(input): Json<PasswordResetInput>,
) -> ApiResult<StatusCode> {
    check_auth_limit(&state, address.ip(), "complete_password_reset")?;
    if !valid_token(&input.token) || input.password != input.password_confirmation {
        return Err(ApiError::invalid(
            "Ссылка недействительна или пароли не совпадают",
        ));
    }
    synapse_domain::validate_new_password(&input.password).map_err(ApiError::invalid)?;
    let mut tx = state.db.begin().await?;
    let user_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM auth_action_tokens WHERE token_hash=$1 AND purpose='reset_password'
         AND consumed_at IS NULL AND expires_at>now() FOR UPDATE",
    )
    .bind(token_hash(&input.token.to_lowercase()))
    .fetch_optional(&mut *tx)
    .await?;
    let user_id =
        user_id.ok_or_else(|| ApiError::invalid("Ссылка сброса истекла или уже использована"))?;
    let password = input.password;
    let password_hash = tokio::task::spawn_blocking(move || seed::hash(&password))
        .await
        .map_err(|_| ApiError::internal())?;
    sqlx::query("UPDATE users SET password_hash=$1 WHERE id=$2")
        .bind(password_hash)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=$1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE auth_action_tokens SET consumed_at=now() WHERE user_id=$1 AND purpose='reset_password' AND consumed_at IS NULL")
        .bind(user_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn fresh_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.chars().all(|character| character.is_ascii_hexdigit())
}
