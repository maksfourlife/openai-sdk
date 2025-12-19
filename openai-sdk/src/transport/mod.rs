use std::sync::LazyLock;

use reqwest::{Method, RequestBuilder, Url};
use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use snafu::ResultExt;

use crate::{DeserializeResponseSnafu, OpenAIError};

#[cfg(feature = "responses-streaming")]
pub mod streaming;

static BASE_URL: LazyLock<Url> = LazyLock::new(|| "https://api.openai.com/".parse().unwrap());

pub trait Transport {
    fn send<P, R>(
        &self,
        method: Method,
        path: &str,
        params: Option<&P>,
    ) -> impl Future<Output = Result<R, OpenAIError>> + Send
    where
        P: Sync + Serialize,
        R: DeserializeOwned;
}

#[derive(Clone)]
pub struct StandardHttpTransport {
    access_token: SecretString,
    client: reqwest::Client,
}

impl StandardHttpTransport {
    pub fn new(access_token: SecretString, client: reqwest::Client) -> Self {
        Self {
            access_token,
            client,
        }
    }

    fn prepare_request<P>(
        &self,
        method: Method,
        path: &str,
        params: Option<&P>,
    ) -> Result<RequestBuilder, OpenAIError>
    where
        P: Sync + Serialize,
    {
        let mut builder = self
            .client
            .request(method.clone(), BASE_URL.join(path)?)
            .bearer_auth(self.access_token.expose_secret());

        if let Some(params) = params {
            if method == Method::GET {
                builder = builder.query(params);
            } else if method == Method::POST {
                builder = builder.json(params);
            } else {
                unimplemented!("Method {method} not supported");
            }
        }

        Ok(builder)
    }
}

impl Transport for StandardHttpTransport {
    async fn send<P, R>(
        &self,
        method: Method,
        path: &str,
        params: Option<&P>,
    ) -> Result<R, OpenAIError>
    where
        P: Sync + Serialize,
        R: DeserializeOwned,
    {
        let builder = self.prepare_request(method.clone(), path, params)?;

        let response = builder.send().await?;

        let status = response.status();
        let text = response.text().await?;

        if status.is_success() {
            serde_json::from_str(&text).context(DeserializeResponseSnafu { text })
        } else {
            Err(OpenAIError::Api { status, text })
        }
    }
}
