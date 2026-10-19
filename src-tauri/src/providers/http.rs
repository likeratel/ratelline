use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use futures::{future::BoxFuture, StreamExt};
use reqwest::{header::HeaderValue, redirect::Policy, StatusCode, Url};
use serde::de::DeserializeOwned;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) enum ApiOrigin {
    Cloudflare,
    FirebaseHosting,
    FirebaseAppHosting,
}

impl ApiOrigin {
    fn url(self) -> &'static str {
        match self {
            Self::Cloudflare => "https://api.cloudflare.com/client/v4/",
            Self::FirebaseHosting => "https://firebasehosting.googleapis.com/",
            Self::FirebaseAppHosting => "https://firebaseapphosting.googleapis.com/",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum HttpError {
    Unauthorized,
    Forbidden,
    NotFound,
    RateLimited,
    Status(u16),
    Transport,
    ResponseTooLarge,
    InvalidResponse,
    InvalidRequest,
}

impl HttpError {
    pub(crate) fn user_message(&self, service: &str) -> String {
        match self {
            Self::Unauthorized => format!("{service} rejected the selected credential."),
            Self::Forbidden => format!(
                "{service} denied this read-only request. Check the credential permissions."
            ),
            Self::NotFound => format!("{service} could not find the selected resource."),
            Self::RateLimited => format!("{service} rate-limited this request. Try again later."),
            Self::Status(status) => format!("{service} returned HTTP {status}."),
            Self::Transport => format!("{service} could not be reached."),
            Self::ResponseTooLarge => format!("{service} returned a response larger than 2 MiB."),
            Self::InvalidResponse => format!("{service} returned an invalid response."),
            Self::InvalidRequest => format!("The {service} request was invalid."),
        }
    }
}

pub(crate) struct JsonHttpClient {
    client: Arc<reqwest::Client>,
    origin: ApiOrigin,
}

impl JsonHttpClient {
    pub(crate) fn new(origin: ApiOrigin) -> Result<Self, HttpError> {
        let client = shared_client()?;
        Ok(Self { client, origin })
    }

    pub(crate) fn get<T: DeserializeOwned + Send + 'static>(
        &self,
        path: &[&str],
        query: &[(&str, &str)],
        bearer_token: &str,
        quota_project: Option<&str>,
    ) -> BoxFuture<'static, Result<T, HttpError>> {
        let prepared = prepare_request(self.origin, path, query, bearer_token, quota_project);
        let client = self.client.clone();
        Box::pin(async move {
            let (url, token, quota_project) = prepared?;
            let mut request = client.get(url).bearer_auth(token);
            if let Some(project) = quota_project {
                request = request.header("X-Goog-User-Project", project);
            }
            let response = request.send().await.map_err(|_| HttpError::Transport)?;
            if !response.status().is_success() {
                return Err(status_error(response.status()));
            }
            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| HttpError::Transport)?;
                if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                    return Err(HttpError::ResponseTooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            serde_json::from_slice(&bytes).map_err(|_| HttpError::InvalidResponse)
        })
    }
}

fn shared_client() -> Result<Arc<reqwest::Client>, HttpError> {
    static CLIENT: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = Arc::new(
        reqwest::Client::builder()
            .redirect(Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent("ratelline/0.1")
            .build()
            .map_err(|_| HttpError::InvalidRequest)?,
    );
    let _ = CLIENT.set(client.clone());
    Ok(CLIENT.get().cloned().unwrap_or(client))
}

fn prepare_request(
    origin: ApiOrigin,
    path: &[&str],
    query: &[(&str, &str)],
    bearer_token: &str,
    quota_project: Option<&str>,
) -> Result<(Url, String, Option<HeaderValue>), HttpError> {
    if bearer_token.trim().is_empty() || bearer_token.len() > 16_384 {
        return Err(HttpError::InvalidRequest);
    }
    let quota_project = quota_project
        .map(HeaderValue::from_str)
        .transpose()
        .map_err(|_| HttpError::InvalidRequest)?;
    Ok((
        build_url(origin, path, query)?,
        bearer_token.to_owned(),
        quota_project,
    ))
}

fn build_url(origin: ApiOrigin, path: &[&str], query: &[(&str, &str)]) -> Result<Url, HttpError> {
    if path.iter().any(|segment| {
        segment.is_empty()
            || *segment == "."
            || *segment == ".."
            || segment.len() > 1024
            || segment.chars().any(char::is_control)
    }) || query.len() > 16
        || query.iter().any(|(key, value)| {
            key.is_empty()
                || key.len() > 128
                || value.len() > 4096
                || key.chars().any(char::is_control)
                || value.chars().any(char::is_control)
        })
    {
        return Err(HttpError::InvalidRequest);
    }
    let mut url = Url::parse(origin.url()).map_err(|_| HttpError::InvalidRequest)?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| HttpError::InvalidRequest)?;
        segments.pop_if_empty();
        segments.extend(path.iter().copied());
    }
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    Ok(url)
}

fn status_error(status: StatusCode) -> HttpError {
    match status {
        StatusCode::UNAUTHORIZED => HttpError::Unauthorized,
        StatusCode::FORBIDDEN => HttpError::Forbidden,
        StatusCode::NOT_FOUND => HttpError::NotFound,
        StatusCode::TOO_MANY_REQUESTS => HttpError::RateLimited,
        _ => HttpError::Status(status.as_u16()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_origin_builder_encodes_untrusted_segments_and_query() {
        let url = build_url(
            ApiOrigin::Cloudflare,
            &["accounts", "account/with space", "pages", "projects"],
            &[("pageToken", "token&next=forged")],
        )
        .expect("safe URL");

        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("api.cloudflare.com"));
        assert!(url.path().contains("account%2Fwith%20space"));
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![("pageToken".into(), "token&next=forged".into())]
        );
    }

    #[test]
    fn fixed_origin_builder_rejects_traversal_and_control_text() {
        assert_eq!(
            build_url(ApiOrigin::FirebaseHosting, &[".."], &[]),
            Err(HttpError::InvalidRequest)
        );
        assert_eq!(
            build_url(
                ApiOrigin::FirebaseAppHosting,
                &["v1beta"],
                &[("x", "bad\n")]
            ),
            Err(HttpError::InvalidRequest)
        );
    }

    #[test]
    fn clients_share_one_connection_pool() {
        let cloudflare = JsonHttpClient::new(ApiOrigin::Cloudflare).unwrap();
        let firebase = JsonHttpClient::new(ApiOrigin::FirebaseHosting).unwrap();
        assert!(Arc::ptr_eq(&cloudflare.client, &firebase.client));
    }
}
