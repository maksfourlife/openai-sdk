use reqwest::StatusCode;
use secrecy::SecretString;
use snafu::Snafu;

use crate::transport::StandardHttpTransport;

#[cfg(feature = "responses")]
pub mod responses;

pub mod transport;

pub mod models;

mod macros;

#[derive(Debug, Snafu)]
pub enum OpenAIError {
    #[snafu(transparent)]
    UrlParse { source: url::ParseError },
    #[snafu(transparent)]
    Reqwest { source: reqwest::Error },
    #[snafu(display("Could not deserialize response: {source}"))]
    DeserializeResponse {
        source: serde_json::Error,
        text: String,
    },
    #[snafu(display("ApiError ({status}): {text}"))]
    Api { status: StatusCode, text: String },
}

#[derive(Clone)]
pub struct OpenAI<T = StandardHttpTransport> {
    transport: T,
}

impl OpenAI<StandardHttpTransport> {
    pub fn standard_http(access_token: SecretString, client: reqwest::Client) -> Self {
        Self {
            transport: StandardHttpTransport::new(access_token, client),
        }
    }
}

impl<T> OpenAI<T> {
    #[cfg(feature = "responses")]
    pub fn responses<Stream>(&self) -> responses::ResponsesHandler<'_, T, Stream> {
        responses::ResponsesHandler {
            client: self,
            _marker: std::marker::PhantomData,
        }
    }
}
