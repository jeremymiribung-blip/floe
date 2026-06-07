use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{ErrorResponse, Request, Response},
        Message,
    },
};

use super::{ClientMessage, LocalAsrEvent, MockAsrScenario, SIDE_CAR_PORT_PREFIX};

const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(750);
const FINAL_TEXT: &str = "Mock local ASR final transcript.";

pub async fn run_mock_sidecar(
    token: String,
    scenario: MockAsrScenario,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    println!("{SIDE_CAR_PORT_PREFIX}{port}");

    let Ok((stream, _addr)) = listener.accept().await else {
        return Ok(());
    };

    handle_connection(stream, token, scenario).await;
    Ok(())
}

#[allow(clippy::result_large_err)]
async fn handle_connection(stream: TcpStream, token: String, scenario: MockAsrScenario) {
    let token_for_check = token.clone();
    let ws_stream = accept_hdr_async(stream, move |request: &Request, response: Response| {
        if authorized(request, &token_for_check) {
            Ok(response)
        } else {
            Err(http_response(401, "Unauthorized"))
        }
    })
    .await;

    let Ok(ws_stream) = ws_stream else {
        return;
    };
    let (mut writer, mut reader) = ws_stream.split();

    if send_event(&mut writer, LocalAsrEvent::Ready).await.is_err() {
        return;
    }

    if matches!(scenario, MockAsrScenario::CrashDisconnect) {
        return;
    }

    if !matches!(
        scenario,
        MockAsrScenario::NoHeartbeat | MockAsrScenario::Timeout
    ) && send_event(&mut writer, LocalAsrEvent::Heartbeat)
        .await
        .is_err()
    {
        return;
    }

    let mut started = false;
    let mut audio_frames = 0_usize;
    while let Some(message) = reader.next().await {
        match message {
            Ok(Message::Text(text)) => {
                let Ok(client_message) = serde_json::from_str::<ClientMessage>(&text) else {
                    let _ = send_event(
                        &mut writer,
                        LocalAsrEvent::Error {
                            code: "malformed_event".to_string(),
                        },
                    )
                    .await;
                    return;
                };

                match client_message {
                    ClientMessage::StartSession {
                        sample_rate,
                        channels,
                        format,
                        ..
                    } if sample_rate == 16_000 && channels == 1 && format == "pcm_s16le" => {
                        started = true;
                        match scenario {
                            MockAsrScenario::ModelMissing => {
                                let _ = send_event(
                                    &mut writer,
                                    LocalAsrEvent::Error {
                                        code: "model_missing".to_string(),
                                    },
                                )
                                .await;
                                return;
                            }
                            MockAsrScenario::Busy => {
                                let _ = send_event(
                                    &mut writer,
                                    LocalAsrEvent::Error {
                                        code: "busy".to_string(),
                                    },
                                )
                                .await;
                                return;
                            }
                            MockAsrScenario::MalformedEvent => {
                                let _ = writer
                                    .send(Message::Text(
                                        "{\"type\":\"final_transcript\"".to_string(),
                                    ))
                                    .await;
                                return;
                            }
                            _ => {}
                        }
                    }
                    ClientMessage::StartSession { .. } => {
                        let _ = send_event(
                            &mut writer,
                            LocalAsrEvent::Error {
                                code: "malformed_event".to_string(),
                            },
                        )
                        .await;
                        return;
                    }
                    ClientMessage::EndOfAudio => {
                        if !started || matches!(scenario, MockAsrScenario::Timeout) {
                            continue;
                        }
                        if matches!(scenario, MockAsrScenario::SlowFinal) {
                            tokio::time::sleep(Duration::from_millis(900)).await;
                        }
                        let _ = send_event(
                            &mut writer,
                            LocalAsrEvent::PartialTranscript {
                                text: "internal partial must never be pasted".to_string(),
                                stable: false,
                            },
                        )
                        .await;
                        let _ = send_event(
                            &mut writer,
                            LocalAsrEvent::FinalTranscript {
                                text: FINAL_TEXT.to_string(),
                                stable: true,
                            },
                        )
                        .await;
                        return;
                    }
                    ClientMessage::CancelSession => return,
                }
            }
            Ok(Message::Binary(frame)) if frame.len() >= 16 => {
                audio_frames = audio_frames.saturating_add(1);
                if audio_frames == 1
                    && !matches!(
                        scenario,
                        MockAsrScenario::NoHeartbeat | MockAsrScenario::Timeout
                    )
                {
                    let _ = send_event(&mut writer, LocalAsrEvent::Heartbeat).await;
                }
            }
            Ok(Message::Close(_)) | Err(_) => return,
            _ => {}
        }

        if matches!(
            scenario,
            MockAsrScenario::Success | MockAsrScenario::SlowFinal
        ) {
            let _ = tokio::time::timeout(HEARTBEAT_INTERVAL, std::future::ready(())).await;
        }
    }
}

fn authorized(request: &Request, token: &str) -> bool {
    request.uri().host().is_none_or(|host| host == "127.0.0.1")
        && request.uri().query().is_some_and(|query| {
            query
                .split('&')
                .any(|part| part == format!("token={token}"))
        })
}

fn http_response(status: u16, body: &'static str) -> ErrorResponse {
    Response::builder()
        .status(status)
        .body(Some(body.to_string()))
        .expect("static response should build")
}

async fn send_event<S>(
    writer: &mut S,
    event: LocalAsrEvent,
) -> Result<(), tokio_tungstenite::tungstenite::Error>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    writer
        .send(Message::Text(
            serde_json::to_string(&event).unwrap_or_else(|_| "{\"type\":\"error\"}".to_string()),
        ))
        .await
}

#[cfg(test)]
mod tests {
    use super::authorized;
    use tokio_tungstenite::tungstenite::handshake::server::Request;

    #[test]
    fn token_is_required_in_query_string() {
        let valid = Request::builder()
            .uri("/asr?token=secret")
            .body(())
            .unwrap();
        let invalid = Request::builder().uri("/asr").body(()).unwrap();

        assert!(authorized(&valid, "secret"));
        assert!(!authorized(&invalid, "secret"));
    }
}
