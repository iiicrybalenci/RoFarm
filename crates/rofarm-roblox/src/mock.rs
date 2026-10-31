use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use crate::api::{ApiRequest, ApiResponse, HttpTransport};
use crate::error::RobloxResult;
use crate::net::{safe_path, NetPolicy, NetworkLogEntry};

#[derive(Debug, Clone, Default)]
pub struct MockTransport {
    routes: Arc<Mutex<HashMap<String, VecDeque<ApiResponse>>>>,
    pub requests: Arc<Mutex<Vec<ApiRequest>>>,
    policy: Option<Arc<NetPolicy>>,
}

impl MockTransport {
    fn key(method: &str, url: &str) -> String {
        format!("{} {}", method.to_ascii_uppercase(), url)
    }

    pub fn with_policy(mut self, policy: Arc<NetPolicy>) -> Self {
        self.policy = Some(policy);
        self
    }

    fn push(&self, method: &str, url: &str, response: ApiResponse) {
        self.routes
            .lock()
            .entry(Self::key(method, url))
            .or_default()
            .push_back(response);
    }

    pub fn with_json(self, url: &str, body: &str) -> Self {
        self.push("GET", url, ApiResponse::json(body));
        self
    }

    pub fn with_post_json(self, url: &str, body: &str) -> Self {
        self.push("POST", url, ApiResponse::json(body));
        self
    }

    pub fn with_status(self, url: &str, status: u16, body: &str) -> Self {
        self.push("GET", url, ApiResponse { status, headers: BTreeMap::new(), body: body.into() });
        self
    }

    pub fn with_response(self, url: &str, status: u16, body: &str, headers: BTreeMap<String, String>) -> Self {
        self.push("POST", url, ApiResponse { status, headers, body: body.into() });
        self
    }

    pub fn recorded(&self) -> Vec<ApiRequest> {
        self.requests.lock().clone()
    }
}

#[async_trait]
impl HttpTransport for MockTransport {
    async fn send(&self, request: ApiRequest) -> RobloxResult<ApiResponse> {
        self.requests.lock().push(request.clone());
        if let Some(policy) = &self.policy {
            if let Ok(url) = url::Url::parse(&request.url) {
                let (allowed, reason) = policy.is_allowed(&url);
                policy.record(NetworkLogEntry {
                    at: chrono::Utc::now(),
                    method: request.method.clone(),
                    host: url.host_str().unwrap_or_default().into(),
                    path: safe_path(&url),
                    status: None,
                    duration_ms: 0,
                    request_bytes: request.body.as_ref().map(|b| b.len() as u64).unwrap_or(0),
                    response_bytes: 0,
                    allowed,
                    reason,
                });
            }
        }
        let key = Self::key(&request.method, &request.url);
        let mut routes = self.routes.lock();
        if let Some(queue) = routes.get_mut(&key) {
            if let Some(response) = queue.pop_front() {
                return Ok(response);
            }
        }
        Ok(ApiResponse { status: 404, headers: BTreeMap::new(), body: "{}".into() })
    }
}
