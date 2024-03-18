use crate::grants::authorization_code::AuthorizationCodeCookieConfig;
use crate::{url, OAuth2ClientStore, COOKIE_NAME};
use async_trait::async_trait;
use axum::response::{IntoResponse, IntoResponseParts, ResponseParts};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
    response::Response,
    Extension, RequestPartsExt,
};
use axum_extra::extract;
use cookie::{Cookie, Key};
use http::HeaderMap;
use loco_rs::prelude::AppContext;
use oauth2::basic::BasicTokenResponse;
use oauth2::TokenResponse;
use std::convert::Infallible;

#[derive(Clone)]
pub struct OAuth2PrivateCookieJar(extract::cookie::PrivateCookieJar);

impl IntoResponse for OAuth2PrivateCookieJar {
    fn into_response(self) -> Response {
        self.0.into_response()
    }
}

impl IntoResponseParts for OAuth2PrivateCookieJar {
    type Error = Infallible;
    fn into_response_parts(self, res: ResponseParts) -> Result<ResponseParts, Self::Error> {
        self.0.into_response_parts(res)
    }
}

impl AsMut<extract::cookie::PrivateCookieJar> for OAuth2PrivateCookieJar {
    fn as_mut(&mut self) -> &mut extract::cookie::PrivateCookieJar {
        &mut self.0
    }
}

impl OAuth2PrivateCookieJar {
    pub fn add<C: Into<Cookie<'static>>>(mut self, cookie: C) -> Self {
        Self(self.0.add(cookie.into()))
    }
    pub fn from_headers(headers: &HeaderMap, key: Key) -> Self {
        Self(extract::cookie::PrivateCookieJar::from_headers(
            headers, key,
        ))
    }
    pub fn get(&self, name: &str) -> Option<Cookie<'static>> {
        self.0.get(name)
    }
    pub fn remove<C: Into<Cookie<'static>>>(mut self, cookie: C) -> Self {
        Self(self.0.remove(cookie.into()))
    }
    pub fn iter(&self) -> impl Iterator<Item = Cookie<'static>> + '_ {
        self.0.iter()
    }

    pub fn decrypt(&self, cookie: Cookie<'static>) -> Option<Cookie<'static>> {
        self.0.decrypt(cookie)
    }
}
#[async_trait]
pub trait OAuth2PrivateCookieJarTrait: Clone {
    /// Create a short live cookie with the token response
    ///
    /// # Arguments
    /// config - The authorization code config with the oauth2 authorization code
    /// grant configuration token - The token response from the oauth2 authorization
    /// code grant jar - The private cookie jar
    ///
    /// # Returns
    /// A result with the private cookie jar
    ///
    /// # Errors
    /// When url parsing fails
    fn create_short_live_cookie_with_token_response(
        config: &AuthorizationCodeCookieConfig,
        token: &BasicTokenResponse,
        jar: Self,
    ) -> loco_rs::prelude::Result<Self>;
}

impl OAuth2PrivateCookieJarTrait for OAuth2PrivateCookieJar {
    fn create_short_live_cookie_with_token_response(
        config: &AuthorizationCodeCookieConfig,
        token: &BasicTokenResponse,
        jar: Self,
    ) -> loco_rs::prelude::Result<Self> {
        // Set the cookie
        let secs: i64 = token
            .expires_in()
            .unwrap_or(std::time::Duration::new(0, 0))
            .as_secs()
            .try_into()
            .map_err(|_e| loco_rs::errors::Error::InternalServerError)?;
        // domain
        let protected_url = config
            .protected_url
            .clone()
            .unwrap_or_else(|| "http://localhost:3000/oauth2/protected".to_string());
        let protected_url = url::Url::parse(&protected_url)
            .map_err(|_e| loco_rs::errors::Error::InternalServerError)?;
        let protected_domain = protected_url.domain().unwrap_or("localhost");
        let protected_path = protected_url.path();
        // Create the cookie with the session id, domain, path, and secure flag from
        // the token and profile
        let cookie = cookie::Cookie::build((COOKIE_NAME, token.access_token().secret().to_owned()))
            .domain(protected_domain.to_owned())
            .path(protected_path.to_owned())
            // secure flag is for https - https://datatracker.ietf.org/doc/html/rfc6749#section-3.1.2.1
            .secure(true)
            // Restrict access in the client side code to prevent XSS attacks
            .http_only(true)
            .max_age(time::Duration::seconds(secs));
        Ok(jar.add(cookie))
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for OAuth2PrivateCookieJar
where
    S: Send + Sync,
    AppContext: FromRef<S>,
{
    type Rejection = Response;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> core::result::Result<Self, Self::Rejection> {
        let Extension(store) = parts
            .extract::<Extension<OAuth2ClientStore>>()
            .await
            .map_err(|err| err.into_response())?;
        let key = store.key.clone();
        let jar = extract::cookie::PrivateCookieJar::from_headers(&parts.headers, key);
        Ok(Self(jar))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::StatusCode;
    use axum::routing::get;
    use axum::Router;
    use axum_extra::extract::PrivateCookieJar;
    use axum_test::TestServer;
    use http::header::{HeaderValue, COOKIE};
    use loco_rs::config::{Config, Database, Middlewares, Server};
    use loco_rs::environment::Environment;
    use serde_json::json;
    use std::collections::BTreeMap;

    // Helper function to create a Key for encryption/decryption
    fn create_key() -> Key {
        Key::generate()
    }
    // Helper function to create a default AppContext for testing
    fn create_default_app_context() -> AppContext {
        AppContext {
            environment: Environment::Production,
            db: Default::default(),
            redis: None,
            config: Config {
                logger: Default::default(),
                server: Server {
                    binding: "test-binding".to_string(),
                    port: 8080,
                    host: "test-host".to_string(),
                    ident: None,
                    middlewares: Middlewares {
                        compression: None,
                        etag: None,
                        limit_payload: None,
                        logger: None,
                        catch_panic: None,
                        timeout_request: None,
                        cors: None,
                        static_assets: None,
                    },
                },
                database: Database {
                    uri: "".to_string(),
                    enable_logging: false,
                    min_connections: 0,
                    max_connections: 0,
                    connect_timeout: 0,
                    idle_timeout: 0,
                    auto_migrate: false,
                    dangerously_truncate: false,
                    dangerously_recreate: false,
                },
                redis: None,
                auth: None,
                workers: Default::default(),
                mailer: None,
                settings: None,
            },
            mailer: None,
            storage: None,
        }
    }
    fn cookies_from_request(headers: &HeaderMap) -> impl Iterator<Item = Cookie<'static>> + '_ {
        headers
            .get_all(COOKIE)
            .into_iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|cookie| Cookie::parse_encoded(cookie.to_owned()).ok())
    }

    #[tokio::test]
    async fn test_add_and_get_cookie() {
        let key = create_key();
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static(""));
        let jar = OAuth2PrivateCookieJar::from_headers(&headers, key.clone());

        let cookie_name = "test_cookie";
        let cookie_value = "test_value";
        let cookie = Cookie::build((cookie_name, cookie_value)).http_only(true);

        // Add a cookie
        let jar = jar.add(cookie.clone());

        // Attempt to retrieve the added cookie
        let retrieved_cookie = jar.get(cookie_name).expect("Cookie was not found");

        assert_eq!(
            retrieved_cookie.value(),
            cookie_value,
            "Retrieved cookie does not match the added cookie"
        );
    }

    #[tokio::test]
    async fn test_remove_cookie() {
        let key = create_key();
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static(""));
        let jar = OAuth2PrivateCookieJar::from_headers(&headers, key.clone());

        let cookie_name = "test_cookie";
        let cookie_value = "test_value";
        let cookie = Cookie::build((cookie_name, cookie_value)).http_only(true);

        // Add a cookie
        let jar = jar.add(cookie.clone());

        // Remove the added cookie
        let jar = jar.remove(cookie.clone());

        // Attempt to retrieve the removed cookie
        let retrieved_cookie = jar.get(cookie_name);

        assert!(
            retrieved_cookie.is_none(),
            "Retrieved cookie was found, but it should have been removed"
        );
    }

    #[tokio::test]
    async fn test_decrypt_cookie() {
        let key = create_key();
        let cookie_name = "test_cookie";
        let cookie_value = "test_value";
        let cookie = Cookie::new(cookie_name, cookie_value);

        // Create a PrivateCookieJar and add a cookie to it
        let jar = PrivateCookieJar::new(key.clone());
        let jar = jar.add(cookie);

        // Simulate sending the jar in a response to encrypt the cookie
        let response = jar.into_response();

        // Extract the 'Set-Cookie' header from the response
        let encrypted_cookie_value = response
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("Cookie was not set in response");

        // Simulate receiving a request with the encrypted cookie
        let mut headers = HeaderMap::new();
        headers.insert("cookie", encrypted_cookie_value.parse().unwrap());
        let private_jar = PrivateCookieJar::from_headers(&HeaderMap::new(), key.clone());
        let mut original_cookie = None;
        for cookie in cookies_from_request(&headers) {
            if let Some(cookie) = private_jar.decrypt(cookie) {
                original_cookie = Some(cookie);
            }
        }
        let original_cookie = original_cookie.expect("Failed to decrypt cookie");
        // Attempt to retrieve and decrypt the cookie
        assert_eq!(
            original_cookie.value(),
            cookie_value,
            "Decrypted cookie value does not match original"
        );
    }

    #[tokio::test]
    async fn test_iter_cookies() {
        let key = create_key();
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static(""));
        let jar = OAuth2PrivateCookieJar::from_headers(&headers, key.clone());

        let cookie_name = "test_cookie";
        let cookie_value = "test_value";
        let cookie = Cookie::build((cookie_name, cookie_value)).http_only(true);

        // Add a cookie
        let jar = jar.add(cookie.clone());

        // Iterate over the cookies
        let mut iter = jar.iter();
        let retrieved_cookie = iter.next().expect("Cookie was not found");

        assert_eq!(
            retrieved_cookie.value(),
            cookie_value,
            "Retrieved cookie does not match the added cookie"
        );
    }
    #[tokio::test]
    async fn test_from_request_parts() {
        let oauth2_store = OAuth2ClientStore {
            key: create_key(),
            clients: BTreeMap::new(),
        };

        let routes = Router::new()
            .route("/", get(|_: OAuth2PrivateCookieJar| async move { "OK" }))
            .with_state(create_default_app_context())
            .layer(Extension(oauth2_store.clone()));
        // Run the application for testing.
        let server = TestServer::new(routes).unwrap();
        // Simulate a request
        let response = server.get("/").json(&json!({})).await;
        let response_status = StatusCode::from_u16(response.status_code().as_u16()).unwrap();
        assert_eq!(response_status, StatusCode::OK);
    }
}
