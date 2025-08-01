use ::serenity::all::ActivityData;
use dotenv::dotenv;
use num_format::{Locale, ToFormattedString};
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions, types::Json};
use std::collections::HashMap;
use thiserror::Error;
use ureq::http::StatusCode;

static SERVER_IP: &str = "http://192.168.1.18";

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

#[derive(sqlx::FromRow, Debug)]
struct User {
    id: String,
    bank: i32,
    clean_money: i32,
    dirty_money: i32,
    staff: i16,
}

#[derive(sqlx::FromRow, Debug)]
struct Ban {
    id: i32,
    expiry: i64,
    reason: String,
    staff: String,
    user: String,
    active: bool,
}

#[derive(sqlx::FromRow, Debug)]
struct Warning {
    id: i32,
    reason: String,
    staff: String,
    user: String,
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

#[derive(Deserialize, Debug)]
struct ServerPlayerMoney {
    clean: i32,
    dirty: i32,
    bank: i32,
}

#[derive(Deserialize, Debug)]
struct ServerPlayer {
    id: String,
    active: bool,
    name: String,
    staff: i16,
    money: ServerPlayerMoney,
    items: HashMap<String, i64>,
    weapons: HashMap<String, String>,
    ammo: HashMap<String, i64>,
}

// Required Struct for poise
struct Data {
    steam_key: String,
    pg_pool: Pool<Postgres>,
    verify_id: String,
}

/// Lookup a Player in the Server - Restricted
#[poise::command(
    slash_command,
    prefix_command,
    guild_only = true,
    ephemeral = true,
    default_member_permissions = "ADMINISTRATOR"
)]
async fn lookup_id(
    ctx: poise::Context<'_, Data, Box<dyn std::error::Error + Send + Sync>>,
    #[description = "Target Player"] user: i64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    ctx.send(
        match ureq::get(format!("{}:30120/spectrum/users", SERVER_IP))
            .header("v", &ctx.data().verify_id)
            .header("id", user)
            .call()
        {
            Ok(response) => match response.status() {
                StatusCode::OK => match response.into_body().read_json::<ServerPlayer>() {
                    Ok(player) => poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title(format!(
                                "User Lookup (Server){}",
                                if player.active {
                                    String::new()
                                } else {
                                    String::from(" - Offline")
                                }
                            ))
                            .description(format!(
                                "**{}** (`{}`){}",
                                player.name,
                                player.id,
                                if player.staff > 0 {
                                    format!(" - Staff ({})", player.staff)
                                } else {
                                    String::new()
                                }
                            ))
                            .fields(vec![
                                (
                                    "Bank",
                                    format!(
                                        "${}",
                                        player.money.bank.to_formatted_string(&Locale::en_GB)
                                    ),
                                    true,
                                ),
                                (
                                    "Cash (Clean)",
                                    format!(
                                        "${}",
                                        player.money.clean.to_formatted_string(&Locale::en_GB)
                                    ),
                                    true,
                                ),
                                (
                                    "Cash (Dirty)",
                                    format!(
                                        "${}",
                                        player.money.dirty.to_formatted_string(&Locale::en_GB)
                                    ),
                                    true,
                                ),
                                (
                                    "Inventory",
                                    player
                                        .items
                                        .iter()
                                        .map(|(item, count)| format!("x{} {}", count, item))
                                        .collect::<Vec<String>>()
                                        .join("\n"),
                                    false,
                                ),
                                (
                                    "Weapons",
                                    player
                                        .weapons
                                        .iter()
                                        .map(|(id, weapon)| format!("{} ({})", weapon, id))
                                        .collect::<Vec<String>>()
                                        .join("\n"),
                                    false,
                                ),
                                (
                                    "Ammo",
                                    player
                                        .ammo
                                        .iter()
                                        .map(|(ammo_type, count)| {
                                            format!("{}: {}", ammo_type, count)
                                        })
                                        .collect::<Vec<String>>()
                                        .join("\n"),
                                    false,
                                ),
                            ]),
                    ),
                    Err(_) => poise::CreateReply::default().content("This player does not exist"),
                },
                _ => poise::CreateReply::default().content("This player does not exist"),
            },
            Err(_) => {
                poise::CreateReply::default().content("There was a problem reaching the server")
            }
        },
    )
    .await?;
    Ok(())
}

/// Lookup a Player from the Server - Restricted
#[poise::command(
    slash_command,
    prefix_command,
    guild_only = true,
    ephemeral = true,
    default_member_permissions = "ADMINISTRATOR"
)]
async fn lookup(
    ctx: poise::Context<'_, Data, Box<dyn std::error::Error + Send + Sync>>,
    #[description = "Target Player"] user: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    ctx.send(match parse_fivem_steam_id(user.clone()) {
        Ok(n) => match steam_user(n, &ctx.data().steam_key) {
            Ok(steam_user) => match sqlx::query_as(
                "select id, clean_money, dirty_money, bank, staff from users where id = $1;",
            )
            .bind(user.clone())
            .fetch_one(&ctx.data().pg_pool)
            .await
            {
                Ok(User {
                    id: _,
                    bank,
                    clean_money,
                    dirty_money,
                    staff,
                }) => poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("User Lookup")
                        .description(format!(
                            "**{}** (`{}`){}",
                            steam_user.name,
                            user,
                            if staff > 0 {
                                format!(" - Staff ({})", staff)
                            } else {
                                String::new()
                            }
                        ))
                        .fields(vec![
                            (
                                "Bank",
                                format!("${}", bank.to_formatted_string(&Locale::en_GB)),
                                true,
                            ),
                            (
                                "Cash (Clean)",
                                format!("${}", clean_money.to_formatted_string(&Locale::en_GB)),
                                true,
                            ),
                            (
                                "Cash (Dirty)",
                                format!("${}", dirty_money.to_formatted_string(&Locale::en_GB)),
                                true,
                            ),
                        ])
                        .thumbnail(steam_user.avatar),
                ),
                Err(_) => poise::CreateReply::default().content("This player does not exist"),
            },

            Err(_) => {
                poise::CreateReply::default().content("There was a problem fetching this player")
            }
        },
        Err(_) => poise::CreateReply::default().content("There was a problem fetching this player"),
    })
    .await?;
    Ok(())
}

/// Lookup a Player's Inventory - Restricted
#[poise::command(
    slash_command,
    prefix_command,
    guild_only = true,
    ephemeral = true,
    default_member_permissions = "ADMINISTRATOR"
)]
async fn inventory(
    ctx: poise::Context<'_, Data, Box<dyn std::error::Error + Send + Sync>>,
    #[description = "Target Player"] user: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    ctx.send(match parse_fivem_steam_id(user.clone()) {
        Ok(n) => match steam_user(n, &ctx.data().steam_key) {
            Ok(steam_user) => {
                match sqlx::query_scalar::<_, Json<HashMap<String, i64>>>(
                    "select inventory from users where id = $1;",
                )
                .bind(user.clone())
                .fetch_one(&ctx.data().pg_pool)
                .await
                {
                    Ok(items) => poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Inventory Lookup")
                            .description(format!("**{}** (`{}`)", steam_user.name, user))
                            .fields(
                                items
                                    .iter()
                                    .map(|(key, value)| (key, value.to_string(), true))
                                    .collect::<Vec<(&String, String, bool)>>(),
                            ),
                    ),
                    Err(_) => poise::CreateReply::default().content("This player does not exist"),
                }
            }

            Err(_) => {
                poise::CreateReply::default().content("There was a problem fetching this player")
            }
        },
        Err(_) => poise::CreateReply::default().content("There was a problem fetching this player"),
    })
    .await?;
    Ok(())
}

/// Lookup a Player's Bans & Warnings - Restricted
#[poise::command(
    slash_command,
    prefix_command,
    guild_only = true,
    ephemeral = true,
    default_member_permissions = "ADMINISTRATOR"
)]
async fn record(
    ctx: poise::Context<'_, Data, Box<dyn std::error::Error + Send + Sync>>,
    #[description = "Target Player"] user: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    ctx.send(match parse_fivem_steam_id(user.clone()) {
        Ok(n) => match steam_user(n, &ctx.data().steam_key) {
            Ok(steam_user) => match sqlx::query_as(
                "select id, clean_money, dirty_money, bank, staff from users where id = $1;",
            )
            .bind(user.clone())
            .fetch_one(&ctx.data().pg_pool)
            .await
            {
                Ok(User {
                    id,
                    bank: _,
                    clean_money: _,
                    dirty_money: _,
                    staff: _,
                }) => {
                    match (
                        sqlx::query_as::<_, Ban>("select * from bans where \"user\" = $1")
                            .bind(&id)
                            .fetch_all(&ctx.data().pg_pool)
                            .await,
                        sqlx::query_as::<_, Warning>("select * from warnings where \"user\" = $1")
                            .bind(&id)
                            .fetch_all(&ctx.data().pg_pool)
                            .await,
                    ) {
                        (Ok(bans), Ok(warnings)) => poise::CreateReply::default().embed(
                            serenity::CreateEmbed::new()
                                .title(if bans.len() + warnings.len() > 0 {
                                    "Record Lookup"
                                } else {
                                    "Clean Record"
                                })
                                .description(format!("**{}** (`{}`)", steam_user.name, user))
                                .fields(bans.iter().map(
                                    |Ban {
                                         id,
                                         expiry,
                                         reason,
                                         staff,
                                         user: _,
                                         active,
                                     }| {
                                        (
                                            format!("Banned By: {}", staff),
                                            format!(
                                                "ID: {}{}\nReason: {}\nExpires: <t:{}>",
                                                id,
                                                if *active { "" } else { " (Lifted)" },
                                                reason,
                                                expiry
                                            ),
                                            false,
                                        )
                                    },
                                ))
                                .fields(warnings.iter().map(
                                    |Warning {
                                         id,
                                         reason,
                                         staff,
                                         user: _,
                                     }| {
                                        (
                                            format!("Warned By: {}", staff),
                                            format!("ID: {}\nReason: {}", id, reason),
                                            false,
                                        )
                                    },
                                ))
                                .thumbnail(steam_user.avatar),
                        ),
                        _ => poise::CreateReply::default().embed(
                            serenity::CreateEmbed::new()
                                .title("Record Lookup")
                                .description(format!(
                                    "**{}** (`{}`)\n\nThis player has a squeaky clean record",
                                    steam_user.name, user,
                                ))
                                .thumbnail(steam_user.avatar),
                        ),
                    }
                }
                Err(_) => poise::CreateReply::default().content("This player does not exist"),
            },

            Err(_) => {
                poise::CreateReply::default().content("There was a problem fetching this player")
            }
        },
        Err(_) => poise::CreateReply::default().content("There was a problem fetching this player"),
    })
    .await?;
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
    let verify_id = std::env::var("VERIFY_ID").expect("Missing Bridge Verify ID");

    let pool = PgPoolOptions::new().max_connections(5).connect(&db).await?;

    let client_intents = serenity::GatewayIntents::non_privileged();
    let client_framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![lookup_id(), lookup(), inventory(), record()],
            ..Default::default()
        })
        .setup(|ctx, _, framework| {
            ctx.set_activity(Some(ActivityData::watching("Spectrum <3")));
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    steam_key: steam_key,
                    pg_pool: pool,
                    verify_id,
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
