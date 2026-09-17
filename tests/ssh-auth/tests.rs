use crate::authentication::{AuthenticationChallenge, authenticate};
use anyhow::Result;
use futures::FutureExt as _;
use russh::{
    MethodKind, MethodSet, client,
    keys::{Algorithm, PrivateKey, PublicKey, ssh_key::rand_core::OsRng},
    server,
};
use std::{
    borrow::Cow,
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

enum Exchange {
    None(server::Auth),
    Password(&'static str, server::Auth),
    Key(server::Auth),
    Keyboard(Option<Vec<&'static str>>, server::Auth),
}

struct Server(Arc<Mutex<VecDeque<Exchange>>>);
impl server::Handler for Server {
    type Error = anyhow::Error;
    async fn auth_none(&mut self, _: &str) -> Result<server::Auth> {
        match self.0.lock().unwrap().pop_front().unwrap() {
            Exchange::None(result) => Ok(result),
            _ => panic!("unexpected none authentication"),
        }
    }
    async fn auth_password(&mut self, _: &str, password: &str) -> Result<server::Auth> {
        match self.0.lock().unwrap().pop_front().unwrap() {
            Exchange::Password(expected, result) => {
                assert_eq!(password, expected);
                Ok(result)
            }
            _ => panic!("unexpected password authentication"),
        }
    }
    async fn auth_publickey(&mut self, _: &str, _: &PublicKey) -> Result<server::Auth> {
        match self.0.lock().unwrap().pop_front().unwrap() {
            Exchange::Key(result) => Ok(result),
            _ => panic!("unexpected public key authentication"),
        }
    }
    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _: &str,
        _: &str,
        response: Option<server::Response<'a>>,
    ) -> Result<server::Auth> {
        let response = response.map(|responses| {
            responses
                .map(|response| String::from_utf8(response.to_vec()).unwrap())
                .collect::<Vec<_>>()
        });
        match self.0.lock().unwrap().pop_front().unwrap() {
            Exchange::Keyboard(expected, result) => {
                assert_eq!(
                    response,
                    expected.map(|responses| responses.into_iter().map(str::to_owned).collect())
                );
                Ok(result)
            }
            _ => panic!("unexpected keyboard-interactive authentication"),
        }
    }
}

struct Client;
impl client::Handler for Client {
    type Error = anyhow::Error;
    async fn check_server_key(&mut self, _: &PublicKey) -> Result<bool> {
        Ok(true)
    }
}

#[derive(Debug, PartialEq)]
struct Challenge {
    name: String,
    instructions: String,
    prompts: Vec<(String, bool)>,
}
impl From<&AuthenticationChallenge> for Challenge {
    fn from(challenge: &AuthenticationChallenge) -> Self {
        Self {
            name: challenge.name().into(),
            instructions: challenge.instructions().into(),
            prompts: challenge
                .prompts()
                .iter()
                .map(|prompt| (prompt.text().into(), prompt.echo()))
                .collect(),
        }
    }
}

fn reject(methods: &[MethodKind], partial_success: bool) -> server::Auth {
    server::Auth::Reject {
        proceed_with_methods: Some(MethodSet::from(methods)),
        partial_success,
    }
}
fn challenge(
    name: &'static str,
    instructions: &'static str,
    prompts: &[(&'static str, bool)],
) -> server::Auth {
    server::Auth::Partial {
        name: name.into(),
        instructions: instructions.into(),
        prompts: Cow::Owned(
            prompts
                .iter()
                .map(|(text, echo)| (Cow::Borrowed(*text), *echo))
                .collect(),
        ),
    }
}

async fn scenario(
    script: Vec<Exchange>,
    keys: Vec<Arc<PrivateKey>>,
    password: Option<&str>,
    answers: Vec<Result<Vec<&str>>>,
) -> (Result<()>, Vec<Challenge>) {
    let exchanges = Arc::new(Mutex::new(VecDeque::from(script)));
    let config = Arc::new(server::Config {
        keys: vec![PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap()],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let (client_stream, server_stream) = tokio::io::duplex(65536);
    let handler = Server(exchanges.clone());
    let server = tokio::spawn(async move {
        server::run_stream(config, server_stream, handler)
            .await?
            .await
    });
    let mut handle =
        client::connect_stream(Arc::new(client::Config::default()), client_stream, Client)
            .await
            .unwrap();
    let answers = Arc::new(Mutex::new(VecDeque::from(
        answers
            .into_iter()
            .map(|answer| {
                answer.map(|responses| responses.into_iter().map(str::to_owned).collect::<Vec<_>>())
            })
            .collect::<Vec<_>>(),
    )));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        authenticate(
            &mut handle,
            "test-user",
            keys,
            password.map(str::to_owned),
            {
                let observed = observed.clone();
                let answers = answers.clone();
                move |challenge| {
                    observed.lock().unwrap().push(Challenge::from(&challenge));
                    let answer = answers
                        .lock()
                        .unwrap()
                        .pop_front()
                        .expect("unexpected UI prompt");
                    async move { answer }.boxed()
                }
            },
        ),
    )
    .await
    .expect("authentication stalled");
    handle
        .disconnect(russh::Disconnect::ByApplication, "test complete", "en")
        .await
        .unwrap();
    // russh 0.58 waits exclusively for responses during a keyboard challenge.
    // Dropping the handle closes that wait without sending any response.
    drop(handle);
    let server_result = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    if result.is_ok() {
        server_result.unwrap();
    }
    assert!(
        exchanges.lock().unwrap().is_empty(),
        "authentication stopped before completing the required factors"
    );
    assert!(
        answers.lock().unwrap().is_empty(),
        "not all expected prompts were presented"
    );
    let observed = Arc::try_unwrap(observed).unwrap().into_inner().unwrap();
    (result, observed)
}

use Exchange::*;
use MethodKind::{
    HostBased, KeyboardInteractive, Password as PasswordMethod, PublicKey as PublicKeyMethod,
};

#[tokio::test]
async fn saved_password_only() {
    let (result, prompts) = scenario(
        vec![
            None(reject(&[PasswordMethod], false)),
            Password("saved", server::Auth::Accept),
        ],
        vec![],
        Some("saved"),
        vec![],
    )
    .await;
    result.unwrap();
    assert!(prompts.is_empty());
}

#[tokio::test]
async fn rejected_saved_password_prompts_for_replacement() {
    let (result, prompts) = scenario(
        vec![
            None(reject(&[PasswordMethod], false)),
            Password("old", reject(&[PasswordMethod], false)),
            Password("new", server::Auth::Accept),
        ],
        vec![],
        Some("old"),
        vec![Ok(vec!["new"])],
    )
    .await;
    result.unwrap();
    assert_eq!(prompts[0].prompts, vec![("Password:".into(), false)]);
}

#[tokio::test]
async fn pam_password_is_requested_without_replaying_saved_password() {
    let (result, prompts) = scenario(
        vec![
            None(reject(&[KeyboardInteractive], false)),
            Keyboard(
                Option::None,
                challenge(
                    "PAM",
                    "Enter your account password",
                    &[("Password: ", false)],
                ),
            ),
            Keyboard(Some(vec!["entered"]), server::Auth::Accept),
        ],
        vec![],
        Some("never-send-this"),
        vec![Ok(vec!["entered"])],
    )
    .await;
    result.unwrap();
    assert_eq!(prompts[0].name, "PAM");
    assert_eq!(prompts[0].prompts, vec![("Password: ".into(), false)]);
}

#[tokio::test]
async fn multiple_rounds_echo_flags_and_empty_prompts_preserve_response_order() {
    let (result, prompts) = scenario(
        vec![
            None(reject(&[KeyboardInteractive], false)),
            Keyboard(Option::None, challenge("", "", &[])),
            Keyboard(
                Some(vec![]),
                challenge(
                    "Sign in",
                    "Enter both fields",
                    &[("User:", true), ("", false), ("OTP:", false)],
                ),
            ),
            Keyboard(
                Some(vec!["alice", "", "123456"]),
                challenge("Next factor", "", &[("Recovery code:", false)]),
            ),
            Keyboard(Some(vec!["recovery"]), server::Auth::Accept),
        ],
        vec![],
        Some("not-an-otp"),
        vec![Ok(vec!["alice", "123456"]), Ok(vec!["recovery"])],
    )
    .await;
    result.unwrap();
    assert_eq!(prompts.len(), 2);
    assert_eq!(
        prompts[0].prompts,
        vec![("User:".into(), true), ("OTP:".into(), false)]
    );
    assert_eq!(prompts[0].instructions, "Enter both fields");
}

#[tokio::test]
async fn zero_prompt_instructions_are_shown() {
    let (result, prompts) = scenario(
        vec![
            None(reject(&[KeyboardInteractive], false)),
            Keyboard(
                Option::None,
                challenge("Security key", "Touch the key, then continue", &[]),
            ),
            Keyboard(Some(vec![]), server::Auth::Accept),
        ],
        vec![],
        Option::None,
        vec![Ok(vec![])],
    )
    .await;
    result.unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].prompts.is_empty());
}

#[tokio::test]
async fn rejected_credentials_stop_after_bounded_retries() {
    let (result, _) = scenario(
        vec![
            None(reject(&[KeyboardInteractive], false)),
            Keyboard(Option::None, challenge("", "", &[("OTP:", false)])),
            Keyboard(Some(vec!["wrong"]), reject(&[KeyboardInteractive], false)),
            Keyboard(Option::None, challenge("", "", &[("OTP:", false)])),
            Keyboard(Some(vec!["wrong"]), reject(&[KeyboardInteractive], false)),
        ],
        vec![],
        Option::None,
        vec![Ok(vec!["wrong"]), Ok(vec!["wrong"])],
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("authentication was rejected")
    );
}

#[tokio::test]
async fn cancellation_sends_no_responses_and_does_not_retry() {
    let (result, _) = scenario(
        vec![
            None(reject(&[KeyboardInteractive], false)),
            Keyboard(Option::None, challenge("", "", &[("OTP:", false)])),
        ],
        vec![],
        Some("never-send"),
        vec![Err(anyhow::anyhow!("SSH authentication cancelled"))],
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("cancelled"));
}

#[tokio::test]
async fn unsupported_method_is_not_reported_as_wrong_password() {
    let (result, prompts) = scenario(
        vec![None(reject(&[HostBased], false))],
        vec![],
        Some("unused"),
        vec![],
    )
    .await;
    let message = result.unwrap_err().to_string();
    assert!(message.contains("No supported SSH authentication"));
    assert!(message.contains("hostbased"));
    assert!(prompts.is_empty());
}

#[tokio::test]
async fn public_key_password_and_keyboard_factors_follow_server_methods() {
    let key = Arc::new(PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap());
    let (result, _) = scenario(
        vec![
            None(reject(&[PublicKeyMethod], false)),
            Key(reject(&[PasswordMethod], true)),
            Password("saved", reject(&[KeyboardInteractive], true)),
            Keyboard(Option::None, challenge("", "", &[("OTP:", false)])),
            Keyboard(Some(vec!["654321"]), server::Auth::Accept),
        ],
        vec![key],
        Some("saved"),
        vec![Ok(vec!["654321"])],
    )
    .await;
    result.unwrap();
}

#[tokio::test]
async fn partial_success_with_unavailable_next_factor_is_incomplete() {
    // The russh 0.58 test server preserves partial_success for keyboard-interactive;
    // its password/public-key handlers clear that flag before writing the packet.
    let (result, _) = scenario(
        vec![
            None(reject(&[KeyboardInteractive], false)),
            Keyboard(Option::None, challenge("", "", &[("OTP:", false)])),
            Keyboard(Some(vec!["123456"]), reject(&[HostBased], true)),
        ],
        vec![],
        Option::None,
        vec![Ok(vec!["123456"])],
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("needs another factor")
    );
}

#[tokio::test]
async fn response_count_mismatch_is_rejected_before_sending() {
    let (result, _) = scenario(
        vec![
            None(reject(&[KeyboardInteractive], false)),
            Keyboard(
                Option::None,
                challenge("", "", &[("User:", true), ("OTP:", false)]),
            ),
        ],
        vec![],
        Option::None,
        vec![Ok(vec!["only-one"])],
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("invalid response count")
    );
}

#[tokio::test]
async fn none_authentication_success_needs_no_prompt() {
    let (result, prompts) = scenario(
        vec![None(server::Auth::Accept)],
        vec![],
        Option::None,
        vec![],
    )
    .await;
    result.unwrap();
    assert!(prompts.is_empty());
}

#[tokio::test]
async fn password_rejection_can_fall_back_to_keyboard_interactive() {
    let methods = &[PasswordMethod, KeyboardInteractive];
    let (result, prompts) = scenario(
        vec![
            None(reject(methods, false)),
            Password("saved", reject(methods, false)),
            Keyboard(Option::None, challenge("PAM", "", &[("Password:", false)])),
            Keyboard(Some(vec!["entered"]), server::Auth::Accept),
        ],
        vec![],
        Some("saved"),
        vec![Ok(vec!["entered"])],
    )
    .await;
    result.unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].name, "PAM");
}

#[tokio::test]
async fn partial_success_resets_attempts_for_the_next_factor() {
    let (result, _) = scenario(
        vec![
            None(reject(&[KeyboardInteractive], false)),
            Keyboard(Option::None, challenge("", "", &[("First factor:", false)])),
            Keyboard(Some(vec!["wrong"]), reject(&[KeyboardInteractive], false)),
            Keyboard(Option::None, challenge("", "", &[("First factor:", false)])),
            Keyboard(Some(vec!["accepted"]), reject(&[KeyboardInteractive], true)),
            Keyboard(
                Option::None,
                challenge("", "", &[("Second factor:", false)]),
            ),
            Keyboard(Some(vec!["next"]), server::Auth::Accept),
        ],
        vec![],
        Option::None,
        vec![Ok(vec!["wrong"]), Ok(vec!["accepted"]), Ok(vec!["next"])],
    )
    .await;
    result.unwrap();
}

#[tokio::test]
async fn password_prompt_can_be_cancelled_without_an_authentication_attempt() {
    let (result, _) = scenario(
        vec![None(reject(&[PasswordMethod], false))],
        vec![],
        Option::None,
        vec![Err(anyhow::anyhow!("SSH authentication cancelled"))],
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("cancelled"));
}
