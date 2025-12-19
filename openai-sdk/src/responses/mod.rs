use std::marker::PhantomData;

use reqwest::Method;
use serde::Serialize;
use serde_bool::False;
use serde_with::skip_serializing_none;

use crate::{
    OpenAI, OpenAIError,
    models::responses::{Response, ResponseIdRef, ResponseInput},
    transport::Transport,
};

#[cfg(feature = "responses-streaming")]
mod streaming;

pub struct ResponsesHandler<'a, T, Stream> {
    pub(crate) client: &'a OpenAI<T>,
    pub(crate) _marker: PhantomData<Stream>,
}

impl<T: Transport> ResponsesHandler<'_, T, False> {
    /// Creates a model response. Provide [text](https://platform.openai.com/docs/guides/text) or [image](https://platform.openai.com/docs/guides/images) inputs to generate [text](https://platform.openai.com/docs/guides/text) or [JSON](https://platform.openai.com/docs/guides/structured-outputs) outputs. Have the model call your own [custom code](https://platform.openai.com/docs/guides/function-calling) or use built-in [tools](https://platform.openai.com/docs/guides/tools) like [web search](https://platform.openai.com/docs/guides/tools-web-search) or [file search](https://platform.openai.com/docs/guides/tools-file-search) to use your own data as input for the model's response.
    ///
    /// https://platform.openai.com/docs/api-reference/responses/create
    pub async fn create(&self, params: &ResponseParams<False>) -> Result<Response, OpenAIError> {
        self.client
            .transport
            .send(Method::POST, "/v1/responses", Some(params))
            .await
    }

    /// Retrieves a model response with the given ID.
    ///
    /// https://platform.openai.com/docs/api-reference/responses/get
    pub async fn get(&self, id: &ResponseIdRef) -> Result<Response, OpenAIError> {
        self.client
            .transport
            .send(Method::GET, &format!("/v1/responses/{id}"), None::<&()>)
            .await
    }
}

impl<T: Transport, Stream> ResponsesHandler<'_, T, Stream> {
    /// Deletes a model response with the given ID.
    ///
    /// https://platform.openai.com/docs/api-reference/responses/delete
    pub async fn delete(&self, id: &ResponseIdRef) -> Result<(), OpenAIError> {
        self.client
            .transport
            .send(Method::DELETE, &format!("/v1/responses/{id}"), None::<&()>)
            .await
    }

    /// Cancels a model response with the given ID. Only responses created with the `background` parameter set to `true` can be cancelled. [Learn more.](https://platform.openai.com/docs/guides/background)
    ///
    /// https://platform.openai.com/docs/api-reference/responses/cancel
    pub async fn cancel(&self, id: &ResponseIdRef) -> Result<Response, OpenAIError> {
        self.client
            .transport
            .send(
                Method::POST,
                &format!("/v1/responses/{id}/cancel"),
                None::<&()>,
            )
            .await
    }
}

/// https://platform.openai.com/docs/api-reference/responses/create
#[skip_serializing_none]
#[derive(Debug, Default, Serialize)]
pub struct ResponseParams<Stream = False> {
    /// Whether to run the model response in the background. [Learn more.](https://platform.openai.com/docs/guides/background)
    /// Default: false
    pub background: Option<bool>,
    // TODO: conversation
    /// Text, image, or file inputs to the model, used to generate a response.
    pub input: Option<ResponseInput>,
    /// If set to true, the model response data will be streamed to the client as it is generated using [server-sent events.](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events#Event_stream_format) See the [Streaming section below](https://platform.openai.com/docs/api-reference/responses-streaming) for more information.
    pub stream: Stream,
}

#[cfg(test)]
mod test {
    use dotenv_codegen::dotenv;
    use serde_bool::False;

    use crate::{OpenAI, OpenAIError};

    const OPENAI_API_KEY: &str = dotenv!("OPENAI_API_KEY");
    const RESPONSE_ID: &str = dotenv!("RESPONSE_ID");

    #[tokio::test]
    async fn test_get() -> Result<(), OpenAIError> {
        let client = OpenAI::standard_http(OPENAI_API_KEY.into(), Default::default());

        let response = client.responses::<False>().get(RESPONSE_ID.into()).await?;

        dbg!(&response);

        Ok(())
    }
}
