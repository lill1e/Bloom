use ::serenity::all::ActivityData;
use dotenv::dotenv;
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use thiserror::Error;

#[derive(Error, Debug)]
enum Error {
    #[error("PostgreSQL Error")]
    Postgres(#[from] sqlx::Error),
    #[error("Discord Error")]
    Discord(#[from] serenity::Error),
    #[error("Steam Formatting Error")]
    SteamFmt(),
    #[error("Steam API Error")]
    Steam(#[from] ureq::Error),
}

#[derive(Deserialize, Debug)]
struct SteamResponse {
    response: SteamResponseInner,
}

#[derive(Deserialize, Debug)]
struct SteamResponseInner {
    players: Vec<SteamResponsePlayer>,
}

#[derive(Deserialize, Debug)]
struct SteamResponsePlayer {
    steamid: String,
    personaname: String,
    avatarfull: String,
}

struct SteamUser {
    id: String,
    name: String,
    avatar: String,
}

impl SteamUser {
    fn new(resp: SteamResponse) -> Self {
        return SteamUser {
            id: resp.response.players[0].steamid.clone(),
            name: resp.response.players[0].personaname.clone(),
            avatar: resp.response.players[0].avatarfull.clone(),
        };
    }
}

// Required Struct for poise
struct Data {
    steam_key: String,
    pg_pool: Pool<Postgres>,
}

/// Says something
#[poise::command(slash_command, prefix_command)]
async fn knock(
    ctx: poise::Context<'_, Data, Box<dyn std::error::Error + Send + Sync>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    ctx.say("hello world :3").await?;
    Ok(())
}

fn parse_fivem_steam_id(id: String) -> Result<u64, Error> {
    let mut s = id.split(":");
    match s.next() {
        Some("steam") => match s.next() {
            Some(s) => u64::from_str_radix(s, 16).map_err(|_| Error::SteamFmt()),
            _ => Err(Error::SteamFmt()),
        },
        _ => Err(Error::SteamFmt()),
    }
}

fn steam_user(steam_id: u64, steam_key: &str) -> Result<SteamUser, Error> {
    return Ok(SteamUser::new(ureq::get(format!(
        "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/?key={}&format=json&steamids={}",
       steam_key, steam_id
    )).call()?.into_body().read_json()?));
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv().ok();
    let db = std::env::var("DATABASE_URL").expect("Missing Database URL");
    let token = std::env::var("DISCORD_TOKEN").expect("Missing Discord Token");
    let steam_key = std::env::var("STEAM_API_KEY").expect("Missing Steam API Key");

    let pool = PgPoolOptions::new().max_connections(5).connect(&db).await?;

    let client_intents = serenity::GatewayIntents::non_privileged();
    let client_framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![knock()],
            ..Default::default()
        })
        .setup(|ctx, _, framework| {
            ctx.set_activity(Some(ActivityData::watching("Spectrum <3")));
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    steam_key: steam_key,
                    pg_pool: pool,
                })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(token, client_intents)
        .framework(client_framework)
        .await?;

    client.start().await?;
    Ok(())
}
