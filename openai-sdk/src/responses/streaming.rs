use reqwest::Method;
use serde_bool::True;

use crate::{
    OpenAIError,
    models::responses::{ResponseIdRef, streaming::ResponseEvent},
    responses::{ResponseParams, ResponsesHandler},
    transport::streaming::{ParsedEventStream, StreamingTransport},
};

impl<T: StreamingTransport> ResponsesHandler<'_, T, True> {
    pub async fn create(
        &self,
        params: &ResponseParams<True>,
    ) -> Result<ParsedEventStream<ResponseEvent>, OpenAIError> {
        self.client
            .transport
            .send(Method::POST, "/v1/responses", Some(params))
            .await
    }

    pub async fn get(
        &self,
        id: &ResponseIdRef,
    ) -> Result<ParsedEventStream<ResponseEvent>, OpenAIError> {
        self.client
            .transport
            .send(Method::GET, &format!("/v1/responses/{id}"), None::<&()>)
            .await
    }
}

#[cfg(test)]
mod test {
    use dotenv_codegen::dotenv;
    use futures::StreamExt;
    use serde_bool::True;

    use crate::{OpenAI, OpenAIError, models::responses::ResponseInput, responses::ResponseParams};

    const OPENAI_API_KEY: &str = dotenv!("OPENAI_API_KEY");
    const RESPONSE_ID: &str = dotenv!("RESPONSE_ID");

    #[tokio::test]
    async fn test_get() -> Result<(), OpenAIError> {
        let client = OpenAI::standard_http(OPENAI_API_KEY.into(), Default::default());

        let mut response = client.responses::<True>().get(RESPONSE_ID.into()).await?;

        while let Some(event) = response.inner.next().await {
            let _ = dbg!(event);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_create() -> Result<(), OpenAIError> {
        let client = OpenAI::standard_http(OPENAI_API_KEY.into(), Default::default());

        let params = ResponseParams {
            background: Some(true),
            input: Some(ResponseInput::Text("Hello".to_string())),
            stream: True,
        };

        let mut response = client.responses::<True>().create(&params).await?;

        while let Some(event) = response.inner.next().await {
            let _ = dbg!(event);
        }

        Ok(())
    }
}
