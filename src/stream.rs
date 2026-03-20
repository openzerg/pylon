use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};
use std::convert::Infallible;
use reqwest::Response;

pub async fn stream_response(
    response: Response,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut stream = response.bytes_stream();
        
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for line in text.lines() {
                        if line.is_empty() {
                            continue;
                        }
                        
                        if line == "data: [DONE]" {
                            yield Ok(Event::default().data("[DONE]"));
                            continue;
                        }
                        
                        if let Some(data) = line.strip_prefix("data: ") {
                            yield Ok(Event::default().data(data));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Stream error: {}", e);
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}