use ed25519_dalek::{PublicKey, Signature, Verifier, PUBLIC_KEY_LENGTH};
use hex::FromHex;
use once_cell::sync::Lazy;
use std::future::Future;
use twilight_model::application::{
    callback::{CallbackData, InteractionResponse},
    interaction::Interaction,
};

use hyper::{
    http::StatusCode,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server,
};

type GenericError = Box<dyn std::error::Error + Send + Sync>;

static PUB_KEY: Lazy<PublicKey> = Lazy::new(|| {
    PublicKey::from_bytes(&<[u8; PUBLIC_KEY_LENGTH] as FromHex>::from_hex("PUBLIC_KEY").unwrap())
        .unwrap()
});

async fn interaction_handler<F>(
    req: Request<Body>,
    f: impl Fn(Interaction) -> F,
) -> Result<Response<Body>, GenericError>
where
    F: Future<Output = Result<InteractionResponse, GenericError>>,
{
    if req.method() != Method::POST {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())?);
    }
    if req.uri().path() != "/" {
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())?);
    }

    let timestamp = if let Some(ts) = req.headers().get("x-signature-timestamp") {
        ts.to_owned()
    } else {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())?);
    };

    let signature = if let Some(hex_sig) = req.headers().get("x-signature-ed25519") {
        Signature::new(FromHex::from_hex(hex_sig)?)
    } else {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())?);
    };

    let whole_body = hyper::body::to_bytes(req).await?;

    if PUB_KEY
        .verify(
            vec![timestamp.as_bytes(), &whole_body].concat().as_ref(),
            &signature,
        )
        .is_err()
    {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())?);
    }
    println!("{}", String::from_utf8(whole_body.to_vec()).unwrap());

    let interaction = serde_json::from_slice::<Interaction>(&whole_body)?;

    match interaction {
        Interaction::Ping(_) => {
            let response = InteractionResponse::Pong;

            let json = serde_json::to_vec(&response)?;

            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(json.into())?)
        }
        Interaction::ApplicationCommand(_) => {
            let response = f(interaction).await?;

            let json = serde_json::to_vec(&response)?;

            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(json.into())?)
        }
        _ => Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())?),
    }
}

async fn handler(i: Interaction) -> Result<InteractionResponse, GenericError> {
    match i {
        Interaction::ApplicationCommand(ref cmd) => match cmd.data.name.as_ref() {
            "vroom" => vroom(i).await,
            "debug" => debug(i).await,
            _ => debug(i).await,
        },
        _ => Err("invalid interaction data".into()),
    }
}

async fn debug(i: Interaction) -> Result<InteractionResponse, GenericError> {
    Ok(InteractionResponse::ChannelMessageWithSource(
        CallbackData {
            allowed_mentions: None,
            flags: None,
            tts: None,
            content: Some(format!("```rust\n{:?}\n```", i)),
            embeds: Default::default(),
        },
    ))
}

async fn vroom(_: Interaction) -> Result<InteractionResponse, GenericError> {
    Ok(InteractionResponse::ChannelMessageWithSource(
        CallbackData {
            allowed_mentions: None,
            flags: None,
            tts: None,
            content: Some("Vroom vroom".to_owned()),
            embeds: Default::default(),
        },
    ))
}

#[tokio::main]
async fn main() -> Result<(), GenericError> {
    // Initialize the tracing subscriber.
    tracing_subscriber::fmt::init();

    let addr = "127.0.0.1:3030".parse().unwrap();

    let interaction_service = make_service_fn(|_| async {
        Ok::<_, GenericError>(service_fn(|req| interaction_handler(req, handler)))
    });

    let server = Server::bind(&addr).serve(interaction_service);

    server.await?;

    Ok(())
}
