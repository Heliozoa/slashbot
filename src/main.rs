mod poll;

use anyhow::Context as _;
use serenity::{
    async_trait,
    model::{application::interaction::Interaction, id::GuildId},
    prelude::*,
};
use std::{env, error::Error, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let discord_token = env::var("DISCORD_TOKEN").context("missing DISCORD_TOKEN")?;
    let application_id: u64 = env::var("APPLICATION_ID")
        .context("missing APPLICATION_ID")?
        .parse()
        .context("invalid APPLICATION_ID")?;
    let guild_id: u64 = env::var("GUILD_ID")
        .context("missing GUILD_ID")?
        .parse()
        .context("invalid GUILD_ID")?;

    let intents = GatewayIntents::GUILD_MESSAGES;

    let mut client = Client::builder(discord_token, intents)
        .event_handler(Handler {
            guild_id: GuildId(guild_id),
        })
        .application_id(application_id)
        .await?;

    tracing::info!("starting client");
    let _handle = tokio::spawn(poll::cleaner(
        Duration::from_secs(60),
        Duration::from_secs(60 * 5),
    ));
    client.start().await.context("failed to start client")?;

    Ok(())
}

struct Handler {
    guild_id: GuildId,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _data_about_bot: serenity::model::prelude::Ready) {
        poll::create(self.guild_id, &ctx)
            .await
            .context("Failed to create poll command")
            .unwrap();
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let res = match interaction {
            Interaction::ApplicationCommand(aci) => match aci.data.name.as_str() {
                poll::COMMAND => poll::start(&ctx, aci).await,
                _ => return,
            },
            Interaction::MessageComponent(mci) => {
                let msg = if let Some(mi) = mci.message.interaction.as_ref() {
                    &mi.name
                } else {
                    return;
                };
                match msg.as_str() {
                    poll::COMMAND => poll::vote(&ctx, mci).await,
                    _ => return,
                }
            }
            _ => return,
        };
        print_errors(&res);
    }
}

fn print_errors<T>(res: &anyhow::Result<T>) {
    if let Err(err) = res {
        err.chain().for_each(|e| tracing::error!("{}", e));
    }
}
