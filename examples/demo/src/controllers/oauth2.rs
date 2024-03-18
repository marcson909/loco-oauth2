#![allow(clippy::unused_async)]

use axum_session::SessionNullPool;
use loco_oauth2::controllers::{
    middleware::auth::OAuth2CookieUser,
    oauth2::{google_authorization_url, google_callback},
};
use loco_rs::prelude::*;

use crate::models::{o_auth2_sessions, users, users::OAuth2UserProfile};

async fn protected(
    user: OAuth2CookieUser<OAuth2UserProfile, users::Model, o_auth2_sessions::Model>,
) -> Result<impl IntoResponse> {
    let user: &users::Model = user.as_ref();
    Ok(format!("You are protected! Email: {}", user.email.as_str()))
}
pub fn routes() -> Routes {
    Routes::new()
        .prefix("api/oauth2")
        .add("/google", get(google_authorization_url::<SessionNullPool>))
        .add(
            "/google/callback",
            get(google_callback::<
                OAuth2UserProfile,
                users::Model,
                o_auth2_sessions::Model,
                SessionNullPool,
            >),
        )
        .add("/protected", get(protected))
}
