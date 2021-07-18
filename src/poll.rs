use anyhow::Context as _;
use once_cell::sync::Lazy;
use serenity::{
    builder::{CreateActionRow, CreateButton},
    model::{
        id::{GuildId, InteractionId, UserId},
        interactions::{
            ApplicationCommand, ApplicationCommandInteractionData, ApplicationCommandOptionType,
            ButtonStyle, Component, Interaction, InteractionMessage, InteractionResponseType,
            MessageComponent,
        },
    },
    prelude::*,
};
use std::{collections::HashMap, time::Duration};
use tokio::time::Instant;

pub const COMMAND: &str = "poll";

pub async fn create(guild_id: GuildId, ctx: &Context) -> anyhow::Result<ApplicationCommand> {
    let res = guild_id
        .create_application_command(&ctx, |command| {
            command
                .name(COMMAND)
                .description("a simple poll command")
                .create_option(|option| {
                    option
                        .name("options")
                        .kind(ApplicationCommandOptionType::String)
                        .description("comma separated list of options")
                        .required(true)
                })
        })
        .await
        .context("failed to create poll command")?;
    Ok(res)
}

pub async fn start(
    ctx: &Context,
    interaction: &Interaction,
    command: &ApplicationCommandInteractionData,
) -> anyhow::Result<()> {
    // collect and validate poll options
    let mut options = command
        .options
        .iter()
        .find(|o| o.name == "options")
        .context("missing options")?
        .value
        .as_ref()
        .context("missing options value")?
        .as_str()
        .context("invalid options value")?
        .split(",")
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if options.is_empty() {
        anyhow::bail!("no options");
    }
    options.sort();
    options.dedup();

    // poll data is stored in a static to be accessed for voting and cleanup
    let poll_data = PollData {
        start_time: Instant::now(),
        options: options.iter().copied().map(String::from).collect(),
        votes: HashMap::new(),
    };

    // respond with poll
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|response_data| {
                    response_data
                        .content(create_content(&poll_data))
                        .components(|components| {
                            // create voting buttons
                            let mut row = CreateActionRow::default();
                            for option in options.iter().copied() {
                                row.add_button(create_vote_button(option, 0));
                            }
                            components.add_action_row(row)
                        })
                })
        })
        .await
        .context("failed to create response")?;

    // on success, store poll data
    let mut lock = POLLS.write().await;
    lock.insert(interaction.id, poll_data);
    Ok(())
}

pub async fn vote(
    ctx: &Context,
    interaction: &Interaction,
    interaction_id: InteractionId,
    component: &MessageComponent,
) -> anyhow::Result<()> {
    // save the user's vote in the poll data
    let mut lock = POLLS.write().await;
    let poll_data = lock
        .get_mut(&interaction_id)
        .context("unexpected interaction id")?;
    let user_id = interaction
        .member
        .as_ref()
        .context("missing member")?
        .user
        .id;
    poll_data.votes.insert(user_id, component.custom_id.clone());

    // create updated buttons
    let mut row = CreateActionRow::default();
    let msg = interaction
        .message
        .as_ref()
        .context("missing message in interaction")?;
    // the first (and only) action row should contain only the voting buttons
    let button_row = if let InteractionMessage::Regular(msg) = msg {
        msg.components.first().context("missing action row")?
    } else {
        anyhow::bail!("invalid message type in interaction");
    };
    if let Component::ActionRow(button_row) = button_row {
        for component in button_row.components.iter() {
            if let Component::Button(b) = component {
                let custom_id = b.custom_id.as_ref().context("missing custom id")?;
                let votes = poll_data.votes_for(custom_id);
                row.add_button(create_vote_button(custom_id, votes));
            } else {
                anyhow::bail!("unexpected component");
            }
        }
    } else {
        anyhow::bail!("invalid component");
    }

    // update the message
    interaction
        .create_interaction_response(ctx, |response| {
            response
                .kind(InteractionResponseType::UpdateMessage)
                .interaction_response_data(|response_data| {
                    response_data
                        .content(create_content(&poll_data))
                        .components(|c| c.set_action_rows(vec![row]))
                })
        })
        .await?;
    Ok(())
}

/// Periodically removes old poll data from memory
pub async fn cleaner(interval: Duration, poll_duration: Duration) {
    let mut interval = tokio::time::interval(interval);
    loop {
        interval.tick().await;
        let mut lock = POLLS.write().await;
        let mut remove = vec![];
        for (key, val) in lock.iter() {
            if val.start_time.elapsed() > poll_duration {
                remove.push(*key);
            }
        }
        for target in remove {
            lock.remove(&target);
        }
    }
}

fn create_content(poll_data: &PollData) -> String {
    format!("Vote:\n{}", poll_data.options.join(","))
}

static POLLS: Lazy<RwLock<HashMap<InteractionId, PollData>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

struct PollData {
    start_time: Instant,
    options: Vec<String>,
    votes: HashMap<UserId, String>,
}

impl PollData {
    fn votes_for(&self, vote_id: &str) -> u32 {
        let mut votes = 0;
        for vote in self.votes.values() {
            if vote == vote_id {
                votes += 1;
            }
        }
        votes
    }
}

fn create_vote_button(option: &str, votes: u32) -> CreateButton {
    let mut button = CreateButton::default();
    button
        .custom_id(option)
        .label(format!("{}: {}", option, votes))
        .style(ButtonStyle::Primary);
    button
}
