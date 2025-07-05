use ::serenity::all::ActivityData;
use dotenv::dotenv;
use poise::serenity_prelude as serenity;
use sqlx::postgres::PgPoolOptions;
use thiserror::Error;

#[derive(Error, Debug)]
enum Error {
    #[error("PostgreSQL Error")]
    Postgres(#[from] sqlx::Error),
    #[error("Discord Error")]
    Discord(#[from] serenity::Error),
}

// Required Struct for poise
struct Data {}

/// Says something
#[poise::command(slash_command, prefix_command)]
async fn knock(
    ctx: poise::Context<'_, Data, Box<dyn std::error::Error + Send + Sync>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    ctx.say("hello world :3").await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv().ok();
    let db = std::env::var("DATABASE_URL").expect("Missing Database URL");
    let token = std::env::var("DISCORD_TOKEN").expect("Missing Discord Token");
    let steam_key = std::env::var("STEAM_API_KEY").expect("Missing Steam API Key");

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
                Ok(Data {})
            })
        })
        .build();

    let pool = PgPoolOptions::new().max_connections(5).connect(&db).await?;
    let mut client = serenity::ClientBuilder::new(token, client_intents)
        .framework(client_framework)
        .await?;

    client.start().await?;
    Ok(())
}
