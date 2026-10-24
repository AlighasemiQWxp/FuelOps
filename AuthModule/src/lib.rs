use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use async_trait::async_trait;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use fuel_core::{
    bson::{self, doc, Document},
    database::Actor,
    *,
};
use fuel_permission::User;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    org: String,
    sid: String,
    exp: usize,
    iss: String,
    aud: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Session {
    user_id: String,
    revoked: bool,
    expires_at: i64,
}
#[derive(Serialize, Deserialize)]
struct RefreshToken {
    session_id: String,
    org_id: String,
    station_id: String,
    consumed: bool,
    expires_at: i64,
}
#[derive(Deserialize)]
pub struct Login {
    pub organization: String,
    pub email: String,
    pub password: String,
}
#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    actor: Actor,
}

pub struct AuthModule;
#[async_trait]
impl Module for AuthModule {
    fn name(&self) -> &'static str {
        "AuthModule"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["PermissionModule"]
    }
    fn routes(&self) -> Router<AppState> {
        Router::new().route("/auth/me", get(me))
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        state.record_indexes("refresh_sessions").await?;
        state.index("refresh_tokens", doc! {"hash":1}, true).await?;
        state
            .db
            .collection::<Document>("auth_attempts")
            .create_index(
                mongodb::IndexModel::builder()
                    .keys(doc! {"expires":1})
                    .options(
                        mongodb::options::IndexOptions::builder()
                            .expire_after(std::time::Duration::from_secs(0))
                            .build(),
                    )
                    .build(),
            )
            .await?;
        Ok(())
    }
}
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
}
fn origin(state: &AppState, headers: &HeaderMap) -> ApiResult<()> {
    require(
        headers.get(header::ORIGIN).and_then(|h| h.to_str().ok())
            == Some(state.config.web_origin.as_str()),
        "Invalid request origin",
    )
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn cookie(state: &AppState, token: &str, max_age: i64) -> ApiResult<HeaderValue> {
    let secure = if state.config.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!("fuel_refresh={token}; Path=/api/v1/auth; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}")).map_err(|_|ApiError::validation("Invalid cookie"))
}
fn refresh_cookie(headers: &HeaderMap) -> ApiResult<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';')
                .find_map(|part| part.trim().strip_prefix("fuel_refresh="))
        })
        .map(str::to_owned)
        .ok_or_else(ApiError::unauthorized)
}
async fn rate_limit(state: &AppState, key: &str, limit: i64) -> ApiResult<()> {
    let bucket = now() / 900_000;
    let key = hash(&format!("{key}:{bucket}"));
    let row=state.db.collection::<Document>("auth_attempts").find_one_and_update(doc! {"_id":key},doc! {"$inc":{"count":1_i64},"$setOnInsert":{"expires":bson::DateTime::from_millis((bucket+2)*900_000)}}).upsert(true).return_document(mongodb::options::ReturnDocument::After).await?.ok_or_else(ApiError::conflict)?;
    if row.get_i64("count").unwrap_or(0) > limit {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many attempts; retry in 15 minutes",
        ));
    }
    Ok(())
}
async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<Login>,
) -> ApiResult<Response> {
    origin(&state, &headers)?;
    require(
        input.email.len() <= 254 && input.password.len() <= 1024 && input.organization.len() <= 80,
        "Invalid credentials",
    )?;
    let email = input.email.trim().to_lowercase();
    rate_limit(&state, &format!("ip:{}", peer.ip()), 60).await?;
    rate_limit(
        &state,
        &format!("account:{}:{email}", input.organization),
        12,
    )
    .await?;
    let raw = state
        .db
        .collection::<Document>("users")
        .find_one(doc! {"org_id":&input.organization,"data.email":&email,"data.active":true})
        .await?;
    let user: Option<Record<User>> = raw.map(bson::from_document).transpose()?;
    let password_hash = user.as_ref().map(|u| u.data.password_hash.clone());
    let supplied_password = input.password;
    let valid = tokio::task::spawn_blocking(move || {
        if let Some(encoded) = password_hash {
            return PasswordHash::new(&encoded).ok().is_some_and(|parsed| {
                Argon2::default()
                    .verify_password(supplied_password.as_bytes(), &parsed)
                    .is_ok()
            });
        }
        let _ = Argon2::default().hash_password(
            supplied_password.as_bytes(),
            &SaltString::generate(&mut OsRng),
        );
        false
    })
    .await
    .map_err(|_| ApiError::unauthorized())?;
    if !valid {
        state.db.collection::<Document>("audit_logs").insert_one(doc! {"id":fuel_core::id(),"org_id":&input.organization,"station_id":"","actor_id":"anonymous","actor_name":"Authentication","entity_id":hash(&email),"collection":"authentication","action":"login_failed","before_version":0_i64,"after_version":0_i64,"created_at":now()}).await?;
        return Err(ApiError::unauthorized());
    }
    let user = user.ok_or_else(ApiError::unauthorized)?;
    let actor = fuel_permission::resolve(&state, &user.org_id, &user.id, None).await?;
    let session = Record::new(
        &actor,
        Session {
            user_id: user.id,
            revoked: false,
            expires_at: now() + 7 * 86_400_000,
        },
    );
    let token = secret();
    let mut tx = state.begin(&actor).await?;
    tx.insert("refresh_sessions", &session).await?;
    tx.db().collection::<Document>("refresh_tokens").insert_one(doc! {"hash":hash(&token),"session_id":&session.id,"org_id":&actor.org_id,"station_id":&actor.station_id,"consumed":false,"expires_at":session.data.expires_at}).session(&mut tx.session).await?;
    tx.commit().await?;
    token_response(&state, actor, &session.id, token, 7 * 86_400)
}
fn token_response(
    state: &AppState,
    actor: Actor,
    session: &str,
    refresh: String,
    ttl: i64,
) -> ApiResult<Response> {
    let claims = Claims {
        sub: actor.user_id.clone(),
        org: actor.org_id.clone(),
        sid: session.into(),
        exp: (now() / 1000 + 600) as usize,
        iss: "fuel-ops".into(),
        aud: "fuel-ops-api".into(),
    };
    let access_token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )
    .map_err(|_| ApiError::unauthorized())?;
    Ok((
        [(header::SET_COOKIE, cookie(state, &refresh, ttl)?)],
        Json(TokenResponse {
            access_token,
            actor,
        }),
    )
        .into_response())
}
async fn refresh(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    origin(&state, &headers)?;
    let token = refresh_cookie(&headers)?;
    let raw = state
        .db
        .collection::<Document>("refresh_tokens")
        .find_one(doc! {"hash":hash(&token)})
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    let refresh: RefreshToken = bson::from_document(raw)?;
    let raw = state
        .db
        .collection::<Document>("refresh_sessions")
        .find_one(doc! {"org_id":&refresh.org_id,"id":&refresh.session_id})
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    let session: Record<Session> = bson::from_document(raw)?;
    if session.data.revoked || session.data.expires_at < now() {
        return Err(ApiError::unauthorized());
    }
    let actor = fuel_permission::resolve(
        &state,
        &refresh.org_id,
        &session.data.user_id,
        headers
            .get("x-station-id")
            .and_then(|value| value.to_str().ok())
            .or(Some(&refresh.station_id)),
    )
    .await?;
    let mut tx = state.begin(&actor).await?;
    let consumed = tx
        .db()
        .collection::<Document>("refresh_tokens")
        .update_one(
            doc! {"hash":hash(&token),"consumed":false},
            doc! {"$set":{"consumed":true}},
        )
        .session(&mut tx.session)
        .await?;
    if consumed.modified_count != 1 {
        tx.db()
            .collection::<Document>("refresh_sessions")
            .update_one(
                doc! {"id":&session.id,"org_id":&actor.org_id},
                doc! {"$set":{"data.revoked":true}},
            )
            .session(&mut tx.session)
            .await?;
        tx.commit().await?;
        return Err(ApiError::unauthorized());
    }
    let next = secret();
    tx.db().collection::<Document>("refresh_tokens").insert_one(doc! {"hash":hash(&next),"session_id":&session.id,"org_id":&actor.org_id,"station_id":&actor.station_id,"consumed":false,"expires_at":session.data.expires_at}).session(&mut tx.session).await?;
    tx.commit().await?;
    token_response(
        &state,
        actor,
        &session.id,
        next,
        (session.data.expires_at - now()) / 1000,
    )
}
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    origin(&state, &headers)?;
    if let Ok(token) = refresh_cookie(&headers) {
        if let Some(raw) = state
            .db
            .collection::<Document>("refresh_tokens")
            .find_one(doc! {"hash":hash(&token)})
            .await?
        {
            let refresh: RefreshToken = bson::from_document(raw)?;
            state
                .db
                .collection::<Document>("refresh_sessions")
                .update_one(
                    doc! {"id":refresh.session_id,"org_id":refresh.org_id},
                    doc! {"$set":{"data.revoked":true}},
                )
                .await?;
        }
    }
    Ok((
        [(header::SET_COOKIE, cookie(&state, "", 0)?)],
        StatusCode::NO_CONTENT,
    )
        .into_response())
}
async fn me(Extension(actor): Extension<Actor>) -> Json<Actor> {
    Json(actor)
}
pub async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> ApiResult<Response> {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(ApiError::unauthorized)?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&["fuel-ops"]);
    validation.set_audience(&["fuel-ops-api"]);
    let claims = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
        &validation,
    )
    .map_err(|_| ApiError::unauthorized())?
    .claims;
    let active=state.db.collection::<Document>("refresh_sessions").find_one(doc! {"org_id":&claims.org,"id":&claims.sid,"data.user_id":&claims.sub,"data.revoked":false,"data.expires_at":{"$gt":now()}}).await?;
    if active.is_none() {
        return Err(ApiError::unauthorized());
    }
    let station = request
        .headers()
        .get("x-station-id")
        .and_then(|v| v.to_str().ok());
    let actor = fuel_permission::resolve(&state, &claims.org, &claims.sub, station).await?;
    request.extensions_mut().insert(actor);
    Ok(next.run(request).await)
}
