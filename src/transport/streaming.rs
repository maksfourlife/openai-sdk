use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use eventsource_stream::{Event, EventStream, EventStreamError, Eventsource};
use futures::{Stream, StreamExt, ready, stream::BoxStream};
use pin_project::pin_project;
use reqwest::{Method, header};
use serde::{Serialize, de::DeserializeOwned};
use snafu::{ResultExt, Snafu};

use crate::{OpenAIError, transport::StandardHttpTransport};

#[derive(Debug, Snafu)]
pub enum OpenAIStreamingError {
    #[snafu(display("Could not deserialize event data: {source}"))]
    DeserializeEventData {
        source: serde_json::Error,
        event: Event,
    },
    #[snafu(transparent)]
    EventStream {
        source: EventStreamError<reqwest::Error>,
    },
}

pub trait StreamingTransport {
    fn send<P, E>(
        &self,
        method: Method,
        path: &str,
        params: Option<&P>,
    ) -> impl Future<Output = Result<ParsedEventStream<E>, OpenAIError>> + Send
    where
        P: Sync + Serialize,
        E: Send + DeserializeOwned;
}

impl StreamingTransport for StandardHttpTransport {
    async fn send<P, E>(
        &self,
        method: Method,
        path: &str,
        params: Option<&P>,
    ) -> Result<ParsedEventStream<E>, OpenAIError>
    where
        P: Sync + Serialize,
        E: Send + DeserializeOwned,
    {
        let builder = self
            .prepare_request(method.clone(), path, params)?
            .header(header::ACCEPT, "text/event-stream");

        Ok(ParsedEventStream {
            inner: builder.send().await?.bytes_stream().boxed().eventsource(),
            _marker: PhantomData::<E>,
        })
    }
}

#[pin_project]
pub struct ParsedEventStream<T> {
    #[pin]
    pub(crate) inner: EventStream<BoxStream<'static, Result<Bytes, reqwest::Error>>>,
    _marker: PhantomData<T>,
}

impl<T> Stream for ParsedEventStream<T>
where
    T: DeserializeOwned,
{
    type Item = Result<T, OpenAIStreamingError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        Poll::Ready(ready!(this.inner.poll_next(cx)).map(|result| match result {
            Ok(event) => {
                serde_json::from_str(&event.data).context(DeserializeEventDataSnafu { event })
            }
            Err(err) => Err(err.into()),
        }))
    }
}
